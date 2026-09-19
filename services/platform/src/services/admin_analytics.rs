use chrono::{DateTime, Duration, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::{
    error::ApiError,
    models::{
        AnalyticsBusinessOutcomes, AnalyticsConsentedMetrics, AnalyticsOverview,
        AnalyticsOverviewRange, CursorPage, DashboardMetric, GuestSourceDaily,
    },
    pagination::{
        cursor_limit, decode_scoped_cursor_compat, encode_scoped_cursor, CursorQuery, DecodedCursor,
    },
    services::request_metrics::LegacyCursorEndpoint,
    state::AppState,
};

const DEFAULT_DAYS: i64 = 30;
const MAX_DAYS: i64 = 366;

#[derive(Debug, Default, Deserialize)]
pub struct OverviewQuery {
    pub from: Option<String>,
    pub to: Option<String>,
}

#[derive(Clone, Copy)]
struct OverviewBounds {
    from: DateTime<Utc>,
    to_exclusive: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
struct AnalyticsCursor {
    bucket_date: chrono::NaiveDate,
    dimension_hash: Vec<u8>,
}

pub enum DashboardMetricKind {
    DraftContent,
    OpenRfqs,
}

pub async fn list_guest_sources(
    state: &AppState,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    pagination: CursorQuery,
) -> Result<CursorPage<GuestSourceDaily>, ApiError> {
    if from.zip(to).is_some_and(|(from, to)| from >= to) {
        return Err(ApiError::bad_request("from must be before to."));
    }
    let scope = analytics_scope("admin.analytics.sources", from, to);
    let limit = cursor_limit(&pagination)?;
    let after = decode_cursor(state, &scope, pagination.cursor.as_deref())?;
    let rows = sqlx::query(ANALYTICS_SOURCE_SQL)
        .bind(from)
        .bind(to)
        .bind(after.as_ref().map(|cursor| cursor.bucket_date))
        .bind(
            after
                .as_ref()
                .map(|cursor| cursor.dimension_hash.as_slice()),
        )
        .bind((limit + 1) as i64)
        .fetch_all(&state.pool)
        .await?;
    let mut values = rows
        .into_iter()
        .map(|row| {
            if row.try_get::<i64, _>("hash_count")? != 1 {
                return Err(ApiError::service_unavailable(
                    "Analytics dimension identity collision detected.",
                ));
            }
            let dimension_hash: Vec<u8> = row.try_get("dimension_hash")?;
            if dimension_hash.len() != 32 {
                return Err(ApiError::service_unavailable(
                    "Stored analytics dimension identity is invalid.",
                ));
            }
            let cursor = AnalyticsCursor {
                bucket_date: row.try_get("bucket_date")?,
                dimension_hash,
            };
            Ok((
                GuestSourceDaily {
                    bucket_date: row.try_get("bucket_date")?,
                    source: row.try_get("source_type")?,
                    source_name: row.try_get("source_name")?,
                    referrer_domain: row.try_get("referrer_domain")?,
                    utm_source: row.try_get("utm_source")?,
                    medium: row.try_get("utm_medium")?,
                    campaign: row.try_get("utm_campaign")?,
                    landing_path: row.try_get("landing_path")?,
                    locale: row.try_get("locale")?,
                    visits: row.try_get("visits")?,
                    page_views: row.try_get("page_views")?,
                    rfq_starts: row.try_get("rfq_starts")?,
                    rfq_submissions: row.try_get("rfq_submissions")?,
                },
                cursor,
            ))
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    let has_more = values.len() > limit;
    values.truncate(limit);
    let next_cursor = has_more
        .then(|| values.last())
        .flatten()
        .map(|(_, cursor)| encode_scoped_cursor(&scope, cursor))
        .transpose()?;
    Ok(CursorPage {
        items: values.into_iter().map(|(value, _)| value).collect(),
        next_cursor,
    })
}

fn decode_cursor(
    state: &AppState,
    scope: &str,
    value: Option<&str>,
) -> Result<Option<AnalyticsCursor>, ApiError> {
    let Some(value) = value else { return Ok(None) };
    let cursor =
        match decode_scoped_cursor_compat::<AnalyticsCursor, AnalyticsCursor>(scope, value)? {
            DecodedCursor::Current(cursor) => cursor,
            DecodedCursor::Legacy(cursor) => {
                state
                    .request_metrics
                    .record_legacy_cursor(LegacyCursorEndpoint::Analytics);
                cursor
            }
        };
    if cursor.dimension_hash.len() != 32 {
        return Err(ApiError::bad_request("cursor is invalid."));
    }
    Ok(Some(cursor))
}

fn analytics_scope(
    endpoint: &str,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
) -> String {
    let bound = |value: Option<DateTime<Utc>>| {
        value
            .map(|value| value.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true))
            .unwrap_or_else(|| "*".into())
    };
    format!("{endpoint}|from={}|to={}", bound(from), bound(to))
}

pub async fn overview(
    state: &AppState,
    query: OverviewQuery,
) -> Result<AnalyticsOverview, ApiError> {
    let bounds = overview_bounds(query, Utc::now())?;
    let row = sqlx::query(ANALYTICS_OVERVIEW_SQL)
        .bind(bounds.from)
        .bind(bounds.to_exclusive)
        .fetch_one(&state.pool)
        .await?;
    Ok(AnalyticsOverview {
        range: AnalyticsOverviewRange {
            from: bounds.from,
            to_exclusive: bounds.to_exclusive,
            timezone: "UTC".into(),
        },
        generated_at: row.try_get("generated_at")?,
        consented_metrics: AnalyticsConsentedMetrics {
            visits: row.try_get("visits")?,
            page_views: row.try_get("page_views")?,
            engaged_visit_days: row.try_get("engaged_visit_days")?,
            rfq_start_events: row.try_get("rfq_start_events")?,
            rfq_submit_events: row.try_get("rfq_submit_events")?,
        },
        business_outcomes: AnalyticsBusinessOutcomes {
            rfq_submissions: row.try_get("rfq_submissions")?,
            contact_requests: row.try_get("contact_requests")?,
        },
        source: "firstParty".into(),
        contains_pii: false,
    })
}

pub async fn dashboard_metric(
    state: &AppState,
    kind: DashboardMetricKind,
) -> Result<DashboardMetric, ApiError> {
    let sql = match kind {
        DashboardMetricKind::DraftContent => "SELECT count(*)::bigint FROM cms_drafts",
        DashboardMetricKind::OpenRfqs => {
            "SELECT count(*)::bigint FROM rfq_submissions WHERE status NOT IN ('closed','spam')"
        }
    };
    let value = sqlx::query_scalar::<_, i64>(sql)
        .fetch_one(&state.pool)
        .await?;
    Ok(DashboardMetric {
        available: true,
        value: Some(value),
        unavailable_reason: None,
    })
}

fn overview_bounds(query: OverviewQuery, now: DateTime<Utc>) -> Result<OverviewBounds, ApiError> {
    let latest_to_date = now
        .date_naive()
        .succ_opt()
        .ok_or_else(|| ApiError::internal("Unable to calculate the analytics range."))?;
    let latest_to =
        DateTime::<Utc>::from_naive_utc_and_offset(latest_to_date.and_time(NaiveTime::MIN), Utc);
    let bounds = match (query.from.as_deref(), query.to.as_deref()) {
        (None, None) => OverviewBounds {
            from: latest_to - Duration::days(DEFAULT_DAYS),
            to_exclusive: latest_to,
        },
        (Some(from), Some(to)) => OverviewBounds {
            from: parse_boundary("from", from)?,
            to_exclusive: parse_boundary("to", to)?,
        },
        _ => {
            return Err(ApiError::bad_request(
                "from and to must either both be supplied or both be omitted.",
            ))
        }
    };
    let duration = bounds.to_exclusive - bounds.from;
    if duration <= Duration::zero() {
        return Err(ApiError::bad_request("from must be before to."));
    }
    if duration > Duration::days(MAX_DAYS) {
        return Err(ApiError::bad_request(
            "Analytics overview ranges cannot exceed 366 UTC days.",
        ));
    }
    if bounds.to_exclusive > latest_to {
        return Err(ApiError::bad_request(
            "to cannot be later than the next UTC midnight.",
        ));
    }
    Ok(bounds)
}

fn parse_boundary(field: &str, value: &str) -> Result<DateTime<Utc>, ApiError> {
    let parsed = DateTime::parse_from_rfc3339(value).map_err(|_| {
        ApiError::bad_request(format!(
            "{field} must be an RFC3339 UTC date-time at 00:00:00."
        ))
    })?;
    if parsed.offset().local_minus_utc() != 0 || parsed.time() != NaiveTime::MIN {
        return Err(ApiError::bad_request(format!(
            "{field} must be an RFC3339 UTC date-time at 00:00:00."
        )));
    }
    Ok(parsed.with_timezone(&Utc))
}

const ANALYTICS_SOURCE_SQL: &str = r#"WITH metric_rows AS (
  SELECT bucket_date,source_type,source_name,utm_source,utm_medium,utm_campaign,landing_path,locale,
         visits,page_views,rfq_starts,rfq_submissions FROM guest_source_daily
  UNION ALL SELECT (first_seen_at AT TIME ZONE 'UTC')::date,source_type,
         COALESCE(referrer_host,''),COALESCE(utm_source,''),COALESCE(utm_medium,''),
         COALESCE(utm_campaign,''),landing_path,locale,COUNT(*)::bigint,0,0,0
    FROM guest_visits WHERE consent_analytics_allowed=true
    GROUP BY (first_seen_at AT TIME ZONE 'UTC')::date,source_type,COALESCE(referrer_host,''),
      COALESCE(utm_source,''),COALESCE(utm_medium,''),COALESCE(utm_campaign,''),landing_path,locale
  UNION ALL SELECT (event.occurred_at AT TIME ZONE 'UTC')::date,visit.source_type,
         COALESCE(visit.referrer_host,''),COALESCE(visit.utm_source,''),COALESCE(visit.utm_medium,''),
         COALESCE(visit.utm_campaign,''),visit.landing_path,visit.locale,0,
         COUNT(*) FILTER (WHERE event.event_name='pageView')::bigint,
         COUNT(*) FILTER (WHERE event.event_name='rfqStarted')::bigint,
         COUNT(*) FILTER (WHERE event.event_name='rfqSubmitted')::bigint
    FROM analytics_events event JOIN guest_visits visit ON visit.id=event.guest_visit_id
      AND visit.anonymous_session_id=event.anonymous_session_id
      AND visit.consent_record_id=event.consent_record_id
    WHERE visit.consent_analytics_allowed=true
      AND event.event_name IN ('pageView','rfqStarted','rfqSubmitted')
    GROUP BY (event.occurred_at AT TIME ZONE 'UTC')::date,visit.source_type,
      COALESCE(visit.referrer_host,''),COALESCE(visit.utm_source,''),COALESCE(visit.utm_medium,''),
      COALESCE(visit.utm_campaign,''),visit.landing_path,visit.locale
), aggregated AS (
  SELECT bucket_date,source_type,source_name,utm_source,utm_medium,utm_campaign,landing_path,locale,
         SUM(visits)::bigint visits,SUM(page_views)::bigint page_views,
         SUM(rfq_starts)::bigint rfq_starts,SUM(rfq_submissions)::bigint rfq_submissions
    FROM metric_rows WHERE ($1::timestamptz IS NULL OR bucket_date >= ($1 AT TIME ZONE 'UTC')::date)
      AND ($2::timestamptz IS NULL OR bucket_date < ($2 AT TIME ZONE 'UTC')::date)
    GROUP BY bucket_date,source_type,source_name,utm_source,utm_medium,utm_campaign,landing_path,locale
), hashed AS (
  SELECT aggregated.*,guest_source_dimension_hash(source_type,source_name,utm_source,utm_medium,
    utm_campaign,landing_path,locale) dimension_hash FROM aggregated
), guarded AS (
  SELECT hashed.*,COUNT(*) OVER (PARTITION BY bucket_date,dimension_hash)::bigint hash_count FROM hashed
) SELECT bucket_date,source_type,
  COALESCE(NULLIF(utm_source,''),NULLIF(source_name,''),source_type) source_name,
  NULLIF(source_name,'') referrer_domain,NULLIF(utm_source,'') utm_source,
  NULLIF(utm_medium,'') utm_medium,NULLIF(utm_campaign,'') utm_campaign,
  landing_path,locale,visits,page_views,rfq_starts,rfq_submissions,dimension_hash,hash_count
  FROM guarded WHERE ($3::date IS NULL OR bucket_date<$3 OR (bucket_date=$3 AND dimension_hash>$4::bytea))
  ORDER BY bucket_date DESC,dimension_hash LIMIT $5"#;

const ANALYTICS_OVERVIEW_SQL: &str = r#"WITH historical AS (
  SELECT COALESCE(SUM(visits),0)::bigint visits,COALESCE(SUM(page_views),0)::bigint page_views,
    COALESCE(SUM(engaged_visits),0)::bigint engaged_visit_days,
    COALESCE(SUM(rfq_starts),0)::bigint rfq_start_events,
    COALESCE(SUM(rfq_submissions),0)::bigint rfq_submit_events FROM guest_source_daily
    WHERE bucket_date>=($1 AT TIME ZONE 'UTC')::date AND bucket_date<($2 AT TIME ZONE 'UTC')::date
), raw_visits AS (
  SELECT COUNT(*)::bigint visits FROM guest_visits WHERE consent_analytics_allowed=true
    AND first_seen_at >= $1 AND first_seen_at < $2
), raw_event_days AS (
  SELECT event.guest_visit_id,(event.occurred_at AT TIME ZONE 'UTC')::date bucket_date,
    COUNT(*) FILTER (WHERE event.event_name='pageView')::bigint page_views,
    COUNT(*) FILTER (WHERE event.event_name='rfqStarted')::bigint rfq_start_events,
    COUNT(*) FILTER (WHERE event.event_name='rfqSubmitted')::bigint rfq_submit_events
    FROM analytics_events event JOIN guest_visits visit ON visit.id=event.guest_visit_id
      AND visit.anonymous_session_id=event.anonymous_session_id
      AND visit.consent_record_id=event.consent_record_id
    WHERE visit.consent_analytics_allowed=true AND event.occurred_at >= $1 AND event.occurred_at < $2
      AND event.event_name IN ('pageView','rfqStarted','rfqSubmitted')
    GROUP BY event.guest_visit_id,(event.occurred_at AT TIME ZONE 'UTC')::date
), raw_events AS (
  SELECT COALESCE(SUM(page_views),0)::bigint page_views,
    COALESCE(SUM(rfq_start_events),0)::bigint rfq_start_events,
    COALESCE(SUM(rfq_submit_events),0)::bigint rfq_submit_events FROM raw_event_days
), engagement_delta AS (
  SELECT COUNT(*)::bigint engaged_visit_days FROM raw_event_days raw
    LEFT JOIN guest_visit_daily_event_materializations ledger
      ON ledger.guest_visit_id=raw.guest_visit_id AND ledger.bucket_date=raw.bucket_date
    WHERE NOT COALESCE(ledger.engaged_materialized,false)
      AND (COALESCE(ledger.page_views,0)+raw.page_views>=2
        OR COALESCE(ledger.rfq_starts,0)+raw.rfq_start_events>0
        OR COALESCE(ledger.rfq_submissions,0)+raw.rfq_submit_events>0)
), business AS (
  SELECT (SELECT COUNT(*)::bigint FROM rfq_submissions WHERE submitted_at >= $1 AND submitted_at < $2) rfq_submissions,
    (SELECT COUNT(*)::bigint FROM contact_requests WHERE submitted_at >= $1 AND submitted_at < $2) contact_requests
) SELECT statement_timestamp() generated_at,historical.visits+raw_visits.visits visits,
  historical.page_views+raw_events.page_views page_views,
  historical.engaged_visit_days+engagement_delta.engaged_visit_days engaged_visit_days,
  historical.rfq_start_events+raw_events.rfq_start_events rfq_start_events,
  historical.rfq_submit_events+raw_events.rfq_submit_events rfq_submit_events,
  business.rfq_submissions,business.contact_requests
  FROM historical,raw_visits,raw_events,engagement_delta,business"#;
