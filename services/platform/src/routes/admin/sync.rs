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
    State(_state): State<AppState>,
    _headers: HeaderMap,
    Json(_request): Json<StartSyncRequest>,
) -> Result<Response, ApiError> {
    Err(ApiError::conflict(
        "Feishu provider adapter is not connected; synchronization is disabled.",
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
