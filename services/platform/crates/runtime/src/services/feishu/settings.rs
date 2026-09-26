use chrono::{NaiveTime, Utc};
use serde_json::Value;
use sqlx::{postgres::PgRow, Row};
use uuid::Uuid;

use crate::error::ApiError;
use crate::services::{media, object_storage_settings};
use crate::state::AppState;
use airtek_domain::models::{
    FeishuConnectionTest, FeishuSettings, FeishuSource, FeishuTableCheck, UpdateFeishuSettings,
};

use super::{discover_sources, ensure_mapping, load_client, CONNECTOR_ID};

const SETTINGS_COLUMNS: &str = r#"settings.connector_id,settings.app_id,
    settings.app_secret IS NOT NULL AS secret_configured,connector.enabled,
    settings.interval_enabled,settings.interval_minutes,settings.daily_enabled,
    settings.daily_local_time,settings.timezone,settings.mapping_version,settings.sources,
    settings.revision,settings.connection_revision,settings.tested_connection_revision,
    settings.last_connection_test_at,settings.last_interval_at,settings.last_daily_at,
    settings.updated_at,settings.updated_by"#;

pub async fn get_feishu_settings(state: &AppState) -> Result<FeishuSettings, ApiError> {
    let query = format!(
        "SELECT {SETTINGS_COLUMNS} FROM feishu_connector_settings settings \
         JOIN source_connectors connector ON connector.id=settings.connector_id \
         WHERE settings.connector_id=$1"
    );
    let row = sqlx::query(&query)
        .bind(CONNECTOR_ID)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| ApiError::service_unavailable("Feishu settings are not initialized."))?;
    decode_settings(&row)
}

pub(super) async fn get_feishu_settings_in_transaction(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
) -> Result<FeishuSettings, ApiError> {
    let query = format!(
        "SELECT {SETTINGS_COLUMNS} FROM feishu_connector_settings settings \
         JOIN source_connectors connector ON connector.id=settings.connector_id \
         WHERE settings.connector_id=$1"
    );
    let row = sqlx::query(&query)
        .bind(CONNECTOR_ID)
        .fetch_optional(&mut **transaction)
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
    super::settings_validation::validate_update(input)?;
    let normalized_sources = super::settings_validation::normalize_sources(&input.sources);
    let daily_time = NaiveTime::parse_from_str(&input.daily_local_time, "%H:%M")
        .map_err(|_| ApiError::bad_request("dailyLocalTime must use HH:MM."))?;
    let mut transaction = state.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('airtek.feishu.settings',0))")
        .execute(&mut *transaction)
        .await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("feishu:queue:{CONNECTOR_ID}"))
        .execute(&mut *transaction)
        .await?;
    let query = format!(
        "SELECT {SETTINGS_COLUMNS},settings.app_secret \
         FROM feishu_connector_settings settings \
         JOIN source_connectors connector ON connector.id=settings.connector_id \
         WHERE settings.connector_id=$1 FOR UPDATE OF settings,connector"
    );
    let before_row = sqlx::query(&query)
        .bind(CONNECTOR_ID)
        .fetch_one(&mut *transaction)
        .await?;
    let before = decode_settings(&before_row)?;
    if before.revision != expected_revision {
        return Err(ApiError::conflict(
            "Feishu settings changed; reload before saving.",
        ));
    }
    let previous_secret: Option<String> = before_row.try_get("app_secret")?;
    let (app_id, app_secret, credentials_changed) =
        resolve_credentials(input, before.app_id.as_deref(), previous_secret.as_deref())?;
    let sources_changed = before.sources != normalized_sources;
    if active_connector_work(&mut transaction).await? {
        return Err(ApiError::conflict(
            "Feishu settings cannot change while connector work is active.",
        ));
    }
    let connection_changed = credentials_changed || sources_changed;
    let connection_revision = before.connection_revision + i64::from(connection_changed);
    let tested_revision = (!connection_changed)
        .then_some(before.tested_connection_revision)
        .flatten();
    let mapping_version = if sources_changed {
        format!("feishu-product-v1-r{connection_revision}")
    } else {
        before.mapping_version.clone()
    };
    if input.enabled {
        validate_runnable_settings(
            state,
            app_id.as_deref(),
            app_secret.as_deref(),
            &normalized_sources,
            tested_revision,
            connection_revision,
        )
        .await?;
    }
    super::deletion::mark_removed_sources_pending(
        &mut transaction,
        CONNECTOR_ID,
        &before.sources,
        &normalized_sources,
    )
    .await?;
    let sources = serde_json::to_value(&normalized_sources)
        .map_err(|_| ApiError::bad_request("Feishu sources are invalid."))?;
    sqlx::query("UPDATE source_connectors SET enabled=$2,updated_at=now() WHERE id=$1")
        .bind(CONNECTOR_ID)
        .bind(input.enabled)
        .execute(&mut *transaction)
        .await?;
    let row = sqlx::query(
        r#"UPDATE feishu_connector_settings SET
             app_id=$2,app_secret=$3,sources=$4,mapping_version=$5,
             interval_enabled=$6,interval_minutes=$7,daily_enabled=$8,
             daily_local_time=$9,connection_revision=$10,
             tested_connection_revision=$11,revision=revision+1,
             updated_at=now(),updated_by=$12
           WHERE connector_id=$1 AND revision=$13
           RETURNING connector_id,app_id,app_secret IS NOT NULL AS secret_configured,
             interval_enabled,interval_minutes,daily_enabled,daily_local_time,timezone,
             mapping_version,sources,revision,connection_revision,tested_connection_revision,
             last_connection_test_at,last_interval_at,last_daily_at,updated_at,updated_by"#,
    )
    .bind(CONNECTOR_ID)
    .bind(app_id)
    .bind(app_secret)
    .bind(sources)
    .bind(mapping_version)
    .bind(input.interval_enabled)
    .bind(input.interval_minutes)
    .bind(input.daily_enabled)
    .bind(daily_time)
    .bind(connection_revision)
    .bind(tested_revision)
    .bind(actor)
    .bind(expected_revision)
    .fetch_one(&mut *transaction)
    .await?;
    let after = decode_updated_settings(&row, input.enabled)?;
    write_settings_audit(&mut transaction, actor, request_id, &before, &after).await?;
    if input.enabled && !before.enabled {
        let queued = super::queue_full_sync_in_transaction(
            &mut transaction,
            state,
            &after,
            airtek_domain::models::FeishuSyncTrigger::Initial,
            actor,
        )
        .await?;
        if queued.is_none() {
            return Err(ApiError::conflict(
                "The Feishu connector became active while settings were being saved.",
            ));
        }
    }
    transaction.commit().await?;
    Ok(after)
}

pub async fn test_feishu_connection(state: &AppState) -> Result<FeishuConnectionTest, ApiError> {
    let settings = get_feishu_settings(state).await?;
    let enabled_sources = settings
        .sources
        .iter()
        .filter(|source| source.enabled)
        .cloned()
        .collect::<Vec<_>>();
    let credentials_configured = settings.secret_configured;
    let client = if credentials_configured {
        load_client(state).await.ok()
    } else {
        None
    };
    let token_issued = match client.as_ref() {
        Some(client) => client.test_token().await.is_ok(),
        None => false,
    };
    let object_storage_ready = test_storage(state).await.is_ok();
    let private_staging_ready = state.config.product_staging_encryption_key.is_some();
    let (mut tables, discovered) =
        inspect_sources(client.as_ref().filter(|_| token_issued), &enabled_sources).await;
    if discovered.len() == enabled_sources.len() && !discovered.is_empty() {
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
    let mut runnable = credentials_configured
        && token_issued
        && object_storage_ready
        && private_staging_ready
        && !enabled_sources.is_empty()
        && tables.len() == enabled_sources.len()
        && tables.iter().all(|table| table.mapping_valid);
    if runnable {
        let updated = sqlx::query(
            r#"UPDATE feishu_connector_settings
               SET tested_connection_revision=connection_revision,
                   last_connection_test_at=now()
               WHERE connector_id=$1 AND connection_revision=$2"#,
        )
        .bind(settings.connector_id)
        .bind(settings.connection_revision)
        .execute(&state.pool)
        .await?;
        runnable = updated.rows_affected() == 1;
    }
    Ok(FeishuConnectionTest {
        credentials_configured,
        token_issued,
        object_storage_ready,
        private_staging_ready,
        runnable,
        connection_revision: settings.connection_revision,
        tables,
        checked_at: Utc::now(),
    })
}

async fn inspect_sources(
    client: Option<&super::FeishuClient>,
    sources: &[FeishuSource],
) -> (Vec<FeishuTableCheck>, Vec<super::DiscoveredSource>) {
    let mut tables = Vec::with_capacity(sources.len());
    let mut discovered = Vec::new();
    for source in sources {
        let result = match client {
            Some(client) => discover_sources(client, std::slice::from_ref(source)).await,
            None => Err(ApiError::service_unavailable(
                "Feishu credentials are unavailable or were rejected.",
            )),
        };
        match result {
            Ok(mut values) => {
                let value = values.remove(0);
                let probe = client
                    .expect("successful discovery has a client")
                    .probe_records(&value.app_token, &source.table_id)
                    .await;
                tables.push(FeishuTableCheck {
                    wiki_token: source.wiki_token.clone(),
                    table_id: source.table_id.clone(),
                    name: source.name.clone(),
                    accessible: probe.is_ok(),
                    field_count: value.fields.len(),
                    mapping_valid: false,
                    errors: probe
                        .err()
                        .map(|error| vec![error.to_string()])
                        .unwrap_or_default(),
                });
                if tables.last().is_some_and(|table| table.accessible) {
                    discovered.push(value);
                }
            }
            Err(error) => tables.push(FeishuTableCheck {
                wiki_token: source.wiki_token.clone(),
                table_id: source.table_id.clone(),
                name: source.name.clone(),
                accessible: false,
                field_count: 0,
                mapping_valid: false,
                errors: vec![error.to_string()],
            }),
        }
    }
    (tables, discovered)
}

async fn validate_runnable_settings(
    state: &AppState,
    app_id: Option<&str>,
    app_secret: Option<&str>,
    sources: &[FeishuSource],
    tested_revision: Option<i64>,
    connection_revision: i64,
) -> Result<(), ApiError> {
    if app_id.is_none() || app_secret.is_none() {
        return Err(super::settings_validation::validation(
            "appSecret",
            "Save Feishu application credentials before enabling synchronization.",
        ));
    }
    if !sources.iter().any(|source| source.enabled) {
        return Err(super::settings_validation::validation(
            "sources",
            "Enable at least one Feishu product table.",
        ));
    }
    if tested_revision != Some(connection_revision) {
        return Err(super::settings_validation::validation(
            "enabled",
            "Run a successful connection test for the current credentials and sources first.",
        ));
    }
    if state.config.product_staging_encryption_key.is_none() {
        return Err(ApiError::service_unavailable(
            "Product staging encryption is not configured.",
        ));
    }
    object_storage_settings::active_storage(state).await?;
    Ok(())
}

async fn active_connector_work(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
) -> Result<bool, ApiError> {
    Ok(sqlx::query_scalar::<_, bool>(
        r#"SELECT EXISTS(
             SELECT 1 FROM sync_runs WHERE connector_id=$1
             AND status IN ('queued','fetching','validating','readyToPublish')
           ) OR EXISTS(
             SELECT 1 FROM jobs WHERE job_type='feishuSourcePurge'
             AND status IN ('queued','running')
             AND payload->>'connectorId'=$1::text
           )"#,
    )
    .bind(CONNECTOR_ID)
    .fetch_one(&mut **transaction)
    .await?)
}

fn resolve_credentials(
    input: &UpdateFeishuSettings,
    previous_app_id: Option<&str>,
    previous_secret: Option<&str>,
) -> Result<(Option<String>, Option<String>, bool), ApiError> {
    if input.clear_credentials {
        return Ok((None, None, previous_app_id.is_some()));
    }
    let app_id = input.app_id.trim().to_owned();
    if input.app_secret.is_empty() {
        if previous_app_id == Some(app_id.as_str()) {
            if let Some(secret) = previous_secret {
                return Ok((Some(app_id), Some(secret.to_owned()), false));
            }
        }
        return Err(super::settings_validation::validation(
            "appSecret",
            "App Secret is required for a new or changed App ID.",
        ));
    }
    Ok((Some(app_id), Some(input.app_secret.clone()), true))
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

async fn write_settings_audit(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    actor: &str,
    request_id: Uuid,
    before: &FeishuSettings,
    after: &FeishuSettings,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"INSERT INTO audit_log
           (id,actor,action,entity_type,entity_id,before_value,after_value,reason,
            current_version,request_id,occurred_at)
           VALUES ($1,$2,'settings.feishu.update','feishuSettings',$3,$4,$5,
                   'Update Feishu connection and scheduling policy',$6,$7,now())"#,
    )
    .bind(Uuid::new_v4())
    .bind(actor)
    .bind(CONNECTOR_ID)
    .bind(serde_json::to_value(before).unwrap_or(Value::Null))
    .bind(serde_json::to_value(after).unwrap_or(Value::Null))
    .bind(after.revision)
    .bind(request_id)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

fn decode_settings(row: &PgRow) -> Result<FeishuSettings, ApiError> {
    decode_settings_with_enabled(row, row.try_get("enabled")?)
}

fn decode_updated_settings(row: &PgRow, enabled: bool) -> Result<FeishuSettings, ApiError> {
    decode_settings_with_enabled(row, enabled)
}

fn decode_settings_with_enabled(row: &PgRow, enabled: bool) -> Result<FeishuSettings, ApiError> {
    let sources: Value = row.try_get("sources")?;
    Ok(FeishuSettings {
        connector_id: row.try_get("connector_id")?,
        app_id: row.try_get("app_id")?,
        secret_configured: row.try_get("secret_configured")?,
        enabled,
        interval_enabled: row.try_get("interval_enabled")?,
        interval_minutes: row.try_get("interval_minutes")?,
        daily_enabled: row.try_get("daily_enabled")?,
        daily_local_time: time_text(row.try_get("daily_local_time")?),
        timezone: row.try_get("timezone")?,
        mapping_version: row.try_get("mapping_version")?,
        sources: serde_json::from_value(sources).map_err(|error| {
            tracing::error!(%error, "stored Feishu sources are invalid");
            ApiError::service_unavailable("Stored Feishu source settings are invalid.")
        })?,
        revision: row.try_get("revision")?,
        connection_revision: row.try_get("connection_revision")?,
        tested_connection_revision: row.try_get("tested_connection_revision")?,
        last_connection_test_at: row.try_get("last_connection_test_at")?,
        last_interval_at: row.try_get("last_interval_at")?,
        last_daily_at: row.try_get("last_daily_at")?,
        updated_at: row.try_get("updated_at")?,
        updated_by: row.try_get("updated_by")?,
    })
}

fn time_text(value: NaiveTime) -> String {
    value.format("%H:%M").to_string()
}
