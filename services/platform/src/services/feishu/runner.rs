use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    error::ApiError,
    models::{SyncRunKind, ValidationIssue},
    state::AppState,
};

use super::runner_support::{
    finalize_run, increment_table_failure, next_source_row_number, persist_cursor,
    report_suspected_missing, runtime_table_mapping, staging_exists,
};
use super::{
    compensate_source_assets, discover_sources, ensure_mapping, get_feishu_settings, load_mapping,
    load_sync_run, normalize_record, open_cursor, promote_record, record_sync_error,
    stage_invalid_record, store_source_assets, FeishuResumeCursor, PromotionOutcome,
};

pub async fn execute_sync_run(state: &AppState, run_id: Uuid) -> Result<Value, String> {
    execute_sync_run_inner(state, run_id)
        .await
        .map_err(|error| error.to_string())
}

async fn execute_sync_run_inner(state: &AppState, run_id: Uuid) -> Result<Value, ApiError> {
    let run = load_sync_run(state, run_id).await?;
    let connector_id = run
        .connector_id
        .ok_or_else(|| ApiError::service_unavailable("Feishu run has no connector."))?;
    let settings = get_feishu_settings(state).await?;
    let key = state
        .config
        .product_staging_encryption_key
        .as_ref()
        .ok_or_else(|| ApiError::service_unavailable("Product staging encryption is missing."))?;
    let import_checksum = format!("feishu-run:{run_id}");
    sqlx::query("UPDATE sync_runs SET status='fetching' WHERE id=$1")
        .bind(run_id)
        .execute(&state.pool)
        .await?;
    sqlx::query("UPDATE product_import_runs SET status='receiving' WHERE id=$1")
        .bind(run_id)
        .execute(&state.pool)
        .await?;

    let stored_mapping = load_mapping(state, connector_id, &settings.mapping_version).await?;
    let initial_discovery = if stored_mapping.is_none() {
        Some(discover_sources(&state.feishu_client, &settings.sources).await?)
    } else {
        None
    };
    let mapping = match (&stored_mapping, &initial_discovery) {
        (Some(mapping), _) => mapping.clone(),
        (None, Some(discovered)) => {
            ensure_mapping(state, connector_id, &settings.mapping_version, discovered).await?
        }
        _ => unreachable!("one mapping source is always available"),
    };
    sqlx::query("UPDATE sync_runs SET status='validating' WHERE id=$1")
        .bind(run_id)
        .execute(&state.pool)
        .await?;
    sqlx::query("UPDATE product_import_runs SET status='validating' WHERE id=$1")
        .bind(run_id)
        .execute(&state.pool)
        .await?;

    let mut cursor = run
        .resume_cursor
        .as_deref()
        .map(|value| open_cursor(key, run_id, value))
        .transpose()?
        .unwrap_or_default();
    let mut assets_copied = run.assets_copied;
    let mut assets_reused = run.assets_reused;
    let mut assets_failed = run.assets_failed;
    let mut processed_tables = Vec::new();

    for (table_index, source) in settings.sources.iter().enumerate() {
        if table_index < cursor.table_index {
            processed_tables.push(source.table_id.clone());
            continue;
        }
        let discovered = if let Some(discovered) = &initial_discovery {
            discovered
                .iter()
                .find(|value| value.source.table_id == source.table_id)
                .cloned()
                .ok_or_else(|| {
                    ApiError::service_unavailable("Feishu table discovery is incomplete.")
                })?
        } else {
            match discover_sources(&state.feishu_client, std::slice::from_ref(source)).await {
                Ok(mut values) => values.remove(0),
                Err(error) => {
                    record_sync_error(
                        state,
                        run_id,
                        None,
                        "tableUnavailable",
                        &format!("{}: {error}", source.table_id),
                    )
                    .await?;
                    increment_table_failure(state, run_id).await?;
                    cursor = FeishuResumeCursor {
                        table_index: table_index + 1,
                        page_token: None,
                        record_index: 0,
                    };
                    persist_cursor(
                        state,
                        run_id,
                        key,
                        &cursor,
                        assets_copied,
                        assets_reused,
                        assets_failed,
                    )
                    .await?;
                    continue;
                }
            }
        };
        let table_mapping = match runtime_table_mapping(&mapping, &discovered) {
            Ok(value) => value,
            Err(error) => {
                record_sync_error(
                    state,
                    run_id,
                    None,
                    "schemaDrift",
                    &format!("{}: {error}", source.table_id),
                )
                .await?;
                increment_table_failure(state, run_id).await?;
                cursor = FeishuResumeCursor {
                    table_index: table_index + 1,
                    page_token: None,
                    record_index: 0,
                };
                persist_cursor(
                    state,
                    run_id,
                    key,
                    &cursor,
                    assets_copied,
                    assets_reused,
                    assets_failed,
                )
                .await?;
                continue;
            }
        };
        processed_tables.push(source.table_id.clone());
        let mut page_token = (table_index == cursor.table_index)
            .then(|| cursor.page_token.clone())
            .flatten();
        let mut record_index = if table_index == cursor.table_index {
            cursor.record_index
        } else {
            0
        };
        loop {
            let page = state
                .feishu_client
                .list_records_page(
                    &discovered.app_token,
                    &source.table_id,
                    page_token.as_deref(),
                )
                .await?;
            for (index, source_record) in page.items.iter().enumerate().skip(record_index) {
                let normalized = normalize_record(source, &table_mapping, source_record);
                cursor = FeishuResumeCursor {
                    table_index,
                    page_token: page_token.clone(),
                    record_index: index + 1,
                };
                if staging_exists(state, run_id, &normalized.source_record_id).await? {
                    persist_cursor(
                        state,
                        run_id,
                        key,
                        &cursor,
                        assets_copied,
                        assets_reused,
                        assets_failed,
                    )
                    .await?;
                    continue;
                }
                let row_number = next_source_row_number(state, run_id).await?;
                if !normalized.issues.is_empty() {
                    stage_invalid_record(
                        state,
                        connector_id,
                        run_id,
                        run_id,
                        &settings.mapping_version,
                        &import_checksum,
                        row_number,
                        &normalized,
                    )
                    .await?;
                    persist_cursor(
                        state,
                        run_id,
                        key,
                        &cursor,
                        assets_copied,
                        assets_reused,
                        assets_failed,
                    )
                    .await?;
                    continue;
                }
                let assets =
                    match store_source_assets(state, connector_id, &normalized.attachments).await {
                        Ok(assets) => assets,
                        Err(error) if error.status().is_server_error() => return Err(error),
                        Err(error) => {
                            assets_failed =
                                assets_failed.saturating_add(normalized.attachments.len() as u64);
                            let mut invalid = normalized.clone();
                            invalid.issues.push(ValidationIssue {
                                field_path: "attachments".into(),
                                code: "attachmentSyncFailed".into(),
                                detail: error.to_string(),
                            });
                            stage_invalid_record(
                                state,
                                connector_id,
                                run_id,
                                run_id,
                                &settings.mapping_version,
                                &import_checksum,
                                row_number,
                                &invalid,
                            )
                            .await?;
                            persist_cursor(
                                state,
                                run_id,
                                key,
                                &cursor,
                                assets_copied,
                                assets_reused,
                                assets_failed,
                            )
                            .await?;
                            continue;
                        }
                    };
                assets_copied = assets_copied.saturating_add(
                    assets.iter().filter(|asset| asset.newly_created).count() as u64,
                );
                assets_reused = assets_reused.saturating_add(
                    assets.iter().filter(|asset| !asset.newly_created).count() as u64,
                );
                match promote_record(
                    state,
                    connector_id,
                    run_id,
                    run_id,
                    &settings.mapping_version,
                    &import_checksum,
                    row_number,
                    &normalized,
                    &assets,
                )
                .await
                {
                    Ok(
                        PromotionOutcome::Created
                        | PromotionOutcome::Updated
                        | PromotionOutcome::Archived
                        | PromotionOutcome::Unchanged,
                    ) => {}
                    Err(error) if error.status().is_server_error() => return Err(error),
                    Err(error) => {
                        compensate_source_assets(state, &assets).await;
                        let mut invalid = normalized.clone();
                        invalid.issues.push(ValidationIssue {
                            field_path: "$".into(),
                            code: "publicationFailed".into(),
                            detail: error.to_string(),
                        });
                        stage_invalid_record(
                            state,
                            connector_id,
                            run_id,
                            run_id,
                            &settings.mapping_version,
                            &import_checksum,
                            row_number,
                            &invalid,
                        )
                        .await?;
                    }
                }
                persist_cursor(
                    state,
                    run_id,
                    key,
                    &cursor,
                    assets_copied,
                    assets_reused,
                    assets_failed,
                )
                .await?;
            }
            if !page.has_more {
                break;
            }
            page_token = page.next_page_token;
            record_index = 0;
            cursor = FeishuResumeCursor {
                table_index,
                page_token: page_token.clone(),
                record_index: 0,
            };
            persist_cursor(
                state,
                run_id,
                key,
                &cursor,
                assets_copied,
                assets_reused,
                assets_failed,
            )
            .await?;
        }
        cursor = FeishuResumeCursor {
            table_index: table_index + 1,
            page_token: None,
            record_index: 0,
        };
        persist_cursor(
            state,
            run_id,
            key,
            &cursor,
            assets_copied,
            assets_reused,
            assets_failed,
        )
        .await?;
    }

    if run.run_kind == SyncRunKind::Full {
        report_suspected_missing(state, run_id, &processed_tables).await?;
    }
    finalize_run(
        state,
        run_id,
        run.run_kind,
        assets_copied,
        assets_reused,
        assets_failed,
    )
    .await?;
    let completed = load_sync_run(state, run_id).await?;
    serde_json::to_value(json!({"syncRun": completed}))
        .map_err(|_| ApiError::internal("Feishu run result serialization failed."))
}
