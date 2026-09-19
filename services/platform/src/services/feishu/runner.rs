use serde_json::{json, Value};
use uuid::Uuid;

use crate::{error::ApiError, models::ValidationIssue, state::AppState};

use super::runner_support::{
    complete_table, fail_table, finalize_run, load_table_asset_counts, next_source_row_number,
    persist_cursor, persist_table_asset_counts, runtime_table_mapping, staging_exists, start_table,
};
use super::{
    compensate_source_assets, discover_sources, load_client, load_mapping, load_sync_run,
    normalize_record, open_cursor, promote_record, reconcile_missing_records, record_sync_error,
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
    let client = load_client(state).await?;
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

    let mapping = load_mapping(state, connector_id, &run.mapping_version)
        .await?
        .ok_or_else(|| {
            ApiError::conflict(
                "The frozen Feishu mapping is unavailable; run the connection test again.",
            )
        })?;
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

    'tables: for (table_index, source) in run.sources.iter().enumerate() {
        if table_index < cursor.table_index {
            continue;
        }
        start_table(state, run_id, &source.wiki_token, &source.table_id).await?;
        let mut table_assets =
            load_table_asset_counts(state, run_id, &source.wiki_token, &source.table_id).await?;
        let discovered = match discover_sources(&client, std::slice::from_ref(source)).await {
            Ok(mut values) => values.remove(0),
            Err(error) => {
                fail_current_table(
                    state,
                    run_id,
                    source,
                    "tableUnavailable",
                    &error.to_string(),
                )
                .await?;
                advance_table_cursor(
                    state,
                    run_id,
                    key,
                    table_index,
                    &mut cursor,
                    assets_copied,
                    assets_reused,
                    assets_failed,
                )
                .await?;
                continue;
            }
        };
        let table_mapping = match runtime_table_mapping(&mapping, &discovered) {
            Ok(value) => value,
            Err(error) => {
                fail_current_table(state, run_id, source, "schemaDrift", &error.to_string())
                    .await?;
                advance_table_cursor(
                    state,
                    run_id,
                    key,
                    table_index,
                    &mut cursor,
                    assets_copied,
                    assets_reused,
                    assets_failed,
                )
                .await?;
                continue;
            }
        };
        let mut page_token = (table_index == cursor.table_index)
            .then(|| cursor.page_token.clone())
            .flatten();
        let mut record_index = if table_index == cursor.table_index {
            cursor.record_index
        } else {
            0
        };
        loop {
            let page = match client
                .list_records_page(
                    &discovered.app_token,
                    &source.table_id,
                    page_token.as_deref(),
                )
                .await
            {
                Ok(page) => page,
                Err(error) => {
                    fail_current_table(
                        state,
                        run_id,
                        source,
                        "tableUnavailable",
                        &error.to_string(),
                    )
                    .await?;
                    advance_table_cursor(
                        state,
                        run_id,
                        key,
                        table_index,
                        &mut cursor,
                        assets_copied,
                        assets_reused,
                        assets_failed,
                    )
                    .await?;
                    continue 'tables;
                }
            };
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
                table_assets.seen = table_assets
                    .seen
                    .saturating_add(normalized.attachments.len() as u64);
                if !normalized.issues.is_empty() {
                    stage_invalid_record(
                        state,
                        connector_id,
                        run_id,
                        run_id,
                        &run.mapping_version,
                        &import_checksum,
                        row_number,
                        &normalized,
                    )
                    .await?;
                    persist_table_asset_counts(
                        state,
                        run_id,
                        &source.wiki_token,
                        &source.table_id,
                        table_assets,
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
                let assets = match store_source_assets(
                    state,
                    &client,
                    connector_id,
                    run_id,
                    &normalized.attachments,
                )
                .await
                {
                    Ok(assets) => assets,
                    Err(error) => {
                        assets_failed =
                            assets_failed.saturating_add(normalized.attachments.len() as u64);
                        table_assets.failed = table_assets
                            .failed
                            .saturating_add(normalized.attachments.len() as u64);
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
                            &run.mapping_version,
                            &import_checksum,
                            row_number,
                            &invalid,
                        )
                        .await?;
                        persist_table_asset_counts(
                            state,
                            run_id,
                            &source.wiki_token,
                            &source.table_id,
                            table_assets,
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
                let copied = assets.iter().filter(|asset| asset.newly_created).count() as u64;
                let reused = assets.len() as u64 - copied;
                assets_copied = assets_copied.saturating_add(copied);
                assets_reused = assets_reused.saturating_add(reused);
                table_assets.copied = table_assets.copied.saturating_add(copied);
                table_assets.reused = table_assets.reused.saturating_add(reused);
                match promote_record(
                    state,
                    connector_id,
                    run_id,
                    run_id,
                    &run.mapping_version,
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
                            &run.mapping_version,
                            &import_checksum,
                            row_number,
                            &invalid,
                        )
                        .await?;
                    }
                }
                persist_table_asset_counts(
                    state,
                    run_id,
                    &source.wiki_token,
                    &source.table_id,
                    table_assets,
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
        let deleted = reconcile_missing_records(
            state,
            run_id,
            connector_id,
            &source.wiki_token,
            &source.table_id,
        )
        .await?;
        complete_table(state, run_id, &source.wiki_token, &source.table_id, deleted).await?;
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

    finalize_run(state, run_id, assets_copied, assets_reused, assets_failed).await?;
    let completed = load_sync_run(state, run_id).await?;
    serde_json::to_value(json!({"syncRun": completed}))
        .map_err(|_| ApiError::internal("Feishu run result serialization failed."))
}

async fn fail_current_table(
    state: &AppState,
    run_id: Uuid,
    source: &crate::models::FeishuSource,
    code: &str,
    detail: &str,
) -> Result<(), ApiError> {
    let message = format!("{}: {detail}", source.table_id);
    record_sync_error(state, run_id, None, code, &message).await?;
    fail_table(
        state,
        run_id,
        &source.wiki_token,
        &source.table_id,
        &message,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn advance_table_cursor(
    state: &AppState,
    run_id: Uuid,
    key: &crate::config::ProductStagingEncryptionKey,
    table_index: usize,
    cursor: &mut FeishuResumeCursor,
    assets_copied: u64,
    assets_reused: u64,
    assets_failed: u64,
) -> Result<(), ApiError> {
    *cursor = FeishuResumeCursor {
        table_index: table_index + 1,
        page_token: None,
        record_index: 0,
    };
    persist_cursor(
        state,
        run_id,
        key,
        cursor,
        assets_copied,
        assets_reused,
        assets_failed,
    )
    .await
}
