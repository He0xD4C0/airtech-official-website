use super::*;

pub(super) async fn analytics_overview(
    State(state): State<AppState>,
    Query(query): Query<airtek_runtime::services::admin_analytics::OverviewQuery>,
) -> Result<Json<AnalyticsOverview>, ApiError> {
    Ok(Json(
        airtek_runtime::services::admin_analytics::overview(&state, query).await?,
    ))
}
