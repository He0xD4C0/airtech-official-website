const ANALYTICS_OVERVIEW_DEFAULT_DAYS: i64 = 30;
const ANALYTICS_OVERVIEW_MAX_DAYS: i64 = 366;

#[derive(Debug, Default, Deserialize)]
struct AnalyticsOverviewQuery {
    from: Option<String>,
    to: Option<String>,
}

#[derive(Clone, Copy)]
struct AnalyticsOverviewBounds {
    from: DateTime<Utc>,
    to_exclusive: DateTime<Utc>,
}

async fn analytics_overview(
    State(state): State<AppState>,
    Query(query): Query<AnalyticsOverviewQuery>,
) -> Result<Json<AnalyticsOverview>, ApiError> {
    let bounds = analytics_overview_bounds(query, Utc::now())?;
    if let Some(pool) = &state.pool {
        // A single statement gives every metric one PostgreSQL MVCC snapshot.
        // The ledger contributes only to the not-yet-materialized engagement
        // decision: its event counters are already present in the daily totals.
        let row = sqlx::query(
            r#"WITH historical AS (
                   SELECT COALESCE(SUM(visits),0)::bigint AS visits,
                          COALESCE(SUM(page_views),0)::bigint AS page_views,
                          COALESCE(SUM(engaged_visits),0)::bigint AS engaged_visit_days,
                          COALESCE(SUM(rfq_starts),0)::bigint AS rfq_start_events,
                          COALESCE(SUM(rfq_submissions),0)::bigint AS rfq_submit_events
                   FROM guest_source_daily
                   WHERE bucket_date >= ($1 AT TIME ZONE 'UTC')::date
                     AND bucket_date < ($2 AT TIME ZONE 'UTC')::date
               ), raw_visits AS (
                   SELECT COUNT(*)::bigint AS visits
                   FROM guest_visits
                   WHERE consent_analytics_allowed=true
                     AND first_seen_at >= $1 AND first_seen_at < $2
               ), raw_event_days AS (
                   SELECT event.guest_visit_id,
                          (event.occurred_at AT TIME ZONE 'UTC')::date AS bucket_date,
                          COUNT(*) FILTER (WHERE event.event_name='pageView')::bigint
                              AS page_views,
                          COUNT(*) FILTER (WHERE event.event_name='rfqStarted')::bigint
                              AS rfq_start_events,
                          COUNT(*) FILTER (WHERE event.event_name='rfqSubmitted')::bigint
                              AS rfq_submit_events
                   FROM analytics_events AS event
                   INNER JOIN guest_visits AS visit
                       ON visit.id=event.guest_visit_id
                      AND visit.anonymous_session_id=event.anonymous_session_id
                      AND visit.consent_record_id=event.consent_record_id
                   WHERE visit.consent_analytics_allowed=true
                     AND event.occurred_at >= $1 AND event.occurred_at < $2
                     AND event.event_name IN ('pageView','rfqStarted','rfqSubmitted')
                   GROUP BY event.guest_visit_id,
                            (event.occurred_at AT TIME ZONE 'UTC')::date
               ), raw_events AS (
                   SELECT COALESCE(SUM(page_views),0)::bigint AS page_views,
                          COALESCE(SUM(rfq_start_events),0)::bigint AS rfq_start_events,
                          COALESCE(SUM(rfq_submit_events),0)::bigint AS rfq_submit_events
                   FROM raw_event_days
               ), engagement_delta AS (
                   SELECT COUNT(*)::bigint AS engaged_visit_days
                   FROM raw_event_days AS raw
                   LEFT JOIN guest_visit_daily_event_materializations AS ledger
                     ON ledger.guest_visit_id=raw.guest_visit_id
                    AND ledger.bucket_date=raw.bucket_date
                   WHERE NOT COALESCE(ledger.engaged_materialized,false)
                     AND (
                         COALESCE(ledger.page_views,0) + raw.page_views >= 2
                         OR COALESCE(ledger.rfq_starts,0) + raw.rfq_start_events > 0
                         OR COALESCE(ledger.rfq_submissions,0) + raw.rfq_submit_events > 0
                     )
               ), business AS (
                   SELECT
                       (SELECT COUNT(*)::bigint FROM rfq_submissions
                        WHERE submitted_at >= $1 AND submitted_at < $2) AS rfq_submissions,
                       (SELECT COUNT(*)::bigint FROM contact_requests
                        WHERE submitted_at >= $1 AND submitted_at < $2) AS contact_requests
               )
               SELECT statement_timestamp() AS generated_at,
                      historical.visits + raw_visits.visits AS visits,
                      historical.page_views + raw_events.page_views AS page_views,
                      historical.engaged_visit_days + engagement_delta.engaged_visit_days
                          AS engaged_visit_days,
                      historical.rfq_start_events + raw_events.rfq_start_events
                          AS rfq_start_events,
                      historical.rfq_submit_events + raw_events.rfq_submit_events
                          AS rfq_submit_events,
                      business.rfq_submissions,business.contact_requests
               FROM historical,raw_visits,raw_events,engagement_delta,business"#,
        )
        .bind(bounds.from)
        .bind(bounds.to_exclusive)
        .fetch_one(pool)
        .await?;
        return Ok(Json(AnalyticsOverview {
            range: analytics_overview_range(bounds),
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
        }));
    }

    let data = state.data.read().await;
    let generated_at = Utc::now();
    let mut consented_metrics = AnalyticsConsentedMetrics {
        visits: 0,
        page_views: 0,
        engaged_visit_days: 0,
        rfq_start_events: 0,
        rfq_submit_events: 0,
    };
    for visit in data.guest_visits.values() {
        if timestamp_in_analytics_bounds(visit.first_seen_at, bounds) {
            consented_metrics.visits += 1;
        }
    }
    let mut visit_days = BTreeMap::<(Uuid, chrono::NaiveDate), (i64, i64, i64)>::new();
    for event in data.analytics_events.values() {
        if !timestamp_in_analytics_bounds(event.occurred_at, bounds) {
            continue;
        }
        let counters = visit_days
            .entry((event.guest_visit_id, event.occurred_at.date_naive()))
            .or_default();
        match event.event_name.as_str() {
            "pageView" => {
                counters.0 += 1;
                consented_metrics.page_views += 1;
            }
            "rfqStarted" => {
                counters.1 += 1;
                consented_metrics.rfq_start_events += 1;
            }
            "rfqSubmitted" => {
                counters.2 += 1;
                consented_metrics.rfq_submit_events += 1;
            }
            _ => {}
        }
    }
    consented_metrics.engaged_visit_days = visit_days
        .values()
        .filter(|(page_views, starts, submissions)| {
            *page_views >= 2 || *starts > 0 || *submissions > 0
        })
        .fold(0_i64, |total, _| total + 1);
    let business_outcomes = AnalyticsBusinessOutcomes {
        rfq_submissions: data
            .rfqs
            .values()
            .filter(|submission| timestamp_in_analytics_bounds(submission.submitted_at, bounds))
            .fold(0_i64, |total, _| total + 1),
        contact_requests: data
            .contacts
            .values()
            .filter(|request| timestamp_in_analytics_bounds(request.submitted_at, bounds))
            .fold(0_i64, |total, _| total + 1),
    };
    Ok(Json(AnalyticsOverview {
        range: analytics_overview_range(bounds),
        generated_at,
        consented_metrics,
        business_outcomes,
        source: "firstParty".into(),
        contains_pii: false,
    }))
}

fn analytics_overview_bounds(
    query: AnalyticsOverviewQuery,
    now: DateTime<Utc>,
) -> Result<AnalyticsOverviewBounds, ApiError> {
    let latest_to_date = now
        .date_naive()
        .succ_opt()
        .ok_or_else(|| ApiError::internal("Unable to calculate the analytics range."))?;
    let latest_to_exclusive =
        DateTime::<Utc>::from_naive_utc_and_offset(latest_to_date.and_time(NaiveTime::MIN), Utc);
    let bounds = match (query.from.as_deref(), query.to.as_deref()) {
        (None, None) => AnalyticsOverviewBounds {
            from: latest_to_exclusive - ChronoDuration::days(ANALYTICS_OVERVIEW_DEFAULT_DAYS),
            to_exclusive: latest_to_exclusive,
        },
        (Some(from), Some(to)) => AnalyticsOverviewBounds {
            from: parse_analytics_utc_boundary("from", from)?,
            to_exclusive: parse_analytics_utc_boundary("to", to)?,
        },
        _ => {
            return Err(ApiError::bad_request(
                "from and to must either both be supplied or both be omitted.",
            ));
        }
    };
    let duration = bounds.to_exclusive - bounds.from;
    if duration <= ChronoDuration::zero() {
        return Err(ApiError::bad_request("from must be before to."));
    }
    if duration > ChronoDuration::days(ANALYTICS_OVERVIEW_MAX_DAYS) {
        return Err(ApiError::bad_request(
            "Analytics overview ranges cannot exceed 366 UTC days.",
        ));
    }
    if bounds.to_exclusive > latest_to_exclusive {
        return Err(ApiError::bad_request(
            "to cannot be later than the next UTC midnight.",
        ));
    }
    Ok(bounds)
}

fn parse_analytics_utc_boundary(field: &str, value: &str) -> Result<DateTime<Utc>, ApiError> {
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

fn timestamp_in_analytics_bounds(value: DateTime<Utc>, bounds: AnalyticsOverviewBounds) -> bool {
    value >= bounds.from && value < bounds.to_exclusive
}

fn analytics_overview_range(bounds: AnalyticsOverviewBounds) -> AnalyticsOverviewRange {
    AnalyticsOverviewRange {
        from: bounds.from,
        to_exclusive: bounds.to_exclusive,
        timezone: "UTC".into(),
    }
}
