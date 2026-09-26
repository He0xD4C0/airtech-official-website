use super::*;

pub(super) async fn list_audit(
    State(state): State<AppState>,
    Query(query): Query<airtek_runtime::services::audit_query::AuditQuery>,
) -> Result<Json<airtek_domain::models::AuditEventPage>, ApiError> {
    Ok(Json(
        airtek_runtime::services::audit_query::list(&state, query).await?,
    ))
}

pub(super) async fn export_audit_csv(
    State(state): State<AppState>,
    Query(query): Query<airtek_runtime::services::audit_query::AuditQuery>,
) -> Result<Response, ApiError> {
    let stream = airtek_runtime::services::audit_query::csv_stream(state.pool.clone(), query)?;
    let mut response = Response::new(axum::body::Body::from_stream(stream));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=airtek-audit.csv"),
    );
    Ok(response)
}

pub(super) fn entity_response<T: serde::Serialize>(
    status: StatusCode,
    value: &T,
    revision: i64,
) -> Response {
    let mut response = (status, Json(value)).into_response();
    response.headers_mut().insert(
        "etag",
        HeaderValue::from_str(&etag(revision)).expect("revision ETag is valid"),
    );
    response
}
