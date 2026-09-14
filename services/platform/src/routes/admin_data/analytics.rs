async fn list_guest_sources(
    State(state): State<AppState>,
    Query(query): Query<AnalyticsQuery>,
) -> Result<Json<CursorPage<GuestSourceDaily>>, ApiError> {
    validate_time_range(query.from, query.to)?;
    let scope = analytics_cursor_scope("admin.analytics.sources", query.from, query.to);
    let limit = cursor_limit(&query.pagination())?;
    let after = decode_analytics_cursor(&scope, query.cursor.as_deref())?;
    let values = if let Some(pool) = &state.pool {
        // Keep the acquisition dimensions byte-for-byte aligned with the
        // Worker aggregate so historical and live contributions collapse into
        // one daily source row.
        let rows = sqlx::query(
            r#"WITH metric_rows AS (
                   SELECT bucket_date,source_type,source_name,utm_source,utm_medium,
                          utm_campaign,landing_path,locale,visits,page_views,
                          rfq_starts,rfq_submissions
                   FROM guest_source_daily
                   UNION ALL
                   SELECT (first_seen_at AT TIME ZONE 'UTC')::date AS bucket_date,
                          source_type,COALESCE(referrer_host,'') AS source_name,
                          COALESCE(utm_source,'') AS utm_source,
                          COALESCE(utm_medium,'') AS utm_medium,
                          COALESCE(utm_campaign,'') AS utm_campaign,landing_path,locale,
                          COUNT(*)::bigint AS visits,0::bigint AS page_views,
                          0::bigint AS rfq_starts,0::bigint AS rfq_submissions
                   FROM guest_visits
                   WHERE consent_analytics_allowed=true
                   GROUP BY (first_seen_at AT TIME ZONE 'UTC')::date,source_type,
                            COALESCE(referrer_host,''),COALESCE(utm_source,''),
                            COALESCE(utm_medium,''),COALESCE(utm_campaign,''),
                            landing_path,locale
                   UNION ALL
                   SELECT (event.occurred_at AT TIME ZONE 'UTC')::date AS bucket_date,
                          visit.source_type,COALESCE(visit.referrer_host,'') AS source_name,
                          COALESCE(visit.utm_source,'') AS utm_source,
                          COALESCE(visit.utm_medium,'') AS utm_medium,
                          COALESCE(visit.utm_campaign,'') AS utm_campaign,
                          visit.landing_path,visit.locale,0::bigint AS visits,
                          COUNT(*) FILTER (WHERE event.event_name='pageView')::bigint
                              AS page_views,
                          COUNT(*) FILTER (WHERE event.event_name='rfqStarted')::bigint
                              AS rfq_starts,
                          COUNT(*) FILTER (WHERE event.event_name='rfqSubmitted')::bigint
                              AS rfq_submissions
                   FROM analytics_events AS event
                   INNER JOIN guest_visits AS visit
                       ON visit.id=event.guest_visit_id
                      AND visit.anonymous_session_id=event.anonymous_session_id
                      AND visit.consent_record_id=event.consent_record_id
                   WHERE visit.consent_analytics_allowed=true
                     AND event.event_name IN ('pageView','rfqStarted','rfqSubmitted')
                   GROUP BY (event.occurred_at AT TIME ZONE 'UTC')::date,
                            visit.source_type,COALESCE(visit.referrer_host,''),
                            COALESCE(visit.utm_source,''),COALESCE(visit.utm_medium,''),
                            COALESCE(visit.utm_campaign,''),visit.landing_path,visit.locale
               ), aggregated AS (
                   SELECT bucket_date,source_type,source_name,utm_source,utm_medium,
                          utm_campaign,landing_path,locale,SUM(visits)::bigint AS visits,
                          SUM(page_views)::bigint AS page_views,
                          SUM(rfq_starts)::bigint AS rfq_starts,
                          SUM(rfq_submissions)::bigint AS rfq_submissions
                   FROM metric_rows
                   WHERE ($1::timestamptz IS NULL OR bucket_date >= ($1 AT TIME ZONE 'UTC')::date)
                     AND ($2::timestamptz IS NULL OR bucket_date < ($2 AT TIME ZONE 'UTC')::date)
                   GROUP BY bucket_date,source_type,source_name,utm_source,utm_medium,
                            utm_campaign,landing_path,locale
               ), hashed AS (
                   SELECT aggregated.*,
                          guest_source_dimension_hash(
                              source_type,source_name,utm_source,utm_medium,utm_campaign,
                              landing_path,locale
                          ) AS dimension_hash
                   FROM aggregated
               ), guarded AS (
                   SELECT hashed.*,
                          COUNT(*) OVER (PARTITION BY bucket_date,dimension_hash)::bigint
                              AS hash_count
                   FROM hashed
               )
               SELECT bucket_date,source_type,
                      COALESCE(NULLIF(utm_source,''),NULLIF(source_name,''),source_type) AS source_name,
                      NULLIF(source_name,'') AS referrer_domain,
                      NULLIF(utm_source,'') AS utm_source,NULLIF(utm_medium,'') AS utm_medium,
                      NULLIF(utm_campaign,'') AS utm_campaign,landing_path,locale,
                      visits,page_views,rfq_starts,rfq_submissions,dimension_hash,hash_count
               FROM guarded
               WHERE ($3::date IS NULL OR bucket_date < $3
                      OR (bucket_date=$3 AND dimension_hash > $4::bytea))
               ORDER BY bucket_date DESC,dimension_hash
               LIMIT $5"#,
        )
        .bind(query.from)
        .bind(query.to)
        .bind(after.as_ref().map(|cursor| cursor.bucket_date))
        .bind(
            after
                .as_ref()
                .map(|cursor| cursor.dimension_hash.as_slice()),
        )
        .bind((limit + 1) as i64)
        .fetch_all(pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                ensure_no_analytics_hash_collision(row.try_get("hash_count")?)?;
                let cursor = analytics_cursor_from_database_row(&row)?;
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
            .collect::<Result<Vec<_>, ApiError>>()?
    } else {
        aggregate_memory_sources(&state, query.from, query.to)
            .await
            .into_iter()
            .map(|source| {
                let cursor = AnalyticsCursor {
                    bucket_date: source.bucket_date,
                    dimension_hash: analytics_dimension_hash(&[
                        &source.source,
                        source.referrer_domain.as_deref().unwrap_or(""),
                        source.utm_source.as_deref().unwrap_or(""),
                        source.medium.as_deref().unwrap_or(""),
                        source.campaign.as_deref().unwrap_or(""),
                        &source.landing_path,
                        &source.locale,
                    ]),
                };
                (source, cursor)
            })
            .collect()
    };
    let page = if state.pool.is_some() {
        finish_analytics_page(&scope, values, limit)?
    } else {
        paginate_memory_analytics(&scope, values, after.as_ref(), limit)?
    };
    Ok(Json(page))
}
