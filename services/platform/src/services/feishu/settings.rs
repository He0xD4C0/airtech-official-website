use chrono::{NaiveTime, Utc};
use serde_json::Value;
use sqlx::{postgres::PgRow, Row};
use uuid::Uuid;

use crate::{
    error::ApiError,
    models::{
        FeishuConnectionTest, FeishuSettings, FeishuSource, FeishuTableCheck, UpdateFeishuSettings,
    },
    services::{media, object_storage_settings},
    state::AppState,
};

use super::{discover_sources, ensure_mapping};

const CONNECTOR_ID: Uuid = Uuid::from_u128(0x63e3d923632a4e47a31b16f3ec81690e);

pub async fn get_feishu_settings(state: &AppState) -> Result<FeishuSettings, ApiError> {
    let row = sqlx::query(
        r#"SELECT settings.connector_id,connector.enabled,settings.interval_minutes,
                  settings.full_reconcile_enabled,settings.full_reconcile_local_time,
                  settings.timezone,settings.mapping_version,settings.sources,
                  settings.revision,settings.last_incremental_at,settings.last_full_at,
                  settings.updated_at,settings.updated_by
           FROM feishu_connector_settings settings
           JOIN source_connectors connector ON connector.id=settings.connector_id
           WHERE settings.connector_id=$1"#,
    )
    .bind(CONNECTOR_ID)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| ApiError::service_unavailable("Feishu settings are not initialized."))?;
    decode_settings(&row)
}

pub async fn update_feishu_settings(
    state: &AppState,
    expected_revision: i64,
    input: &UpdateFeishuSettings,
    actor: &str,
    request_id: Uuid,
) -> Result<FeishuSettings, ApiError> {
    validate_update(input)?;
    if input.enabled && !state.feishu_client.configured() {
        return Err(ApiError::conflict(
            "Configure FEISHU_APP_ID and FEISHU_APP_SECRET before enabling synchronization.",
        ));
    }
    if input.enabled {
        object_storage_settings::active_storage(state).await?;
    }
    let local_time = NaiveTime::parse_from_str(&input.full_reconcile_local_time, "%H:%M")
        .map_err(|_| ApiError::bad_request("fullReconcileLocalTime must use HH:MM."))?;
    let mut transaction = state.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('airtek.feishu.settings',0))")
        .execute(&mut *transaction)
        .await?;
    let before = sqlx::query(
        r#"SELECT settings.connector_id,connector.enabled,settings.interval_minutes,
                  settings.full_reconcile_enabled,settings.full_reconcile_local_time,
                  settings.timezone,settings.mapping_version,settings.sources,
                  settings.revision,settings.last_incremental_at,settings.last_full_at,
                  settings.updated_at,settings.updated_by
           FROM feishu_connector_settings settings
           JOIN source_connectors connector ON connector.id=settings.connector_id
           WHERE settings.connector_id=$1 FOR UPDATE OF settings,connector"#,
    )
    .bind(CONNECTOR_ID)
    .fetch_one(&mut *transaction)
    .await?;
    let before_settings = decode_settings(&before)?;
    if before_settings.revision != expected_revision {
        return Err(ApiError::conflict(
            "Feishu settings changed; reload before saving.",
        ));
    }
    sqlx::query("UPDATE source_connectors SET enabled=$2,updated_at=now() WHERE id=$1")
        .bind(CONNECTOR_ID)
        .bind(input.enabled)
        .execute(&mut *transaction)
        .await?;
    let row = sqlx::query(
        r#"UPDATE feishu_connector_settings
           SET interval_minutes=$2,full_reconcile_enabled=$3,
               full_reconcile_local_time=$4,revision=revision+1,
               updated_at=now(),updated_by=$5
           WHERE connector_id=$1 AND revision=$6
           RETURNING connector_id,interval_minutes,full_reconcile_enabled,
                     full_reconcile_local_time,timezone,mapping_version,sources,revision,
                     last_incremental_at,last_full_at,updated_at,updated_by"#,
    )
    .bind(CONNECTOR_ID)
    .bind(input.interval_minutes)
    .bind(input.full_reconcile_enabled)
    .bind(local_time)
    .bind(actor)
    .bind(expected_revision)
    .fetch_one(&mut *transaction)
    .await?;
    let after = decode_updated_settings(&row, input.enabled)?;
    sqlx::query(
        r#"INSERT INTO audit_log
           (id,actor,action,entity_type,entity_id,before_value,after_value,reason,
            current_version,request_id,occurred_at)
           VALUES ($1,$2,'settings.feishu.update','feishuSettings',$3,$4,$5,
                   'Update automatic Feishu synchronization policy',$6,$7,now())"#,
    )
    .bind(Uuid::new_v4())
    .bind(actor)
    .bind(CONNECTOR_ID)
    .bind(serde_json::to_value(&before_settings).unwrap_or(Value::Null))
    .bind(serde_json::to_value(&after).unwrap_or(Value::Null))
    .bind(after.revision)
    .bind(request_id)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    if input.enabled && !before_settings.enabled {
        super::queue_sync_run(state, crate::models::SyncRunKind::Full, actor).await?;
    }
    Ok(after)
}

pub async fn test_feishu_connection(state: &AppState) -> Result<FeishuConnectionTest, ApiError> {
    let settings = get_feishu_settings(state).await?;
    let credentials_configured = state.feishu_client.configured();
    let token_issued = credentials_configured && state.feishu_client.test_token().await.is_ok();
    let object_storage_ready = test_storage(state).await.is_ok();
    let mut tables = Vec::with_capacity(settings.sources.len());
    let mut discovered = Vec::new();
    if token_issued {
        for source in &settings.sources {
            match discover_sources(&state.feishu_client, std::slice::from_ref(source)).await {
                Ok(mut values) => {
                    let source = values.remove(0);
                    tables.push(FeishuTableCheck {
                        table_id: source.source.table_id.clone(),
                        name: source.source.name.clone(),
                        accessible: true,
                        field_count: source.fields.len(),
                        mapping_valid: false,
                        errors: Vec::new(),
                    });
                    discovered.push(source);
                }
                Err(error) => tables.push(FeishuTableCheck {
                    table_id: source.table_id.clone(),
                    name: source.name.clone(),
                    accessible: false,
                    field_count: 0,
                    mapping_valid: false,
                    errors: vec![error.to_string()],
                }),
            }
        }
    } else {
        tables.extend(settings.sources.iter().map(|source| FeishuTableCheck {
            table_id: source.table_id.clone(),
            name: source.name.clone(),
            accessible: false,
            field_count: 0,
            mapping_valid: false,
            errors: vec!["Feishu credentials are unavailable or were rejected.".into()],
        }));
    }
    if discovered.len() == settings.sources.len() {
        match ensure_mapping(
            state,
            settings.connector_id,
            &settings.mapping_version,
            &discovered,
        )
        .await
        {
            Ok(_) => tables
                .iter_mut()
                .for_each(|table| table.mapping_valid = true),
            Err(error) => tables
                .iter_mut()
                .filter(|table| table.accessible)
                .for_each(|table| table.errors.push(error.to_string())),
        }
    }
    let runnable = token_issued
        && object_storage_ready
        && tables.len() == settings.sources.len()
        && tables.iter().all(|table| table.mapping_valid);
    Ok(FeishuConnectionTest {
        credentials_configured,
        token_issued,
        object_storage_ready,
        runnable,
        tables,
        checked_at: Utc::now(),
    })
}

async fn test_storage(state: &AppState) -> Result<(), ApiError> {
    let settings = object_storage_settings::active_storage(state).await?;
    let key = format!(
        "{}/.airtek-probe/feishu-{}.txt",
        settings.key_prefix,
        Uuid::new_v4().simple()
    );
    let url = object_storage_settings::public_url(&settings, &key);
    media::probe_storage(&settings, &key, &url).await
}

fn validate_update(input: &UpdateFeishuSettings) -> Result<(), ApiError> {
    if !(5..=1440).contains(&input.interval_minutes) {
        return Err(ApiError::bad_request(
            "intervalMinutes must be between 5 and 1440.",
        ));
    }
    NaiveTime::parse_from_str(&input.full_reconcile_local_time, "%H:%M")
        .map(|_| ())
        .map_err(|_| ApiError::bad_request("fullReconcileLocalTime must use HH:MM."))
}

fn decode_settings(row: &PgRow) -> Result<FeishuSettings, ApiError> {
    let sources: Value = row.try_get("sources")?;
    Ok(FeishuSettings {
        connector_id: row.try_get("connector_id")?,
        enabled: row.try_get("enabled")?,
        interval_minutes: row.try_get("interval_minutes")?,
        full_reconcile_enabled: row.try_get("full_reconcile_enabled")?,
        full_reconcile_local_time: time_text(row.try_get("full_reconcile_local_time")?),
        timezone: row.try_get("timezone")?,
        mapping_version: row.try_get("mapping_version")?,
        sources: decode_sources(sources)?,
        revision: row.try_get("revision")?,
        last_incremental_at: row.try_get("last_incremental_at")?,
        last_full_at: row.try_get("last_full_at")?,
        updated_at: row.try_get("updated_at")?,
        updated_by: row.try_get("updated_by")?,
    })
}

fn decode_updated_settings(row: &PgRow, enabled: bool) -> Result<FeishuSettings, ApiError> {
    let sources: Value = row.try_get("sources")?;
    Ok(FeishuSettings {
        connector_id: row.try_get("connector_id")?,
        enabled,
        interval_minutes: row.try_get("interval_minutes")?,
        full_reconcile_enabled: row.try_get("full_reconcile_enabled")?,
        full_reconcile_local_time: time_text(row.try_get("full_reconcile_local_time")?),
        timezone: row.try_get("timezone")?,
        mapping_version: row.try_get("mapping_version")?,
        sources: decode_sources(sources)?,
        revision: row.try_get("revision")?,
        last_incremental_at: row.try_get("last_incremental_at")?,
        last_full_at: row.try_get("last_full_at")?,
        updated_at: row.try_get("updated_at")?,
        updated_by: row.try_get("updated_by")?,
    })
}

fn decode_sources(value: Value) -> Result<Vec<FeishuSource>, ApiError> {
    serde_json::from_value(value).map_err(|error| {
        tracing::error!(%error, "stored Feishu sources are invalid");
        ApiError::service_unavailable("Stored Feishu source settings are invalid.")
    })
}

fn time_text(value: NaiveTime) -> String {
    value.format("%H:%M").to_string()
}
