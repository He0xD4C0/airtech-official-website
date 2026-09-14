async fn admin_dashboard_summary(
    State(state): State<AppState>,
    Extension(principal): Extension<AdminPrincipal>,
) -> Result<Json<crate::models::AdminDashboardSummary>, ApiError> {
    let draft_content = dashboard_count(
        &state,
        &principal,
        "content.read",
        "SELECT count(*)::bigint FROM content_entries WHERE status='draft'",
        || async {
            Ok(state
                .data
                .read()
                .await
                .content
                .values()
                .filter(|entry| entry.status == PublicationStatus::Draft)
                .count() as i64)
        },
    )
    .await?;
    let open_conflicts = dashboard_count(
        &state,
        &principal,
        "integration.run",
        "SELECT count(*)::bigint FROM sync_conflicts WHERE resolved_at IS NULL",
        || async {
            Ok(state
                .data
                .read()
                .await
                .conflicts
                .values()
                .filter(|entry| entry.resolved_at.is_none())
                .count() as i64)
        },
    )
    .await?;
    let open_rfqs = dashboard_count(
        &state,
        &principal,
        "rfq.read",
        "SELECT count(*)::bigint FROM rfq_submissions WHERE status NOT IN ('closed','spam')",
        || async {
            Ok(state
                .data
                .read()
                .await
                .rfqs
                .values()
                .filter(|entry| !matches!(entry.status.as_str(), "closed" | "spam"))
                .count() as i64)
        },
    )
    .await?;
    let analytics = if principal.has_permission("analytics.read") {
        Some(
            analytics_overview(
                State(state.clone()),
                Query(AnalyticsOverviewQuery::default()),
            )
            .await?
            .0
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
    let readiness_item_count = [draft_content.value, open_conflicts.value]
        .into_iter()
        .flatten()
        .sum();
    Ok(Json(crate::models::AdminDashboardSummary {
        generated_at: Utc::now(),
        draft_content,
        open_conflicts,
        open_rfqs,
        analytics,
        recent_activity,
        readiness_item_count,
    }))
}

async fn dashboard_count<F, Fut>(
    state: &AppState,
    principal: &AdminPrincipal,
    permission: &str,
    sql: &str,
    memory: F,
) -> Result<crate::models::DashboardMetric, ApiError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<i64, ApiError>>,
{
    if !principal.has_permission(permission) {
        return Ok(crate::models::DashboardMetric {
            available: false,
            value: None,
            unavailable_reason: Some(format!("The `{permission}` permission is required.")),
        });
    }
    let value = if let Some(pool) = &state.pool {
        sqlx::query_scalar::<_, i64>(sql).fetch_one(pool).await?
    } else {
        memory().await?
    };
    Ok(crate::models::DashboardMetric {
        available: true,
        value: Some(value),
        unavailable_reason: None,
    })
}
