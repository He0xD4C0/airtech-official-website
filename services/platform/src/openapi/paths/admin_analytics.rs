//! Administrator intake and analytics path definitions.

use serde_json::{json, Map, Value};

use super::super::support::*;

/// Adds RFQ, contact, and high-level analytics endpoints.
pub(super) fn add_overview(paths: &mut Map<String, Value>) {
    add_admin_list(
        paths,
        "/api/admin/v1/rfqs",
        "listRfqSubmissions",
        "List RFQs with permission-aware PII redaction",
        "RfqSubmissionPage",
    );
    add_admin_list(
        paths,
        "/api/admin/v1/contacts",
        "listContactRequests",
        "List contacts with permission-aware PII redaction",
        "ContactRequestPage",
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
