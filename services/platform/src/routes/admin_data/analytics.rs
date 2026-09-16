use super::*;

pub(super) async fn list_guest_sources(
    State(state): State<AppState>,
    Query(query): Query<AnalyticsQuery>,
) -> Result<Json<CursorPage<GuestSourceDaily>>, ApiError> {
    Ok(Json(
        crate::services::admin_analytics::list_guest_sources(
            &state,
            query.from,
            query.to,
            query.pagination(),
        )
        .await?,
    ))
}
