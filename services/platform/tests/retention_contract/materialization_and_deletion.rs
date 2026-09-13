#[tokio::test]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn retention_materializes_once_before_fk_safe_raw_deletion() {
    let admin_database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let sandbox = support::MigrationSandbox::create(&admin_database_url).await;
    sandbox.apply_current().await;
    let database_url = sandbox.connection_url().to_owned();
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .expect("PostgreSQL connection");
    let unconstrained_event_error = sqlx::query(
        r#"INSERT INTO analytics_events
               (id,event_name,source_path,locale,properties,occurred_at)
           VALUES ($1,'pageView','/en/privacy-regression','en','{}'::jsonb,now())"#,
    )
    .bind(Uuid::new_v4())
    .execute(&pool)
    .await
    .expect_err("analytics events must be bound to a consented guest visit");
    assert_eq!(
        unconstrained_event_error
            .as_database_error()
            .and_then(|error| error.code())
            .as_deref(),
        Some("23502")
    );

    let setting_keys = [
        "guestVisitRetentionDays",
        "analyticsEventRetentionDays",
        "guestSourceAggregateRetentionMonths",
    ];
    let original_settings =
        sqlx::query("SELECT key,value FROM app_settings WHERE key = ANY($1) ORDER BY key")
            .bind(setting_keys.as_slice())
            .fetch_all(&pool)
            .await
            .expect("retention settings")
            .into_iter()
            .map(|row| {
                Ok::<_, sqlx::Error>((
                    row.try_get::<String, _>("key")?,
                    row.try_get::<Value, _>("value")?,
                ))
            })
            .collect::<Result<Vec<_>, _>>()
            .expect("decode retention settings");
    for (key, value) in [
        ("guestVisitRetentionDays", json!(180)),
        ("analyticsEventRetentionDays", json!(180)),
        ("guestSourceAggregateRetentionMonths", json!(24)),
    ] {
        sqlx::query(
            "UPDATE app_settings SET value=$2,updated_at=now(),updated_by='retention-test' WHERE key=$1",
        )
        .bind(key)
        .bind(value)
        .execute(&pool)
        .await
        .expect("set test retention policy");
    }

    let test_id = Uuid::new_v4();
    let landing_path = format!("/en/retention-test/{test_id}");
    let source_name = format!("{}.retention.test", test_id.simple());
    let bucket_date = (Utc::now() - Duration::days(181)).date_naive();
    let first_seen_at = DateTime::<Utc>::from_naive_utc_and_offset(
        bucket_date.and_hms_opt(12, 0, 0).expect("valid noon"),
        Utc,
    );
    let session_ids = [Uuid::new_v4(), Uuid::new_v4()];
    let consent_ids = [Uuid::new_v4(), Uuid::new_v4()];
    let visit_ids = [Uuid::new_v4(), Uuid::new_v4()];

    for index in 0..2 {
        sqlx::query(
            r#"INSERT INTO consent_records
                   (id,anonymous_session_id,policy_version,analytics_allowed,granted_at,expires_at)
               VALUES ($1,$2,'retention-test-v1',true,$3,NULL)"#,
        )
        .bind(consent_ids[index])
        .bind(session_ids[index])
        .bind(first_seen_at)
        .execute(&pool)
        .await
        .expect("consent fixture");
        sqlx::query(
            r#"INSERT INTO guest_visits
                   (id,anonymous_session_id,consent_record_id,consent_analytics_allowed,
                    locale,landing_path,source_type,referrer_host,utm_source,utm_medium,
                    utm_campaign,first_seen_at,last_seen_at,retention_until,created_at)
               VALUES ($1,$2,$3,true,'en',$4,'referral',$5,'retention-source',
                       'test-medium','test-campaign',$6,$6,$7,$6)"#,
        )
        .bind(visit_ids[index])
        .bind(session_ids[index])
        .bind(consent_ids[index])
        .bind(&landing_path)
        .bind(&source_name)
        .bind(first_seen_at)
        .bind(first_seen_at + Duration::days(180))
        .execute(&pool)
        .await
        .expect("guest visit fixture");
    }

    let event_names = [
        (0, "pageView"),
        (0, "pageView"),
        (0, "rfqStarted"),
        (0, "rfqSubmitted"),
        (1, "pageView"),
    ];
    let mut event_ids = Vec::new();
    for (offset, (visit_index, event_name)) in event_names.into_iter().enumerate() {
        let event_id = Uuid::new_v4();
        event_ids.push(event_id);
        sqlx::query(
            r#"INSERT INTO analytics_events
                   (id,event_name,source_path,locale,anonymous_session_id,properties,
                    occurred_at,guest_visit_id,consent_record_id)
               VALUES ($1,$2,$3,'en',$4,'{}'::jsonb,$5,$6,$7)"#,
        )
        .bind(event_id)
        .bind(event_name)
        .bind(&landing_path)
        .bind(session_ids[visit_index])
        .bind(first_seen_at + Duration::minutes((offset + 1) as i64))
        .bind(visit_ids[visit_index])
        .bind(consent_ids[visit_index])
        .execute(&pool)
        .await
        .expect("analytics event fixture");
    }

    let recent_at = Utc::now();
    let recent_landing_path = format!("/en/retention-live/{test_id}");
    let recent_session_id = Uuid::new_v4();
    let recent_consent_id = Uuid::new_v4();
    let recent_visit_id = Uuid::new_v4();
    let recent_event_ids = [Uuid::new_v4(), Uuid::new_v4()];
    sqlx::query(
        r#"INSERT INTO consent_records
               (id,anonymous_session_id,policy_version,analytics_allowed,granted_at,expires_at)
           VALUES ($1,$2,'retention-test-v1',true,$3,NULL)"#,
    )
    .bind(recent_consent_id)
    .bind(recent_session_id)
    .bind(recent_at)
    .execute(&pool)
    .await
    .expect("recent consent fixture");
    sqlx::query(
        r#"INSERT INTO guest_visits
               (id,anonymous_session_id,consent_record_id,consent_analytics_allowed,
                locale,landing_path,source_type,referrer_host,utm_source,utm_medium,
                utm_campaign,first_seen_at,last_seen_at,retention_until,created_at)
           VALUES ($1,$2,$3,true,'en',$4,'organicSearch',NULL,'recent-source',
                   'organic','recent-campaign',$5,$5,$6,$5)"#,
    )
    .bind(recent_visit_id)
    .bind(recent_session_id)
    .bind(recent_consent_id)
    .bind(&recent_landing_path)
    .bind(recent_at)
    .bind(recent_at + Duration::days(180))
    .execute(&pool)
    .await
    .expect("recent guest visit fixture");
    for (event_id, event_name) in recent_event_ids.iter().zip(["pageView", "rfqStarted"]) {
        sqlx::query(
            r#"INSERT INTO analytics_events
                   (id,event_name,source_path,locale,anonymous_session_id,properties,
                    occurred_at,guest_visit_id,consent_record_id)
               VALUES ($1,$2,$3,'en',$4,'{}'::jsonb,$5,$6,$7)"#,
        )
        .bind(event_id)
        .bind(event_name)
        .bind(&recent_landing_path)
        .bind(recent_session_id)
        .bind(recent_at)
        .bind(recent_visit_id)
        .bind(recent_consent_id)
        .execute(&pool)
        .await
        .expect("recent analytics event fixture");
    }

    // A prior expired contribution to the same UTC day and acquisition
    // dimension proves the retention transaction adds only the raw rows it
    // removes instead of replacing historical totals.
    sqlx::query(
        r#"INSERT INTO guest_source_daily
               (bucket_date,source_type,source_name,utm_source,utm_medium,utm_campaign,
                landing_path,locale,visits,page_views,engaged_visits,rfq_starts,
                rfq_submissions)
           VALUES ($1,'referral',$2,'retention-source','test-medium','test-campaign',
                   $3,'en',4,6,2,2,1)"#,
    )
    .bind(bucket_date)
    .bind(&source_name)
    .bind(&landing_path)
    .execute(&pool)
    .await
    .expect("historical aggregate fixture");
    let stale_path = format!("{landing_path}/stale");
    sqlx::query(
        r#"INSERT INTO guest_source_daily
               (bucket_date,source_type,source_name,utm_source,utm_medium,utm_campaign,
                landing_path,locale,visits)
           VALUES (((now() AT TIME ZONE 'UTC') - interval '25 months')::date,
                   'direct','','','','',$1,'en',1)"#,
    )
    .bind(&stale_path)
    .execute(&pool)
    .await
    .expect("expired daily aggregate fixture");

    let (before_visit, before_source) = analytics_rows(&database_url, &landing_path).await;
    assert_eq!(before_visit.visits, 6);
    assert_eq!(before_visit.page_views, 9);
    assert_eq!(before_visit.rfq_starts, 3);
    assert_eq!(before_visit.rfq_submissions, 2);
    assert_eq!(before_source.visits, 6);
    assert_eq!(before_source.page_views, 9);
    assert_eq!(before_source.rfq_starts, 3);
    assert_eq!(before_source.rfq_submissions, 2);
    assert_eq!(
        before_source.referrer_domain.as_deref(),
        Some(source_name.as_str())
    );
    assert_eq!(
        before_source.utm_source.as_deref(),
        Some("retention-source")
    );
    let overview_from =
        DateTime::<Utc>::from_naive_utc_and_offset(bucket_date.and_hms_opt(0, 0, 0).unwrap(), Utc);
    let overview_before = analytics_overview(
        &database_url,
        overview_from,
        overview_from + Duration::days(1),
    )
    .await;
    assert_eq!(overview_before.consented_metrics.visits, 6);
    assert_eq!(overview_before.consented_metrics.page_views, 9);
    assert_eq!(overview_before.consented_metrics.engaged_visit_days, 3);
    assert_eq!(overview_before.consented_metrics.rfq_start_events, 3);
    assert_eq!(overview_before.consented_metrics.rfq_submit_events, 2);

    let (recent_before_visit, recent_before_source) =
        analytics_rows(&database_url, &recent_landing_path).await;
    assert_eq!(recent_before_visit.visits, 1);
    assert_eq!(recent_before_visit.page_views, 1);
    assert_eq!(recent_before_visit.rfq_starts, 1);
    assert_eq!(recent_before_visit.rfq_submissions, 0);
    assert_eq!(recent_before_source.visits, 1);
    assert_eq!(recent_before_source.page_views, 1);
    assert_eq!(recent_before_source.rfq_starts, 1);
    assert_eq!(recent_before_source.rfq_submissions, 0);
    assert_eq!(
        recent_before_source.source_name.as_deref(),
        Some("recent-source")
    );

    let first_result = apply_retention(&pool).await.expect("first retention run");
    assert_eq!(first_result["analyticsEventsDeleted"], json!(5));
    assert_eq!(first_result["guestVisitsDeleted"], json!(2));

    let aggregate = sqlx::query(
        r#"SELECT visits,page_views,engaged_visits,rfq_starts,rfq_submissions
           FROM guest_source_daily
           WHERE bucket_date=$1 AND source_type='referral' AND source_name=$2
             AND utm_source='retention-source' AND utm_medium='test-medium'
             AND utm_campaign='test-campaign' AND landing_path=$3 AND locale='en'"#,
    )
    .bind(bucket_date)
    .bind(&source_name)
    .bind(&landing_path)
    .fetch_one(&pool)
    .await
    .expect("materialized aggregate");
    assert_eq!(aggregate.try_get::<i64, _>("visits").unwrap(), 6);
    assert_eq!(aggregate.try_get::<i64, _>("page_views").unwrap(), 9);
    // Visit zero has two page views and RFQ events; visit one has only one page
    // view. Each visit can contribute at most one engaged count.
    assert_eq!(aggregate.try_get::<i64, _>("engaged_visits").unwrap(), 3);
    assert_eq!(aggregate.try_get::<i64, _>("rfq_starts").unwrap(), 3);
    assert_eq!(aggregate.try_get::<i64, _>("rfq_submissions").unwrap(), 2);

    let (after_visit, after_source) = analytics_rows(&database_url, &landing_path).await;
    assert_eq!(
        serde_json::to_value(&after_visit).unwrap(),
        serde_json::to_value(&before_visit).unwrap(),
        "moving expired raw rows into the daily aggregate must not change the report"
    );
    assert_eq!(
        serde_json::to_value(&after_source).unwrap(),
        serde_json::to_value(&before_source).unwrap(),
        "source attribution must remain stable across retention"
    );
    let overview_after = analytics_overview(
        &database_url,
        overview_from,
        overview_from + Duration::days(1),
    )
    .await;
    assert_eq!(
        serde_json::to_value(&overview_after.consented_metrics).unwrap(),
        serde_json::to_value(&overview_before.consented_metrics).unwrap(),
        "retention must not change the consented overview metrics"
    );
    let (recent_after_visit, recent_after_source) =
        analytics_rows(&database_url, &recent_landing_path).await;
    assert_eq!(
        serde_json::to_value(&recent_after_visit).unwrap(),
        serde_json::to_value(&recent_before_visit).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&recent_after_source).unwrap(),
        serde_json::to_value(&recent_before_source).unwrap()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM guest_source_daily WHERE landing_path=$1",
        )
        .bind(&recent_landing_path)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0,
        "recent raw analytics stay live and are not prematurely materialized"
    );
    let dimension_integrity = sqlx::query_as::<_, (i32, bool)>(
        r#"SELECT octet_length(dimension_hash),
                  dimension_hash=guest_source_dimension_hash(
                      source_type,source_name,utm_source,utm_medium,utm_campaign,
                      landing_path,locale
                  )
           FROM guest_source_daily
           WHERE bucket_date=$1 AND source_name=$2 AND landing_path=$3"#,
    )
    .bind(bucket_date)
    .bind(&source_name)
    .bind(&landing_path)
    .fetch_one(&pool)
    .await
    .expect("fixed-length aggregate identity");
    assert_eq!(dimension_integrity, (32, true));
    let primary_key = sqlx::query_scalar::<_, String>(
        r#"SELECT pg_get_constraintdef(oid)
           FROM pg_constraint
           WHERE conrelid='guest_source_daily'::regclass
             AND conname='guest_source_daily_pkey'"#,
    )
    .fetch_one(&pool)
    .await
    .expect("aggregate primary key");
    assert_eq!(primary_key, "PRIMARY KEY (bucket_date, dimension_hash)");
    let tampered_hash_error = sqlx::query(
        r#"UPDATE guest_source_daily SET source_name=source_name || '.tampered'
           WHERE bucket_date=$1 AND source_name=$2 AND landing_path=$3"#,
    )
    .bind(bucket_date)
    .bind(&source_name)
    .bind(&landing_path)
    .execute(&pool)
    .await
    .expect_err("a dimension tuple cannot retain a mismatched digest");
    assert_eq!(
        tampered_hash_error
            .as_database_error()
            .and_then(|error| error.code())
            .as_deref(),
        Some("23514")
    );

    let raw_visits =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM guest_visits WHERE id = ANY($1)")
            .bind(visit_ids.as_slice())
            .fetch_one(&pool)
            .await
            .expect("raw visit count");
    let raw_events =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM analytics_events WHERE id = ANY($1)")
            .bind(event_ids.as_slice())
            .fetch_one(&pool)
            .await
            .expect("raw event count");
    let raw_consents =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM consent_records WHERE id = ANY($1)")
            .bind(consent_ids.as_slice())
            .fetch_one(&pool)
            .await
            .expect("raw consent count");
    assert_eq!(raw_visits, 0);
    assert_eq!(raw_events, 0);
    assert_eq!(raw_consents, 0);
    assert_eq!(first_result["consentRecordsDeleted"], json!(2));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM guest_source_daily WHERE landing_path=$1",
        )
        .bind(&stale_path)
        .fetch_one(&pool)
        .await
        .expect("expired daily count"),
        0
    );

    apply_retention(&pool)
        .await
        .expect("idempotent second retention run");
    let counters = sqlx::query_as::<_, (i64, i64, i64, i64, i64)>(
        r#"SELECT visits,page_views,engaged_visits,rfq_starts,rfq_submissions
           FROM guest_source_daily WHERE bucket_date=$1 AND source_name=$2
             AND landing_path=$3"#,
    )
    .bind(bucket_date)
    .bind(&source_name)
    .bind(&landing_path)
    .fetch_one(&pool)
    .await
    .expect("aggregate after retry");
    assert_eq!(counters, (6, 9, 3, 3, 2));
    let (after_retry_visit, after_retry_source) =
        analytics_rows(&database_url, &landing_path).await;
    assert_eq!(
        serde_json::to_value(after_retry_visit).unwrap(),
        serde_json::to_value(before_visit).unwrap()
    );
    assert_eq!(
        serde_json::to_value(after_retry_source).unwrap(),
        serde_json::to_value(before_source).unwrap()
    );

    sqlx::query("DELETE FROM guest_source_daily WHERE landing_path=$1")
        .bind(&landing_path)
        .execute(&pool)
        .await
        .expect("aggregate cleanup");
    sqlx::query("DELETE FROM consent_records WHERE id = ANY($1)")
        .bind(consent_ids.as_slice())
        .execute(&pool)
        .await
        .expect("consent cleanup");
    sqlx::query("DELETE FROM analytics_events WHERE id = ANY($1)")
        .bind(recent_event_ids.as_slice())
        .execute(&pool)
        .await
        .expect("recent event cleanup");
    sqlx::query("DELETE FROM guest_visits WHERE id=$1")
        .bind(recent_visit_id)
        .execute(&pool)
        .await
        .expect("recent visit cleanup");
    sqlx::query("DELETE FROM consent_records WHERE id=$1")
        .bind(recent_consent_id)
        .execute(&pool)
        .await
        .expect("recent consent cleanup");
    for (key, value) in original_settings {
        sqlx::query(
            "UPDATE app_settings SET value=$2,updated_at=now(),updated_by='retention-test-restore' WHERE key=$1",
        )
        .bind(key)
        .bind(value)
        .execute(&pool)
        .await
        .expect("restore retention setting");
    }
    pool.close().await;
    sandbox.cleanup().await;
}
