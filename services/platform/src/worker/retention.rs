/// Materialize consented acquisition/funnel metrics immediately before removing
/// the contributing raw records. Each raw visit/event is moved into the daily
/// aggregate exactly once in the same transaction. Admin reporting can
/// therefore add historical daily rows to still-live raw rows without a gap or
/// overlap, while transaction rollback/retry cannot double-count a record.
///
/// `engaged_visits` has a deliberately narrow, auditable definition: one visit
/// counts at most once per UTC day and acquisition dimension when it has at
/// least two `pageView` events or any `rfqStarted`/`rfqSubmitted` event. A lone
/// page view is not treated as engagement.
pub async fn apply_retention(pool: &PgPool) -> Result<Value, String> {
    apply_retention_with_deployment_defaults(pool, 180, 24).await
}

async fn apply_retention_with_deployment_defaults(
    pool: &PgPool,
    guest_raw_retention_days: i64,
    guest_aggregate_retention_months: i64,
) -> Result<Value, String> {
    let mut transaction = pool.begin().await.map_err(|error| error.to_string())?;

    // Multiple worker instances may claim different retention jobs. Serialize
    // the materialize/delete transaction without requiring a process-local
    // lock or a privileged database operation.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('airtek.retention.apply', 0))")
        .execute(&mut *transaction)
        .await
        .map_err(|error| error.to_string())?;

    let grace_days = deployment_aware_integer_setting(
        &mut transaction,
        "retentionDeletionGraceDays",
        30,
        1,
        365,
    )
    .await?;
    let guest_visit_retention_days = deployment_aware_integer_setting(
        &mut transaction,
        "guestVisitRetentionDays",
        guest_raw_retention_days,
        1,
        3_650,
    )
    .await?;
    let analytics_event_retention_days = deployment_aware_integer_setting(
        &mut transaction,
        "analyticsEventRetentionDays",
        guest_raw_retention_days,
        1,
        3_650,
    )
    .await?;
    let aggregate_retention_months = deployment_aware_integer_setting(
        &mut transaction,
        "guestSourceAggregateRetentionMonths",
        guest_aggregate_retention_months,
        1,
        120,
    )
    .await?;

    // Prevent a new event from attaching to a visit after that visit's events
    // have been materialized but before the visit is deleted. The foreign-key
    // key-share lock taken by an event insert conflicts with this row lock.
    sqlx::query(
        r#"SELECT id FROM guest_visits
           WHERE first_seen_at < now() - make_interval(days => $1)
           FOR UPDATE"#,
    )
    .bind(guest_visit_retention_days as i32)
    .fetch_all(&mut *transaction)
    .await
    .map_err(|error| error.to_string())?;

    // Visit counts and event counters are materialized separately, and only
    // for rows that the deletion statements below remove in this transaction.
    // Additive upserts preserve earlier expired contributions to the same UTC
    // day/dimension; deletion makes a committed retry a no-op.
    let visit_aggregate_rows = sqlx::query(
        r#"INSERT INTO guest_source_daily
               (bucket_date, source_type, source_name, utm_source, utm_medium,
                utm_campaign, landing_path, locale, visits, page_views,
                engaged_visits, rfq_starts, rfq_submissions, updated_at)
           SELECT (first_seen_at AT TIME ZONE 'UTC')::date,
                  source_type,
                  COALESCE(referrer_host, ''),
                  COALESCE(utm_source, ''),
                  COALESCE(utm_medium, ''),
                  COALESCE(utm_campaign, ''),
                  landing_path,
                  locale,
                  COUNT(*)::bigint,
                  0::bigint,
                  0::bigint,
                  0::bigint,
                  0::bigint,
                  now()
           FROM guest_visits
           WHERE consent_analytics_allowed = true
             AND first_seen_at < now() - make_interval(days => $1)
           GROUP BY (first_seen_at AT TIME ZONE 'UTC')::date,
                    source_type, COALESCE(referrer_host, ''),
                    COALESCE(utm_source, ''), COALESCE(utm_medium, ''),
                    COALESCE(utm_campaign, ''), landing_path, locale
           ON CONFLICT (bucket_date, dimension_hash)
           DO UPDATE SET source_type = EXCLUDED.source_type,
                         source_name = EXCLUDED.source_name,
                         utm_source = EXCLUDED.utm_source,
                         utm_medium = EXCLUDED.utm_medium,
                         utm_campaign = EXCLUDED.utm_campaign,
                         landing_path = EXCLUDED.landing_path,
                         locale = EXCLUDED.locale,
                         visits = guest_source_daily.visits + EXCLUDED.visits,
                         updated_at = now()"#,
    )
    .bind(guest_visit_retention_days as i32)
    .execute(&mut *transaction)
    .await
    .map_err(|error| error.to_string())?
    .rows_affected();

    // Freeze the exact raw event set that this transaction will materialize
    // and remove. PostgreSQL temporary tables are session-local, and ON COMMIT
    // DROP keeps pooled connections clean after either commit or rollback.
    // Deleting by these IDs prevents a concurrently inserted row from being
    // deleted without first contributing to the aggregate.
    sqlx::query(
        r#"CREATE TEMPORARY TABLE retention_event_batch (
               event_id uuid PRIMARY KEY,
               guest_visit_id uuid NOT NULL,
               bucket_date date NOT NULL,
               source_type text NOT NULL,
               source_name text NOT NULL,
               utm_source text NOT NULL,
               utm_medium text NOT NULL,
               utm_campaign text NOT NULL,
               landing_path text NOT NULL,
               locale text NOT NULL,
               event_name text NOT NULL
           ) ON COMMIT DROP"#,
    )
    .execute(&mut *transaction)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query(
        r#"INSERT INTO retention_event_batch
               (event_id,guest_visit_id,bucket_date,source_type,source_name,
                utm_source,utm_medium,utm_campaign,landing_path,locale,event_name)
           SELECT event.id,event.guest_visit_id,
                  (event.occurred_at AT TIME ZONE 'UTC')::date,
                  visit.source_type,COALESCE(visit.referrer_host,''),
                  COALESCE(visit.utm_source,''),COALESCE(visit.utm_medium,''),
                  COALESCE(visit.utm_campaign,''),visit.landing_path,visit.locale,
                  event.event_name
           FROM analytics_events AS event
           INNER JOIN guest_visits AS visit
               ON visit.id=event.guest_visit_id
              AND visit.anonymous_session_id=event.anonymous_session_id
              AND visit.consent_record_id=event.consent_record_id
           WHERE visit.consent_analytics_allowed=true
             AND (
                   event.occurred_at < now() - make_interval(days => $1)
                   OR visit.first_seen_at < now() - make_interval(days => $2)
             )
           FOR UPDATE OF event"#,
    )
    .bind(analytics_event_retention_days as i32)
    .bind(guest_visit_retention_days as i32)
    .execute(&mut *transaction)
    .await
    .map_err(|error| error.to_string())?;

    let event_aggregate_rows = sqlx::query(
        r#"WITH event_batches AS MATERIALIZED (
               SELECT batch.guest_visit_id,batch.bucket_date,batch.source_type,
                      batch.source_name,batch.utm_source,batch.utm_medium,
                      batch.utm_campaign,batch.landing_path,batch.locale,
                      COUNT(*) FILTER (WHERE batch.event_name='pageView')::bigint
                          AS page_views,
                      COUNT(*) FILTER (WHERE batch.event_name='rfqStarted')::bigint
                          AS rfq_starts,
                      COUNT(*) FILTER (WHERE batch.event_name='rfqSubmitted')::bigint
                          AS rfq_submissions
               FROM retention_event_batch AS batch
               WHERE batch.event_name IN ('pageView','rfqStarted','rfqSubmitted')
               GROUP BY batch.guest_visit_id,batch.bucket_date,batch.source_type,
                        batch.source_name,batch.utm_source,batch.utm_medium,
                        batch.utm_campaign,batch.landing_path,batch.locale
           ), ledger_state AS MATERIALIZED (
               SELECT batch.*,
                      COALESCE(ledger.page_views,0) + batch.page_views
                          AS total_page_views,
                      COALESCE(ledger.rfq_starts,0) + batch.rfq_starts
                          AS total_rfq_starts,
                      COALESCE(ledger.rfq_submissions,0) + batch.rfq_submissions
                          AS total_rfq_submissions,
                      (
                          NOT COALESCE(ledger.engaged_materialized,false)
                          AND (
                              COALESCE(ledger.page_views,0) + batch.page_views >= 2
                              OR COALESCE(ledger.rfq_starts,0) + batch.rfq_starts > 0
                              OR COALESCE(ledger.rfq_submissions,0)
                                   + batch.rfq_submissions > 0
                          )
                      )::integer AS engaged_delta
               FROM event_batches AS batch
               LEFT JOIN guest_visit_daily_event_materializations AS ledger
                 ON ledger.guest_visit_id=batch.guest_visit_id
                AND ledger.bucket_date=batch.bucket_date
           ), ledger_upsert AS (
               INSERT INTO guest_visit_daily_event_materializations
                   (guest_visit_id,bucket_date,page_views,rfq_starts,
                    rfq_submissions,engaged_materialized,updated_at)
               SELECT guest_visit_id,bucket_date,total_page_views,total_rfq_starts,
                      total_rfq_submissions,engaged_delta=1,now()
               FROM ledger_state
               ON CONFLICT (guest_visit_id,bucket_date)
               DO UPDATE SET
                   page_views=EXCLUDED.page_views,
                   rfq_starts=EXCLUDED.rfq_starts,
                   rfq_submissions=EXCLUDED.rfq_submissions,
                   engaged_materialized=(
                       guest_visit_daily_event_materializations.engaged_materialized
                       OR EXCLUDED.engaged_materialized
                   ),
                   updated_at=now()
               RETURNING guest_visit_id,bucket_date
           ), event_rollup AS (
               SELECT state.bucket_date,state.source_type,state.source_name,
                      state.utm_source,state.utm_medium,state.utm_campaign,
                      state.landing_path,state.locale,
                      SUM(state.page_views)::bigint AS page_views,
                      SUM(state.engaged_delta)::bigint AS engaged_visits,
                      SUM(state.rfq_starts)::bigint AS rfq_starts,
                      SUM(state.rfq_submissions)::bigint AS rfq_submissions
               FROM ledger_state AS state
               WHERE EXISTS (
                   SELECT 1 FROM ledger_upsert AS upserted
                   WHERE upserted.guest_visit_id=state.guest_visit_id
                     AND upserted.bucket_date=state.bucket_date
               )
               GROUP BY state.bucket_date,state.source_type,state.source_name,
                        state.utm_source,state.utm_medium,state.utm_campaign,
                        state.landing_path,state.locale
           )
           INSERT INTO guest_source_daily
               (bucket_date, source_type, source_name, utm_source, utm_medium,
                utm_campaign, landing_path, locale, visits, page_views,
                engaged_visits, rfq_starts, rfq_submissions, updated_at)
           SELECT bucket_date, source_type, source_name, utm_source, utm_medium,
                  utm_campaign, landing_path, locale, 0::bigint, page_views,
                  engaged_visits, rfq_starts, rfq_submissions, now()
           FROM event_rollup
           ON CONFLICT (bucket_date, dimension_hash)
           DO UPDATE SET source_type = EXCLUDED.source_type,
                         source_name = EXCLUDED.source_name,
                         utm_source = EXCLUDED.utm_source,
                         utm_medium = EXCLUDED.utm_medium,
                         utm_campaign = EXCLUDED.utm_campaign,
                         landing_path = EXCLUDED.landing_path,
                         locale = EXCLUDED.locale,
                         page_views = guest_source_daily.page_views + EXCLUDED.page_views,
                         engaged_visits = guest_source_daily.engaged_visits + EXCLUDED.engaged_visits,
                         rfq_starts = guest_source_daily.rfq_starts + EXCLUDED.rfq_starts,
                         rfq_submissions = guest_source_daily.rfq_submissions + EXCLUDED.rfq_submissions,
                         updated_at = now()"#,
    )
    .execute(&mut *transaction)
    .await
    .map_err(|error| error.to_string())?
    .rows_affected();

    let queued_rfqs = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM rfq_submissions WHERE retention_until < now() AND retention_until >= now() - make_interval(days => $1)",
    )
    .bind(grace_days as i32)
    .fetch_one(&mut *transaction)
    .await
    .map_err(|error| error.to_string())?;
    let queued_contacts = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM contact_requests WHERE retention_until < now() AND retention_until >= now() - make_interval(days => $1)",
    )
    .bind(grace_days as i32)
    .fetch_one(&mut *transaction)
    .await
    .map_err(|error| error.to_string())?;
    let rfqs = sqlx::query(
        r#"UPDATE rfq_submissions
           SET status='piiCleared',
               payload=jsonb_set(
                   jsonb_set(
                       jsonb_set(payload, '{request,contact}',
                           '{"name":"[retention-cleared]","email":"[retention-cleared]","phone":null,"company":null,"countryOrRegion":null}'::jsonb, true),
                       '{request,context}', '{}'::jsonb, true),
                   '{status}', '"piiCleared"'::jsonb, true)
           WHERE retention_until < now() - make_interval(days => $1)
             AND status <> 'piiCleared'"#,
    )
    .bind(grace_days as i32)
    .execute(&mut *transaction)
    .await
    .map_err(|error| error.to_string())?
    .rows_affected();
    let contacts = sqlx::query(
        r#"UPDATE contact_requests
           SET status='piiCleared',
               payload=jsonb_set(
                   jsonb_set(
                       jsonb_set(payload, '{request,contact}',
                           '{"name":"[retention-cleared]","email":"[retention-cleared]","phone":null,"company":null,"countryOrRegion":null}'::jsonb, true),
                       '{request,message}', '"[retention-cleared]"'::jsonb, true),
                   '{status}', '"piiCleared"'::jsonb, true)
           WHERE retention_until < now() - make_interval(days => $1)
             AND status <> 'piiCleared'"#,
    )
    .bind(grace_days as i32)
    .execute(&mut *transaction)
    .await
    .map_err(|error| error.to_string())?
    .rows_affected();

    // Remove old events first. Events attached to an expired visit are also
    // removed even if their own timestamp is newer, so the visit FK cannot
    // force acquisition identifiers beyond their configured maximum lifetime.
    let analytics_events_deleted = sqlx::query(
        r#"DELETE FROM analytics_events AS event
           USING retention_event_batch AS batch
           WHERE event.id=batch.event_id"#,
    )
    .execute(&mut *transaction)
    .await
    .map_err(|error| error.to_string())?
    .rows_affected();
    let event_materialization_rows_deleted = sqlx::query(
        r#"DELETE FROM guest_visit_daily_event_materializations AS ledger
           WHERE EXISTS (
                     SELECT 1 FROM retention_event_batch AS batch
                     WHERE batch.guest_visit_id=ledger.guest_visit_id
                       AND batch.bucket_date=ledger.bucket_date
                 )
             AND NOT EXISTS (
                     SELECT 1 FROM analytics_events AS event
                     WHERE event.guest_visit_id=ledger.guest_visit_id
                       AND (event.occurred_at AT TIME ZONE 'UTC')::date=ledger.bucket_date
                 )"#,
    )
    .execute(&mut *transaction)
    .await
    .map_err(|error| error.to_string())?
    .rows_affected();
    let guest_visits_deleted = sqlx::query(
        r#"DELETE FROM guest_visits
           WHERE first_seen_at < now() - make_interval(days => $1)"#,
    )
    .bind(guest_visit_retention_days as i32)
    .execute(&mut *transaction)
    .await
    .map_err(|error| error.to_string())?
    .rows_affected();
    let consent_retention_days = guest_visit_retention_days.max(analytics_event_retention_days);
    let consent_records_deleted = sqlx::query(
        r#"DELETE FROM consent_records AS consent
           WHERE (consent.granted_at < now() - make_interval(days => $1)
                  OR consent.expires_at < now())
             AND NOT EXISTS (
                   SELECT 1 FROM analytics_events AS event
                   WHERE event.consent_record_id=consent.id
             )
             AND NOT EXISTS (
                   SELECT 1 FROM guest_visits AS visit
                   WHERE visit.consent_record_id=consent.id
             )"#,
    )
    .bind(consent_retention_days as i32)
    .execute(&mut *transaction)
    .await
    .map_err(|error| error.to_string())?
    .rows_affected();
    let source_daily_deleted = sqlx::query(
        r#"DELETE FROM guest_source_daily
           WHERE bucket_date < (
               (now() AT TIME ZONE 'UTC') - make_interval(months => $1)
           )::date"#,
    )
    .bind(aggregate_retention_months as i32)
    .execute(&mut *transaction)
    .await
    .map_err(|error| error.to_string())?
    .rows_affected();
    // Preserve the normalized import report and missing-asset workflow while
    // cryptographically erasing expired confidential source material. The
    // Admin pricing endpoint also rejects `expired` rows, so old quote bands
    // cannot be recovered with the still-configured deployment key.
    let product_private_staging_expired = sqlx::query(
        r#"UPDATE product_import_private_staging
           SET status='expired',
               ciphertext=decode(repeat('00',octet_length(ciphertext)),'hex'),
               nonce=decode(repeat('00',octet_length(nonce)),'hex'),
               authentication_tag=decode(repeat('00',octet_length(authentication_tag)),'hex'),
               processed_at=COALESCE(processed_at,now())
           WHERE expires_at < now() AND status IN ('received','validated','promoted','rejected')"#,
    )
    .execute(&mut *transaction)
    .await
    .map_err(|error| error.to_string())?
    .rows_affected();

    transaction
        .commit()
        .await
        .map_err(|error| error.to_string())?;
    Ok(json!({
        "gracePeriodDays": grace_days,
        "guestVisitRetentionDays": guest_visit_retention_days,
        "analyticsEventRetentionDays": analytics_event_retention_days,
        "guestSourceAggregateRetentionMonths": aggregate_retention_months,
        "guestVisitAggregateRowsUpserted": visit_aggregate_rows,
        "analyticsEventAggregateRowsUpserted": event_aggregate_rows,
        "analyticsEventsDeleted": analytics_events_deleted,
        "guestEventMaterializationRowsDeleted": event_materialization_rows_deleted,
        "guestVisitsDeleted": guest_visits_deleted,
        "consentRecordsDeleted": consent_records_deleted,
        "guestSourceDailyRowsDeleted": source_daily_deleted,
        "productPrivateStagingRowsExpired": product_private_staging_expired,
        "rfqsQueuedForPiiClearing": queued_rfqs,
        "contactsQueuedForPiiClearing": queued_contacts,
        "rfqsPiiCleared": rfqs,
        "contactsPiiCleared": contacts
    }))
}

async fn deployment_aware_integer_setting(
    transaction: &mut Transaction<'_, Postgres>,
    key: &str,
    deployment_default: i64,
    minimum: i64,
    maximum: i64,
) -> Result<i64, String> {
    if !(minimum..=maximum).contains(&deployment_default) {
        return Err(format!(
            "deployment default for {key} must be between {minimum} and {maximum}"
        ));
    }
    let setting = sqlx::query("SELECT value,updated_by FROM app_settings WHERE key=$1")
        .bind(key)
        .fetch_optional(&mut **transaction)
        .await
        .map_err(|error| error.to_string())?;
    let setting = setting
        .map(|setting| {
            Ok::<_, sqlx::Error>((
                setting.try_get::<Value, _>("value")?,
                setting.try_get::<String, _>("updated_by")?,
            ))
        })
        .transpose()
        .map_err(|error| error.to_string())?;
    resolve_integer_setting(key, setting, deployment_default, minimum, maximum)
}

fn resolve_integer_setting(
    key: &str,
    setting: Option<(Value, String)>,
    deployment_default: i64,
    minimum: i64,
    maximum: i64,
) -> Result<i64, String> {
    let value = match setting {
        // Migration-owned values are schema defaults, not an administrator's
        // explicit policy. Deployment environment values therefore remain
        // effective until the setting is changed through the Admin API.
        Some((value, updated_by)) if updated_by != "migration" => value,
        _ => Value::from(deployment_default),
    };
    value
        .as_i64()
        .filter(|value| (minimum..=maximum).contains(value))
        .ok_or_else(|| format!("{key} must be an integer between {minimum} and {maximum}"))
}
