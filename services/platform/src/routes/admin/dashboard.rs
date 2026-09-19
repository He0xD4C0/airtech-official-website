use super::*;

pub(super) async fn admin_dashboard_summary(
    State(state): State<AppState>,
    Extension(principal): Extension<AdminPrincipal>,
) -> Result<Json<crate::models::AdminDashboardSummary>, ApiError> {
    let draft_content = dashboard_count(
        &state,
        &principal,
        "content.read",
        crate::services::admin_analytics::DashboardMetricKind::DraftContent,
    )
    .await?;
    let open_rfqs = dashboard_count(
        &state,
        &principal,
        "rfq.read",
        crate::services::admin_analytics::DashboardMetricKind::OpenRfqs,
    )
    .await?;
    let analytics = if principal.has_permission("analytics.read") {
        Some(
            crate::services::admin_analytics::overview(
                &state,
                crate::services::admin_analytics::OverviewQuery::default(),
            )
            .await?
            .consented_metrics,
        )
    } else {
        None
    };
    let recent_activity = if principal.has_permission("audit.read") {
        let mut events = state.list_stored_audit().await?;
        events.sort_by_key(|event| Reverse((event.occurred_at, event.id)));
        events.truncate(10);
        events
    } else {
        Vec::new()
    };
    let readiness_item_count = draft_content.value.unwrap_or_default();
    Ok(Json(crate::models::AdminDashboardSummary {
        generated_at: Utc::now(),
        draft_content,
        open_rfqs,
        analytics,
        recent_activity,
        readiness_item_count,
    }))
}

pub(super) async fn dashboard_count(
    state: &AppState,
    principal: &AdminPrincipal,
    permission: &str,
    kind: crate::services::admin_analytics::DashboardMetricKind,
) -> Result<crate::models::DashboardMetric, ApiError> {
    if !principal.has_permission(permission) {
        return Ok(crate::models::DashboardMetric {
            available: false,
            value: None,
            unavailable_reason: Some(format!("The `{permission}` permission is required.")),
        });
    }
    crate::services::admin_analytics::dashboard_metric(state, kind).await
}
