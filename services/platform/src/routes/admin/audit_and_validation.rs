async fn list_audit(
    State(state): State<AppState>,
    Query(query): Query<CursorQuery>,
) -> Result<Json<CursorPage<AuditEvent>>, ApiError> {
    let mut values = state.list_stored_audit().await?;
    values.sort_by_key(|event| Reverse(event.occurred_at));
    Ok(Json(paginate_by_id(
        "admin.audit",
        values,
        query,
        |event| event.id,
    )?))
}

fn confirmation_phrase(kind: OperationKind) -> &'static str {
    match kind {
        OperationKind::MigrationPreflight => "PREFLIGHT MIGRATION",
        OperationKind::MigrationApply => "APPLY MIGRATION",
        OperationKind::Backup => "CREATE BACKUP",
        OperationKind::RestoreValidate => "VALIDATE RESTORE",
        OperationKind::RetentionApply => "APPLY RETENTION",
        OperationKind::SearchReindex => "REBUILD SEARCH INDEX",
        OperationKind::CacheInvalidate => "INVALIDATE PUBLIC CACHE",
        OperationKind::FeishuSync => "START FEISHU SYNC",
        OperationKind::ProductImport => "IMPORT PRODUCT MASTER",
    }
}

fn entity_response<T: serde::Serialize>(status: StatusCode, value: &T, revision: i64) -> Response {
    let mut response = (status, Json(value)).into_response();
    response.headers_mut().insert(
        "etag",
        HeaderValue::from_str(&etag(revision)).expect("revision ETag is valid"),
    );
    response
}
