use super::*;

pub(super) async fn list_sync_runs(
    State(state): State<AppState>,
    Query(query): Query<CursorQuery>,
) -> Result<Json<CursorPage<SyncRun>>, ApiError> {
    Ok(Json(
        airtek_runtime::services::admin_sync::list_sync_runs(&state, query).await?,
    ))
}

pub(super) async fn get_feishu_connection_status(
    State(state): State<AppState>,
) -> Result<Json<airtek_domain::models::FeishuConnectionStatus>, ApiError> {
    Ok(Json(
        airtek_runtime::services::admin_sync::connection_status(&state).await?,
    ))
}

pub(super) async fn list_feishu_mappings(
    State(state): State<AppState>,
    Query(query): Query<CursorQuery>,
) -> Result<Json<CursorPage<airtek_domain::models::SyncMapping>>, ApiError> {
    Ok(Json(
        airtek_runtime::services::admin_sync::list_mappings(&state, query).await?,
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
) -> Result<Json<airtek_domain::models::StagingRecordPage>, ApiError> {
    let status = query
        .status
        .map(|value| {
            serde_json::from_value::<airtek_domain::models::StagingValidationStatus>(Value::String(
                value,
            ))
            .map(serialized_enum_label)
            .map_err(|_| ApiError::bad_request("status is not a staging validation state."))
        })
        .transpose()?;
    let search = parse_query_text(query.q)?;
    Ok(Json(
        airtek_runtime::services::admin_sync::list_staging(
            &state,
            airtek_runtime::services::admin_sync::StagingFilter {
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
) -> Result<Response, ApiError> {
    let run = airtek_runtime::services::feishu::queue_full_sync(
        &state,
        airtek_domain::models::FeishuSyncTrigger::Manual,
        &actor(&headers),
    )
    .await?;
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
    let settings = airtek_runtime::services::feishu::get_feishu_settings(&state).await?;
    let mut response = entity_response(StatusCode::OK, &settings, settings.revision);
    add_private_no_store_headers(&mut response);
    Ok(response)
}

pub(super) async fn update_feishu_settings_route(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<airtek_domain::models::UpdateFeishuSettings>,
) -> Result<Response, ApiError> {
    let expected = parse_if_match(&headers)?;
    let request_id = headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| Uuid::parse_str(value).ok())
        .unwrap_or_else(Uuid::new_v4);
    let settings = airtek_runtime::services::feishu::update_feishu_settings(
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
    let result = airtek_runtime::services::feishu::test_feishu_connection(&state).await?;
    let mut response = (StatusCode::OK, Json(result)).into_response();
    add_private_no_store_headers(&mut response);
    Ok(response)
}

pub(super) async fn get_sync_run(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<airtek_domain::models::FeishuSyncRunDetail>, ApiError> {
    Ok(Json(
        airtek_runtime::services::feishu::load_sync_run_detail(&state, id).await?,
    ))
}

pub(super) fn serialized_enum_label(value: impl serde::Serialize) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}
