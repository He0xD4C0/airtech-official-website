#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateAnalyticsConsent {
    pub anonymous_session_id: Uuid,
    pub policy_version: String,
    pub analytics_allowed: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsConsentReceipt {
    pub consent_receipt: Uuid,
    pub anonymous_session_id: Uuid,
    pub policy_version: String,
    pub analytics_allowed: bool,
    pub granted_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateAnalyticsEvent {
    pub event_name: String,
    pub anonymous_session_id: Option<Uuid>,
    pub source_path: String,
    pub locale: String,
    pub consent_granted: bool,
    pub policy_version: Option<String>,
    pub consent_receipt: Option<Uuid>,
    #[serde(default)]
    pub properties: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsEventReceipt {
    pub accepted: bool,
    pub event_id: Option<Uuid>,
}

#[derive(Clone, Debug)]
pub struct StoredAnalyticsEvent {
    pub event_name: String,
    pub guest_visit_id: Uuid,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsOverviewRange {
    pub from: DateTime<Utc>,
    pub to_exclusive: DateTime<Utc>,
    pub timezone: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsConsentedMetrics {
    pub visits: i64,
    pub page_views: i64,
    pub engaged_visit_days: i64,
    pub rfq_start_events: i64,
    pub rfq_submit_events: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsBusinessOutcomes {
    pub rfq_submissions: i64,
    pub contact_requests: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsOverview {
    pub range: AnalyticsOverviewRange,
    pub generated_at: DateTime<Utc>,
    pub consented_metrics: AnalyticsConsentedMetrics,
    pub business_outcomes: AnalyticsBusinessOutcomes,
    pub source: String,
    pub contains_pii: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardMetric {
    pub available: bool,
    pub value: Option<i64>,
    pub unavailable_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdminDashboardSummary {
    pub generated_at: DateTime<Utc>,
    pub draft_content: DashboardMetric,
    pub open_conflicts: DashboardMetric,
    pub open_rfqs: DashboardMetric,
    pub analytics: Option<AnalyticsConsentedMetrics>,
    pub recent_activity: Vec<AuditEvent>,
    pub readiness_item_count: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateGuestVisit {
    pub anonymous_session_id: Uuid,
    pub consent_receipt: Uuid,
    pub policy_version: String,
    pub landing_path: String,
    pub referrer_domain: Option<String>,
    pub source: Option<String>,
    pub medium: Option<String>,
    pub campaign: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuestVisit {
    pub id: Uuid,
    pub anonymous_session_id: Uuid,
    pub landing_path: String,
    pub referrer_domain: Option<String>,
    pub source: String,
    pub medium: Option<String>,
    pub campaign: Option<String>,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub retention_until: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuestSourceDaily {
    pub bucket_date: chrono::NaiveDate,
    pub source: String,
    pub source_name: Option<String>,
    pub referrer_domain: Option<String>,
    pub utm_source: Option<String>,
    pub medium: Option<String>,
    pub campaign: Option<String>,
    pub landing_path: String,
    pub locale: String,
    pub visits: i64,
    pub page_views: i64,
    pub rfq_starts: i64,
    pub rfq_submissions: i64,
}
