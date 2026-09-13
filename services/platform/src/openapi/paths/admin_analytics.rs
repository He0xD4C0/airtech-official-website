//! Administrator intake and analytics path definitions.

use serde_json::{json, Map, Value};

use super::super::support::*;

/// Adds RFQ, contact, and high-level analytics endpoints.
pub(super) fn add_overview(paths: &mut Map<String, Value>) {
    add_business_inbox_paths(paths, "rfqs", "Rfq");
    add_business_inbox_paths(paths, "contacts", "Contact");
    add(
        paths,
        "/api/admin/v1/dashboard/summary",
        "get",
        admin(
            op(
                "getAdminDashboardSummary",
                "Get permission-aware server totals, publication work items, analytics and recent activity",
                "adminDashboard",
                [("200", json_response("Admin dashboard summary", r("AdminDashboardSummary")))],
            ),
            false,
        ),
    );
    let analytics_boundary = || {
        json!({
            "type": "string",
            "format": "date-time",
            "description": "RFC3339 UTC midnight boundary. from is inclusive and to is exclusive; supply both or neither."
        })
    };
    add(
        paths,
        "/api/admin/v1/analytics/overview",
        "get",
        admin(
            params(
                op(
                    "getAnalyticsOverview",
                    "Get a time-scoped first-party analytics overview",
                    "adminAnalytics",
                    [(
                        "200",
                        json_response("Analytics overview", r("AnalyticsOverview")),
                    )],
                ),
                vec![
                    query_param("from", false, analytics_boundary()),
                    query_param("to", false, analytics_boundary()),
                ],
            ),
            false,
        ),
    );
    let mut legacy_analytics_summary = admin(
        op(
            "getAnalyticsSummary",
            "Get legacy unscoped first-party analytics totals",
            "adminAnalytics",
            [(
                "200",
                json_response("Analytics summary", r("AnalyticsSummary")),
            )],
        ),
        false,
    );
    legacy_analytics_summary["deprecated"] = json!(true);
    legacy_analytics_summary["description"] = json!(
        "Deprecated compatibility endpoint. Use /api/admin/v1/analytics/overview for time-scoped metrics with consistent consented and business-outcome cohorts."
    );
    add(
        paths,
        "/api/admin/v1/analytics/summary",
        "get",
        legacy_analytics_summary,
    );
}

fn add_business_inbox_paths(paths: &mut Map<String, Value>, segment: &str, label: &str) {
    let collection = format!("/api/admin/v1/{segment}");
    let detail = format!("{collection}/{{id}}");
    let operation_label = if label == "Rfq" { "RFQ" } else { label };
    let mut list_parameters = admin_pagination_params();
    list_parameters.extend([
        query_param("q", false, json!({"type": "string", "maxLength": 200})),
        query_param("status", false, r("BusinessInboxStatus")),
        query_param("assignedTo", false, uuid()),
    ]);
    add(
        paths,
        &collection,
        "get",
        admin(
            params(
                op(
                    &format!("list{label}InboxItems"),
                    &format!("List always-redacted {operation_label} inbox items with server filtering and total count"),
                    "adminInbox",
                    [("200", json_response("Business inbox page", r("BusinessInboxPage")))],
                ),
                list_parameters,
            ),
            false,
        ),
    );
    add(
        paths,
        &detail,
        "get",
        admin(
            params(
                op(
                    &format!("get{label}InboxItem"),
                    &format!("Get an always-redacted {operation_label} item, immutable notes and status history"),
                    "adminInbox",
                    [("200", json_response("Business inbox detail", r("BusinessInboxDetail")))],
                ),
                vec![path_param("id", uuid())],
            ),
            false,
        ),
    );
    let mut pii_response = json_response("Explicit PII detail", r("BusinessPii"));
    pii_response["headers"] = json!({"Cache-Control": {"schema": {"type": "string", "const": "private, no-store, max-age=0"}}});
    add(
        paths,
        &format!("{detail}/pii"),
        "get",
        admin(
            params(
                op(
                    &format!("get{label}Pii"),
                    &format!("Read {operation_label} PII with no-store response and read audit"),
                    "adminInbox",
                    [("200", pii_response)],
                ),
                vec![path_param("id", uuid())],
            ),
            false,
        ),
    );
    for (suffix, operation, summary, request, response, status) in [
        (
            "assignment",
            format!("assign{label}InboxItem"),
            "Assign or unassign",
            "AssignBusinessInboxRequest",
            "BusinessInboxItem",
            "200",
        ),
        (
            "status",
            format!("update{label}InboxStatus"),
            "Transition status, including spam marking",
            "UpdateBusinessStatusRequest",
            "BusinessInboxItem",
            "200",
        ),
        (
            "notes",
            format!("create{label}InternalNote"),
            "Create an immutable internal note",
            "CreateBusinessNoteRequest",
            "BusinessInboxDetail",
            "201",
        ),
    ] {
        add(
            paths,
            &format!("{detail}/{suffix}"),
            "post",
            admin(
                params(
                    body(
                        op(
                            &operation,
                            &format!(
                                "{summary} for an {operation_label} item with a required reason"
                            ),
                            "adminInbox",
                            [(
                                status,
                                json_response("Business workflow result", r(response)),
                            )],
                        ),
                        r(request),
                    ),
                    idempotent_entity_params(),
                ),
                true,
            ),
        );
    }
}

/// Adds privacy-minimized analytics reporting endpoints.
pub(super) fn add_reports(paths: &mut Map<String, Value>) {
    let analytics_parameters = || {
        let mut parameters = vec![
            query_param("from", false, timestamp()),
            query_param("to", false, timestamp()),
        ];
        parameters.extend(admin_pagination_params());
        parameters
    };
    add(
        paths,
        "/api/admin/v1/analytics/visits",
        "get",
        admin(
            params(
                op(
                    "listGuestVisits",
                    "List privacy-minimized daily landing-page aggregates",
                    "adminAnalytics",
                    [(
                        "200",
                        json_response("Guest visit aggregates", r("GuestVisitAggregatePage")),
                    )],
                ),
                analytics_parameters(),
            ),
            false,
        ),
    );
    add(
        paths,
        "/api/admin/v1/analytics/sources",
        "get",
        admin(
            params(
                op(
                    "listGuestSources",
                    "List daily source and campaign aggregates",
                    "adminAnalytics",
                    [(
                        "200",
                        json_response("Guest source aggregates", r("GuestSourceDailyPage")),
                    )],
                ),
                analytics_parameters(),
            ),
            false,
        ),
    );
}
