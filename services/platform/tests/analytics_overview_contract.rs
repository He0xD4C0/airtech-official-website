use airtek_platform::{
    models::{
        AnalyticsOverview, BusinessContact, ContactRequest, CreateContactRequest, CreateRfqRequest,
        GuestVisit, RfqJourney, RfqSubmission, StoredAnalyticsEvent,
    },
    routes::admin,
    AppState, Config,
};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use chrono::{DateTime, Duration, TimeZone, Utc};
use http_body_util::BodyExt;
use tower::ServiceExt;
use uuid::Uuid;

async fn overview(state: &AppState, query: &str) -> axum::response::Response {
    admin::router()
        .with_state(state.clone())
        .oneshot(
            Request::get(format!("/analytics/overview{query}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

async fn overview_json(state: &AppState, query: &str) -> (AnalyticsOverview, String) {
    let response = overview(state, query).await;
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8(bytes.to_vec()).unwrap();
    (serde_json::from_str(&body).unwrap(), body)
}

fn visit(id: Uuid, at: DateTime<Utc>) -> GuestVisit {
    GuestVisit {
        id,
        anonymous_session_id: Uuid::new_v4(),
        landing_path: "/en".into(),
        referrer_domain: None,
        source: "direct".into(),
        medium: None,
        campaign: None,
        first_seen_at: at,
        last_seen_at: at,
        retention_until: at + Duration::days(180),
    }
}

fn stored_event(guest_visit_id: Uuid, event_name: &str, at: DateTime<Utc>) -> StoredAnalyticsEvent {
    StoredAnalyticsEvent {
        event_name: event_name.into(),
        guest_visit_id,
        occurred_at: at,
    }
}

fn business_contact() -> BusinessContact {
    BusinessContact {
        name: "Must not appear in Analytics".into(),
        email: "private@example.test".into(),
        phone: Some("+86 10000000000".into()),
        company: Some("Private company".into()),
        country_or_region: None,
    }
}

#[tokio::test]
async fn in_memory_overview_separates_consented_metrics_from_business_outcomes() {
    let state = AppState::new(Config::for_test()).unwrap();
    let first_day = Utc.with_ymd_and_hms(2026, 9, 1, 12, 0, 0).unwrap();
    let second_day = Utc.with_ymd_and_hms(2026, 9, 2, 12, 0, 0).unwrap();
    let outside = Utc.with_ymd_and_hms(2026, 8, 31, 12, 0, 0).unwrap();
    let first_visit_id = Uuid::new_v4();
    let second_visit_id = Uuid::new_v4();
    let outside_visit_id = Uuid::new_v4();
    {
        let mut data = state.data.write().await;
        data.guest_visits
            .insert(first_visit_id, visit(first_visit_id, first_day));
        data.guest_visits
            .insert(second_visit_id, visit(second_visit_id, second_day));
        data.guest_visits
            .insert(outside_visit_id, visit(outside_visit_id, outside));
        for (guest_visit_id, event_name, at) in [
            (first_visit_id, "pageView", first_day),
            (first_visit_id, "pageView", first_day + Duration::minutes(1)),
            (second_visit_id, "rfqStarted", second_day),
            (
                second_visit_id,
                "rfqSubmitted",
                second_day + Duration::minutes(1),
            ),
            (outside_visit_id, "pageView", outside),
        ] {
            data.analytics_events
                .insert(Uuid::new_v4(), stored_event(guest_visit_id, event_name, at));
        }
        let rfq_id = Uuid::new_v4();
        data.rfqs.insert(
            rfq_id,
            RfqSubmission {
                id: rfq_id,
                reference: "RFQ-OVERVIEW".into(),
                request: CreateRfqRequest {
                    journey: RfqJourney::Selection,
                    contact: business_contact(),
                    product_context: None,
                    source_path: "/en/request-a-quote/selection".into(),
                    locale: "en".into(),
                    consent: true,
                    context: Default::default(),
                },
                status: "new".into(),
                submitted_at: second_day,
                retention_until: second_day + Duration::days(365),
            },
        );
        let contact_id = Uuid::new_v4();
        data.contacts.insert(
            contact_id,
            ContactRequest {
                id: contact_id,
                reference: "CONTACT-OVERVIEW".into(),
                request: CreateContactRequest {
                    contact: business_contact(),
                    topic: "Private topic".into(),
                    message: "Private message".into(),
                    source_path: "/en/company/contact".into(),
                    locale: "en".into(),
                    consent: true,
                },
                status: "new".into(),
                submitted_at: second_day,
                retention_until: second_day + Duration::days(365),
            },
        );
    }

    let (value, raw_body) =
        overview_json(&state, "?from=2026-09-01T00:00:00Z&to=2026-09-03T00:00:00Z").await;
    assert_eq!(value.consented_metrics.visits, 2);
    assert_eq!(value.consented_metrics.page_views, 2);
    assert_eq!(value.consented_metrics.engaged_visit_days, 2);
    assert_eq!(value.consented_metrics.rfq_start_events, 1);
    assert_eq!(value.consented_metrics.rfq_submit_events, 1);
    assert_eq!(value.business_outcomes.rfq_submissions, 1);
    assert_eq!(value.business_outcomes.contact_requests, 1);
    assert_eq!(value.range.timezone, "UTC");
    assert!(!value.contains_pii);
    for forbidden in [
        "anonymousSessionId",
        "guestVisitId",
        "private@example.test",
        "Private company",
        "Private message",
    ] {
        assert!(!raw_body.contains(forbidden));
    }
}

#[tokio::test]
async fn overview_requires_bounded_paired_utc_midnight_parameters() {
    let state = AppState::new(Config::for_test()).unwrap();
    for query in [
        "?from=2026-09-01T00:00:00Z",
        "?from=2026-09-01T00:00:00%2B08:00&to=2026-09-02T00:00:00%2B08:00",
        "?from=2026-09-01T00:00:01Z&to=2026-09-02T00:00:00Z",
        "?from=2026-09-02T00:00:00Z&to=2026-09-02T00:00:00Z",
        "?from=2025-01-01T00:00:00Z&to=2026-01-03T00:00:00Z",
        "?from=2999-01-01T00:00:00Z&to=2999-01-02T00:00:00Z",
    ] {
        assert_eq!(
            overview(&state, query).await.status(),
            StatusCode::BAD_REQUEST
        );
    }
}

#[tokio::test]
async fn overview_includes_from_and_excludes_to_exactly() {
    let state = AppState::new(Config::for_test()).unwrap();
    let from = Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap();
    let to = Utc.with_ymd_and_hms(2026, 9, 2, 0, 0, 0).unwrap();
    let included_visit_id = Uuid::new_v4();
    let excluded_visit_id = Uuid::new_v4();
    {
        let mut data = state.data.write().await;
        data.guest_visits
            .insert(included_visit_id, visit(included_visit_id, from));
        data.guest_visits
            .insert(excluded_visit_id, visit(excluded_visit_id, to));
        data.analytics_events.insert(
            Uuid::new_v4(),
            stored_event(included_visit_id, "pageView", from),
        );
        data.analytics_events.insert(
            Uuid::new_v4(),
            stored_event(excluded_visit_id, "pageView", to),
        );
    }

    let (value, _) =
        overview_json(&state, "?from=2026-09-01T00:00:00Z&to=2026-09-02T00:00:00Z").await;
    assert_eq!(value.consented_metrics.visits, 1);
    assert_eq!(value.consented_metrics.page_views, 1);
}

#[tokio::test]
async fn overview_defaults_to_thirty_utc_calendar_days() {
    let state = AppState::new(Config::for_test()).unwrap();
    let (value, _) = overview_json(&state, "").await;
    assert_eq!(value.range.timezone, "UTC");
    assert_eq!(
        value.range.to_exclusive - value.range.from,
        Duration::days(30)
    );
    assert_eq!(value.range.from.time(), chrono::NaiveTime::MIN);
    assert_eq!(value.range.to_exclusive.time(), chrono::NaiveTime::MIN);
}

#[test]
fn openapi_exposes_overview_and_deprecates_the_legacy_summary() {
    let document = airtek_platform::openapi::document();
    let overview = &document["paths"]["/api/admin/v1/analytics/overview"]["get"];
    assert_eq!(overview["operationId"], "getAnalyticsOverview");
    let parameters = overview["parameters"].as_array().unwrap();
    assert!(parameters.iter().any(|value| value["name"] == "from"));
    assert!(parameters.iter().any(|value| value["name"] == "to"));
    assert_eq!(
        overview["responses"]["200"]["content"]["application/json"]["schema"]["$ref"],
        "#/components/schemas/AnalyticsOverview"
    );
    assert_eq!(
        document["paths"]["/api/admin/v1/analytics/summary"]["get"]["deprecated"],
        true
    );
    assert_eq!(
        document["components"]["schemas"]["AnalyticsOverview"]["required"],
        serde_json::json!([
            "range",
            "generatedAt",
            "consentedMetrics",
            "businessOutcomes",
            "source",
            "containsPii"
        ])
    );
}
