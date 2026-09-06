use super::*;

const RECORD_COLUMNS: &str = r#"entry.id,entry.status,entry.latest_revision,
    entry.cms_published_revision,entry.cms_created_at,draft.document,
    draft.draft_version,draft.updated_at,draft.updated_by"#;

pub async fn list_content(state: &AppState) -> Result<Vec<ContentRecordV2>, ApiError> {
    let pool = require_postgres(state)?;
    let query = format!(
        "SELECT {RECORD_COLUMNS} FROM content_entries entry \
         JOIN content_drafts draft ON draft.content_id=entry.id \
         ORDER BY draft.updated_at DESC,entry.id"
    );
    sqlx::query(&query)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| decode_record(&row))
        .collect()
}

pub async fn get_content(state: &AppState, id: Uuid) -> Result<ContentRecordV2, ApiError> {
    let pool = require_postgres(state)?;
    let query = format!(
        "SELECT {RECORD_COLUMNS} FROM content_entries entry \
         JOIN content_drafts draft ON draft.content_id=entry.id WHERE entry.id=$1"
    );
    sqlx::query(&query)
        .bind(id)
        .fetch_optional(pool)
        .await?
        .map(|row| decode_record(&row))
        .transpose()?
        .ok_or_else(|| ApiError::not_found("Content entry was not found."))
}

pub async fn list_revisions(
    state: &AppState,
    id: Uuid,
) -> Result<Vec<ContentRevisionV2>, ApiError> {
    get_content(state, id).await?;
    let pool = require_postgres(state)?;
    sqlx::query(
        r#"SELECT content_id,revision,source_draft_version,revision_kind,
                  document,reason,created_by,created_at
           FROM content_revisions
           WHERE content_id=$1 AND document IS NOT NULL
           ORDER BY revision DESC"#,
    )
    .bind(id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| decode_revision(&row))
    .collect()
}

pub(super) async fn lock_record(
    transaction: &mut Transaction<'_, Postgres>,
    id: Uuid,
) -> Result<ContentRecordV2, ApiError> {
    let query = format!(
        "SELECT {RECORD_COLUMNS} FROM content_entries entry \
         JOIN content_drafts draft ON draft.content_id=entry.id \
         WHERE entry.id=$1 FOR UPDATE OF entry,draft"
    );
    sqlx::query(&query)
        .bind(id)
        .fetch_optional(&mut **transaction)
        .await?
        .map(|row| decode_record(&row))
        .transpose()?
        .ok_or_else(|| ApiError::not_found("Content entry was not found."))
}

pub(super) async fn load_revision(
    transaction: &mut Transaction<'_, Postgres>,
    id: Uuid,
    revision: i64,
) -> Result<ContentRevisionV2, ApiError> {
    sqlx::query(
        r#"SELECT content_id,revision,source_draft_version,revision_kind,
                  document,reason,created_by,created_at
           FROM content_revisions
           WHERE content_id=$1 AND revision=$2 AND document IS NOT NULL"#,
    )
    .bind(id)
    .bind(revision)
    .fetch_optional(&mut **transaction)
    .await?
    .map(|row| decode_revision(&row))
    .transpose()?
    .ok_or_else(|| ApiError::not_found("Content revision was not found."))
}

fn decode_record(row: &PgRow) -> Result<ContentRecordV2, ApiError> {
    let document: ContentDraftV2 = decode_json(row.try_get("document")?, "content draft")?;
    let draft_version: i64 = row.try_get("draft_version")?;
    if document.draft_version != draft_version {
        return Err(ApiError::service_unavailable(
            "Stored content draft version does not match its row version.",
        ));
    }
    let latest_revision: i64 = row.try_get("latest_revision")?;
    Ok(ContentRecordV2 {
        id: row.try_get("id")?,
        status: decode_json(Value::String(row.try_get("status")?), "content status")?,
        draft: document,
        latest_revision: (latest_revision > 0).then_some(latest_revision),
        published_revision: row.try_get("cms_published_revision")?,
        created_at: row.try_get("cms_created_at")?,
        updated_at: row.try_get("updated_at")?,
        updated_by: row.try_get("updated_by")?,
    })
}

fn decode_revision(row: &PgRow) -> Result<ContentRevisionV2, ApiError> {
    Ok(ContentRevisionV2 {
        content_id: row.try_get("content_id")?,
        revision: row.try_get("revision")?,
        source_draft_version: row.try_get("source_draft_version")?,
        kind: decode_json(
            Value::String(row.try_get("revision_kind")?),
            "content revision kind",
        )?,
        document: decode_json(row.try_get("document")?, "content revision")?,
        reason: row.try_get("reason")?,
        created_by: row.try_get("created_by")?,
        created_at: row.try_get("created_at")?,
    })
}
