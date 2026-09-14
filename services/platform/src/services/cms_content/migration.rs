use std::collections::BTreeMap;

use super::*;
use crate::{
    models::CmsContentKind,
    services::cms_preflight::{
        load_legacy_snapshot_from_connection, plan_legacy_snapshot, CmsPreflightRecord,
        CmsPreflightRecordRole, LegacyGeneralInformation, LegacySnapshot,
    },
};

const MIGRATION_KEY: &str = "legacy-to-unified-content-v2";
const MIGRATION_ACTOR: &str = "system:cms-v2-migration";
const MIGRATION_REASON: &str = "Migrated from legacy CMS revision";

pub async fn migrate_legacy_content(state: &AppState) -> Result<(), ApiError> {
    let pool = require_postgres(state)?;
    let mut transaction = pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(&mut *transaction)
        .await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext('airtek.cms.v2.migration'))")
        .execute(&mut *transaction)
        .await?;
    if migration_complete(&mut transaction).await? {
        transaction.commit().await?;
        return Ok(());
    }
    let snapshot = load_legacy_snapshot_from_connection(&mut transaction).await?;
    let plan = plan_legacy_snapshot(snapshot.clone(), Utc::now());
    if !plan.report.can_migrate {
        transaction.rollback().await?;
        let report = serde_json::to_string(&plan.report)
            .map_err(|_| ApiError::internal("CMS migration report serialization failed."))?;
        return Err(ApiError::service_unavailable(format!(
            "CMS V2 migration preflight blocked startup: {report}"
        )));
    }
    migrate_records(&mut transaction, &snapshot, &plan.records).await?;
    let report = serde_json::to_value(&plan.report)
        .map_err(|_| ApiError::internal("CMS migration report serialization failed."))?;
    sqlx::query(
        r#"INSERT INTO cms_data_migrations(key,report,completed_at)
           VALUES ($1,$2,now())
           ON CONFLICT (key) DO UPDATE SET
             report=EXCLUDED.report,completed_at=EXCLUDED.completed_at"#,
    )
    .bind(MIGRATION_KEY)
    .bind(report)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(())
}

async fn migration_complete(transaction: &mut Transaction<'_, Postgres>) -> Result<bool, ApiError> {
    let marker = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM cms_data_migrations WHERE key=$1)",
    )
    .bind(MIGRATION_KEY)
    .fetch_one(&mut **transaction)
    .await?;
    if !marker {
        return Ok(false);
    }
    let missing = sqlx::query_scalar::<_, i64>(
        r#"SELECT count(*) FROM content_entries entry
           LEFT JOIN content_drafts draft ON draft.content_id=entry.id
           WHERE draft.content_id IS NULL"#,
    )
    .fetch_one(&mut **transaction)
    .await?;
    let missing_general_information = sqlx::query_scalar::<_, i64>(
        r#"SELECT count(*) FROM general_information information
           LEFT JOIN content_entries entry ON entry.id=information.id
           WHERE entry.id IS NULL"#,
    )
    .fetch_one(&mut **transaction)
    .await?;
    Ok(missing == 0 && missing_general_information == 0)
}

async fn migrate_records(
    transaction: &mut Transaction<'_, Postgres>,
    snapshot: &LegacySnapshot,
    records: &[CmsPreflightRecord],
) -> Result<(), ApiError> {
    for record in records
        .iter()
        .filter(|record| record.role == CmsPreflightRecordRole::Working)
    {
        ensure_parent(transaction, snapshot, record).await?;
    }
    for record in records
        .iter()
        .filter(|record| record.role == CmsPreflightRecordRole::Revision)
    {
        migrate_revision(transaction, snapshot, record).await?;
    }
    let latest = latest_revisions(records);
    for record in records
        .iter()
        .filter(|record| record.role == CmsPreflightRecordRole::Working)
    {
        migrate_working(transaction, snapshot, record, &latest).await?;
    }
    Ok(())
}

async fn ensure_parent(
    transaction: &mut Transaction<'_, Postgres>,
    snapshot: &LegacySnapshot,
    record: &CmsPreflightRecord,
) -> Result<(), ApiError> {
    if record.candidate.kind != CmsContentKind::GeneralInformation {
        return Ok(());
    }
    let source = general_information(snapshot, record.entity_id)?;
    sqlx::query(
        r#"INSERT INTO content_entries
           (id,kind,slug,locale,title,status,is_placeholder,current_revision,
            published_revision,scheduled_for,payload,updated_at,data_origin,
            template_key,latest_revision,cms_published_revision,cms_created_at,
            cms_updated_by)
           VALUES ($1,'generalInformation','general-information',$2,
                   'General Information',$3,$4,$5,NULL,NULL,$6,$7,$8,
                   'generalInformation',0,NULL,$7,$9)
           ON CONFLICT (id) DO NOTHING"#,
    )
    .bind(record.entity_id)
    .bind(&record.candidate.locale)
    .bind(status_label(&source.status))
    .bind(source.is_placeholder)
    .bind(source.current_revision)
    .bind(&source.payload)
    .bind(source.updated_at)
    .bind(&source.data_origin)
    .bind(MIGRATION_ACTOR)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

async fn migrate_revision(
    transaction: &mut Transaction<'_, Postgres>,
    snapshot: &LegacySnapshot,
    record: &CmsPreflightRecord,
) -> Result<(), ApiError> {
    let document = serde_json::to_value(&record.candidate)
        .map_err(|_| ApiError::internal("CMS revision migration serialization failed."))?;
    let kind = if published_revision(snapshot, record.entity_id) == Some(record.source_revision) {
        "publish"
    } else {
        "manual"
    };
    sqlx::query(
        r#"INSERT INTO content_revisions
           (content_id,revision,payload,document,source_draft_version,
            revision_kind,reason,created_by,created_at)
           VALUES ($1,$2,NULL,$3,$2,$4,$5,$6,now())
           ON CONFLICT (content_id,revision) DO UPDATE SET
             document=EXCLUDED.document,
             source_draft_version=EXCLUDED.source_draft_version,
             revision_kind=EXCLUDED.revision_kind,
             reason=EXCLUDED.reason
           WHERE content_revisions.document IS NULL"#,
    )
    .bind(record.entity_id)
    .bind(record.source_revision)
    .bind(document)
    .bind(kind)
    .bind(MIGRATION_REASON)
    .bind(MIGRATION_ACTOR)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

async fn migrate_working(
    transaction: &mut Transaction<'_, Postgres>,
    snapshot: &LegacySnapshot,
    record: &CmsPreflightRecord,
    latest: &BTreeMap<Uuid, i64>,
) -> Result<(), ApiError> {
    let document = serde_json::to_value(&record.candidate)
        .map_err(|_| ApiError::internal("CMS draft migration serialization failed."))?;
    sqlx::query(
        r#"INSERT INTO content_drafts
           (content_id,draft_version,document,updated_by,updated_at)
           VALUES ($1,$2,$3,$4,now())
           ON CONFLICT (content_id) DO NOTHING"#,
    )
    .bind(record.entity_id)
    .bind(record.candidate.draft_version)
    .bind(document)
    .bind(MIGRATION_ACTOR)
    .execute(&mut **transaction)
    .await?;
    sqlx::query(
        r#"UPDATE content_entries
           SET template_key=$2,latest_revision=$3,cms_published_revision=$4,
               cms_updated_by=$5
           WHERE id=$1"#,
    )
    .bind(record.entity_id)
    .bind(enum_label(record.candidate.template_key))
    .bind(latest.get(&record.entity_id).copied().unwrap_or(0))
    .bind(published_revision(snapshot, record.entity_id))
    .bind(MIGRATION_ACTOR)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

fn latest_revisions(records: &[CmsPreflightRecord]) -> BTreeMap<Uuid, i64> {
    let mut latest = BTreeMap::<Uuid, i64>::new();
    for record in records
        .iter()
        .filter(|record| record.role == CmsPreflightRecordRole::Revision)
    {
        latest
            .entry(record.entity_id)
            .and_modify(|value| *value = (*value).max(record.source_revision))
            .or_insert(record.source_revision);
    }
    latest
}

fn published_revision(snapshot: &LegacySnapshot, id: Uuid) -> Option<i64> {
    snapshot
        .content_entries
        .iter()
        .find(|entry| entry.id == id)
        .and_then(|entry| entry.published_revision)
        .or_else(|| {
            snapshot
                .general_information
                .iter()
                .find(|entry| entry.id == id)
                .and_then(|entry| entry.published_revision)
        })
}

fn general_information(
    snapshot: &LegacySnapshot,
    id: Uuid,
) -> Result<&LegacyGeneralInformation, ApiError> {
    snapshot
        .general_information
        .iter()
        .find(|entry| entry.id == id)
        .ok_or_else(|| {
            ApiError::service_unavailable("General Information migration source is missing.")
        })
}

fn status_label(status: &str) -> &str {
    match status {
        "published" => "published",
        "archived" => "archived",
        _ => "draft",
    }
}
