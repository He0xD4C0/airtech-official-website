use super::*;

pub(super) async fn list_sync_runs(
    State(state): State<AppState>,
    Query(query): Query<CursorQuery>,
) -> Result<Json<CursorPage<SyncRun>>, ApiError> {
    Ok(Json(
        crate::services::admin_sync::list_sync_runs(&state, query).await?,
    ))
}

pub(super) async fn get_feishu_connection_status(
    State(state): State<AppState>,
) -> Result<Json<crate::models::FeishuConnectionStatus>, ApiError> {
    Ok(Json(
        crate::services::admin_sync::connection_status(&state).await?,
    ))
}

pub(super) async fn list_feishu_mappings(
    State(state): State<AppState>,
    Query(query): Query<CursorQuery>,
) -> Result<Json<CursorPage<crate::models::SyncMapping>>, ApiError> {
    Ok(Json(
        crate::services::admin_sync::list_mappings(&state, query).await?,
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StagingListQuery {
    pub(super) cursor: Option<String>,
    pub(super) limit: Option<usize>,
    pub(super) sync_run_id: Option<Uuid>,
    pub(super) status: Option<String>,
    pub(super) q: Option<String>,
}

pub(super) async fn list_feishu_staging(
    State(state): State<AppState>,
    Query(query): Query<StagingListQuery>,
) -> Result<Json<crate::models::StagingRecordPage>, ApiError> {
    let status = query
        .status
        .map(|value| {
            serde_json::from_value::<crate::models::StagingValidationStatus>(Value::String(value))
                .map(serialized_enum_label)
                .map_err(|_| ApiError::bad_request("status is not a staging validation state."))
        })
        .transpose()?;
    let search = parse_query_text(query.q)?;
    Ok(Json(
        crate::services::admin_sync::list_staging(
            &state,
            crate::services::admin_sync::StagingFilter {
                sync_run_id: query.sync_run_id,
                status,
                search,
            },
            CursorQuery {
                cursor: query.cursor,
                limit: query.limit,
            },
        )
        .await?,
    ))
}

pub(super) async fn start_sync_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<StartSyncRequest>,
) -> Result<Response, ApiError> {
    let run =
        crate::services::feishu::queue_sync_run(&state, request.run_kind, &actor(&headers)).await?;
    let mut response = (StatusCode::ACCEPTED, Json(run.clone())).into_response();
    response.headers_mut().insert(
        header::LOCATION,
        HeaderValue::from_str(&format!("/api/admin/v1/feishu/sync-runs/{}", run.id))
            .expect("sync run URL is valid"),
    );
    Ok(response)
}

pub(super) async fn get_feishu_settings_route(
    State(state): State<AppState>,
) -> Result<Response, ApiError> {
    let settings = crate::services::feishu::get_feishu_settings(&state).await?;
    let mut response = entity_response(StatusCode::OK, &settings, settings.revision);
    add_private_no_store_headers(&mut response);
    Ok(response)
}

pub(super) async fn update_feishu_settings_route(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<crate::models::UpdateFeishuSettings>,
) -> Result<Response, ApiError> {
    let expected = parse_if_match(&headers)?;
    let request_id = headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| Uuid::parse_str(value).ok())
        .unwrap_or_else(Uuid::new_v4);
    let settings = crate::services::feishu::update_feishu_settings(
        &state,
        expected,
        &input,
        &actor(&headers),
        request_id,
    )
    .await?;
    let mut response = entity_response(StatusCode::OK, &settings, settings.revision);
    add_private_no_store_headers(&mut response);
    Ok(response)
}

pub(super) async fn test_feishu_connection_route(
    State(state): State<AppState>,
) -> Result<Response, ApiError> {
    let result = crate::services::feishu::test_feishu_connection(&state).await?;
    let mut response = (StatusCode::OK, Json(result)).into_response();
    add_private_no_store_headers(&mut response);
    Ok(response)
}

pub(super) async fn get_sync_run(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::models::FeishuSyncRunDetail>, ApiError> {
    Ok(Json(
        crate::services::feishu::load_sync_run_detail(&state, id).await?,
    ))
}

pub(super) async fn rollback_sync_run_route(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<crate::models::FeishuRollbackReport>, ApiError> {
    Ok(Json(
        crate::services::feishu::rollback_sync_run(&state, id, &actor(&headers)).await?,
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ConflictListQuery {
    pub(super) cursor: Option<String>,
    pub(super) limit: Option<usize>,
    pub(super) q: Option<String>,
    pub(super) open_only: Option<bool>,
}

pub(super) async fn list_conflicts(
    State(state): State<AppState>,
    Query(query): Query<ConflictListQuery>,
) -> Result<Json<crate::models::SyncConflictPage>, ApiError> {
    let filter = crate::services::admin_sync::ConflictFilter {
        search: parse_query_text(query.q)?,
        open_only: query.open_only.unwrap_or(true),
    };
    Ok(Json(
        crate::services::admin_sync::list_conflicts(
            &state,
            filter,
            CursorQuery {
                cursor: query.cursor,
                limit: query.limit,
            },
        )
        .await?,
    ))
}

pub(super) fn serialized_enum_label(value: impl serde::Serialize) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}
