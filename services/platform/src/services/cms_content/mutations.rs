use super::{audit::insert_audit, storage::*, validation::*, *};

pub async fn create_content(
    state: &AppState,
    draft: ContentDraftV2,
    metadata: MutationMetadata,
    idempotency: IdempotencyContext,
) -> Result<ContentRecordV2, ApiError> {
    validate_create(&draft)?;
    let pool = require_postgres(state)?;
    let now = Utc::now();
    let id = Uuid::new_v4();
    let document = serde_json::to_value(&draft)
        .map_err(|_| ApiError::internal("Content draft serialization failed."))?;
    let mut transaction = pool.begin().await?;
    let inserted = sqlx::query(
        r#"INSERT INTO content_entries
           (id,kind,slug,locale,title,status,is_placeholder,current_revision,
            published_revision,scheduled_for,payload,updated_at,data_origin,
            template_key,latest_revision,cms_published_revision,cms_created_at,
            cms_updated_by)
           VALUES ($1,$2,$3,$4,$5,'draft',$6,1,NULL,NULL,$7,$8,'editorial',
                   $9,0,NULL,$8,$10)
           ON CONFLICT (kind,slug,locale) DO NOTHING"#,
    )
    .bind(id)
    .bind(enum_label(draft.kind))
    .bind(storage_slug(&draft))
    .bind(&draft.locale)
    .bind(&draft.title)
    .bind(draft.is_placeholder)
    .bind(&document)
    .bind(now)
    .bind(enum_label(draft.template_key))
    .bind(&metadata.actor)
    .execute(&mut *transaction)
    .await?;
    if inserted.rows_affected() != 1 {
        transaction.rollback().await?;
        return Err(ApiError::conflict(
            "A content entry already uses this kind, slug and locale.",
        ));
    }
    sqlx::query(
        r#"INSERT INTO content_drafts
           (content_id,draft_version,document,updated_by,updated_at)
           VALUES ($1,1,$2,$3,$4)"#,
    )
    .bind(id)
    .bind(&document)
    .bind(&metadata.actor)
    .bind(now)
    .execute(&mut *transaction)
    .await?;
    let record = ContentRecordV2 {
        id,
        status: CmsPublicationStatusV2::Draft,
        draft,
        latest_revision: None,
        published_revision: None,
        created_at: now,
        updated_at: now,
        updated_by: metadata.actor.clone(),
    };
    insert_audit(
        &mut transaction,
        &metadata,
        "content.create",
        id,
        None,
        Some(json_value(&record)?),
        "Create unified CMS content draft",
    )
    .await?;
    finish(transaction, idempotency, &record, StatusCode::CREATED).await?;
    Ok(record)
}

pub async fn save_draft(
    state: &AppState,
    id: Uuid,
    expected: i64,
    mut draft: ContentDraftV2,
    metadata: MutationMetadata,
    idempotency: IdempotencyContext,
) -> Result<ContentRecordV2, ApiError> {
    let pool = require_postgres(state)?;
    let mut transaction = pool.begin().await?;
    let before = lock_record(&mut transaction, id).await?;
    ensure_version(&before, expected, "saving")?;
    validate_save(&before.draft, &draft, expected)?;
    draft.draft_version = expected + 1;
    let now = Utc::now();
    let document = json_value(&draft)?;
    let result = sqlx::query(
        r#"UPDATE content_drafts
           SET draft_version=$2,document=$3,updated_by=$4,updated_at=$5
           WHERE content_id=$1 AND draft_version=$6"#,
    )
    .bind(id)
    .bind(draft.draft_version)
    .bind(&document)
    .bind(&metadata.actor)
    .bind(now)
    .bind(expected)
    .execute(&mut *transaction)
    .await?;
    if result.rows_affected() != 1 {
        transaction.rollback().await?;
        return Err(version_conflict("saving"));
    }
    let record = ContentRecordV2 {
        draft,
        status: if before.published_revision.is_some() {
            CmsPublicationStatusV2::Published
        } else {
            before.status
        },
        updated_at: now,
        updated_by: metadata.actor.clone(),
        ..before.clone()
    };
    insert_audit(
        &mut transaction,
        &metadata,
        "content.draft.save",
        id,
        Some(json_value(&before)?),
        Some(json_value(&record)?),
        "Autosave unified CMS draft",
    )
    .await?;
    finish(transaction, idempotency, &record, StatusCode::OK).await?;
    Ok(record)
}

#[allow(clippy::too_many_arguments)]
pub async fn snapshot_content(
    state: &AppState,
    id: Uuid,
    expected: i64,
    intent: ContentSnapshotIntent,
    reason: String,
    metadata: MutationMetadata,
    idempotency: IdempotencyContext,
) -> Result<ContentRecordV2, ApiError> {
    let pool = require_postgres(state)?;
    let mut transaction = pool.begin().await?;
    let before = lock_record(&mut transaction, id).await?;
    ensure_version(&before, expected, "creating a snapshot")?;
    validate_snapshot(&before.draft, intent, &reason)?;
    let revision = before.latest_revision.unwrap_or(0) + 1;
    let revision_kind = match intent {
        ContentSnapshotIntent::Manual => ContentRevisionKindV2::Manual,
        ContentSnapshotIntent::Publish => ContentRevisionKindV2::Publish,
    };
    insert_revision(
        &mut transaction,
        id,
        revision,
        &before.draft,
        revision_kind,
        &reason,
        &metadata.actor,
    )
    .await?;
    let published_revision = matches!(intent, ContentSnapshotIntent::Publish)
        .then_some(revision)
        .or(before.published_revision);
    let status = if published_revision.is_some() {
        CmsPublicationStatusV2::Published
    } else {
        before.status
    };
    sqlx::query(
        r#"UPDATE content_entries
           SET latest_revision=$2,cms_published_revision=$3,status=$4,
               cms_updated_by=$5
           WHERE id=$1 AND latest_revision=$6"#,
    )
    .bind(id)
    .bind(revision)
    .bind(published_revision)
    .bind(enum_label(status))
    .bind(&metadata.actor)
    .bind(before.latest_revision.unwrap_or(0))
    .execute(&mut *transaction)
    .await?;
    let record = ContentRecordV2 {
        status,
        latest_revision: Some(revision),
        published_revision,
        ..before.clone()
    };
    let action = match intent {
        ContentSnapshotIntent::Manual => "content.snapshot.create",
        ContentSnapshotIntent::Publish => "content.publish",
    };
    insert_audit(
        &mut transaction,
        &metadata,
        action,
        id,
        Some(json_value(&before)?),
        Some(json_value(&record)?),
        &reason,
    )
    .await?;
    finish(transaction, idempotency, &record, StatusCode::CREATED).await?;
    Ok(record)
}

#[allow(clippy::too_many_arguments)]
pub async fn restore_revision(
    state: &AppState,
    id: Uuid,
    target_revision: i64,
    expected: i64,
    reason: String,
    metadata: MutationMetadata,
    idempotency: IdempotencyContext,
) -> Result<ContentRecordV2, ApiError> {
    validate_reason(&reason)?;
    let pool = require_postgres(state)?;
    let mut transaction = pool.begin().await?;
    let before = lock_record(&mut transaction, id).await?;
    ensure_version(&before, expected, "restoring")?;
    let target = load_revision(&mut transaction, id, target_revision).await?;
    let mut restored = target.document;
    if restored.kind != before.draft.kind
        || restored.locale != before.draft.locale
        || restored.template_key != before.draft.template_key
    {
        transaction.rollback().await?;
        return Err(ApiError::conflict(
            "The historical revision does not match the immutable content identity.",
        ));
    }
    restored.draft_version = expected + 1;
    validate_save(&before.draft, &restored, expected + 1)?;
    let revision = before.latest_revision.unwrap_or(0) + 1;
    insert_revision(
        &mut transaction,
        id,
        revision,
        &restored,
        ContentRevisionKindV2::Restore,
        &reason,
        &metadata.actor,
    )
    .await?;
    let now = Utc::now();
    sqlx::query(
        r#"UPDATE content_drafts
           SET draft_version=$2,document=$3,updated_by=$4,updated_at=$5
           WHERE content_id=$1 AND draft_version=$6"#,
    )
    .bind(id)
    .bind(restored.draft_version)
    .bind(json_value(&restored)?)
    .bind(&metadata.actor)
    .bind(now)
    .bind(expected)
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        r#"UPDATE content_entries
           SET latest_revision=$2,status='draft',cms_updated_by=$3
           WHERE id=$1 AND latest_revision=$4"#,
    )
    .bind(id)
    .bind(revision)
    .bind(&metadata.actor)
    .bind(before.latest_revision.unwrap_or(0))
    .execute(&mut *transaction)
    .await?;
    let record = ContentRecordV2 {
        status: CmsPublicationStatusV2::Draft,
        draft: restored,
        latest_revision: Some(revision),
        updated_at: now,
        updated_by: metadata.actor.clone(),
        ..before.clone()
    };
    insert_audit(
        &mut transaction,
        &metadata,
        "content.revision.restore",
        id,
        Some(json_value(&before)?),
        Some(json_value(&record)?),
        &reason,
    )
    .await?;
    finish(transaction, idempotency, &record, StatusCode::CREATED).await?;
    Ok(record)
}

async fn insert_revision(
    transaction: &mut Transaction<'_, Postgres>,
    id: Uuid,
    revision: i64,
    draft: &ContentDraftV2,
    kind: ContentRevisionKindV2,
    reason: &str,
    actor: &str,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"INSERT INTO content_revisions
           (content_id,revision,payload,document,source_draft_version,
            revision_kind,reason,created_by,created_at)
           VALUES ($1,$2,NULL,$3,$4,$5,$6,$7,$8)"#,
    )
    .bind(id)
    .bind(revision)
    .bind(json_value(draft)?)
    .bind(draft.draft_version)
    .bind(enum_label(kind))
    .bind(reason.trim())
    .bind(actor)
    .bind(Utc::now())
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

async fn finish<T: serde::Serialize>(
    mut transaction: Transaction<'_, Postgres>,
    idempotency: IdempotencyContext,
    response: &T,
    status: StatusCode,
) -> Result<(), ApiError> {
    let staged = idempotency
        .stage_in_transaction(&mut transaction, response, status)
        .await?;
    transaction.commit().await?;
    staged.finish().await
}

fn ensure_version(record: &ContentRecordV2, expected: i64, action: &str) -> Result<(), ApiError> {
    if record.draft.draft_version == expected {
        Ok(())
    } else {
        Err(version_conflict(action))
    }
}

fn version_conflict(action: &str) -> ApiError {
    ApiError::conflict(format!(
        "The content draft changed; reload before {action}."
    ))
}

fn storage_slug(draft: &ContentDraftV2) -> String {
    draft
        .slug
        .clone()
        .unwrap_or_else(|| enum_label(draft.template_key))
}

fn json_value<T: serde::Serialize>(value: &T) -> Result<Value, ApiError> {
    serde_json::to_value(value)
        .map_err(|_| ApiError::internal("CMS response serialization failed."))
}
