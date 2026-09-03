#[tokio::test]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn engagement_crossing_event_cutoffs_is_materialized_once() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .expect("PostgreSQL connection");
    support::assert_flyway_schema_current(&pool).await;

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
        ("guestVisitRetentionDays", json!(30)),
        ("analyticsEventRetentionDays", json!(3)),
        ("guestSourceAggregateRetentionMonths", json!(24)),
    ] {
        sqlx::query(
            "UPDATE app_settings SET value=$2,updated_at=now(),updated_by='retention-split-test' WHERE key=$1",
        )
        .bind(key)
        .bind(value)
        .execute(&pool)
        .await
        .expect("set split-cutoff retention policy");
    }

    let test_id = Uuid::new_v4();
    let landing_path = format!("/en/retention-split/{test_id}");
    let source_name = format!("{}.split.retention.test", test_id.simple());
    let bucket_date = (Utc::now() - Duration::days(3)).date_naive();
    let first_event_at = DateTime::<Utc>::from_naive_utc_and_offset(
        bucket_date.and_hms_micro_opt(0, 0, 0, 0).unwrap(),
        Utc,
    );
    let second_event_at = DateTime::<Utc>::from_naive_utc_and_offset(
        bucket_date.and_hms_micro_opt(23, 59, 59, 999_999).unwrap(),
        Utc,
    );
    let session_id = Uuid::new_v4();
    let consent_id = Uuid::new_v4();
    let visit_id = Uuid::new_v4();
    let event_ids = [Uuid::new_v4(), Uuid::new_v4()];

    sqlx::query(
        r#"INSERT INTO consent_records
               (id,anonymous_session_id,policy_version,analytics_allowed,granted_at,expires_at)
           VALUES ($1,$2,'retention-split-v1',true,$3,NULL)"#,
    )
    .bind(consent_id)
    .bind(session_id)
    .bind(first_event_at)
    .execute(&pool)
    .await
    .expect("split-cutoff consent fixture");
    sqlx::query(
        r#"INSERT INTO guest_visits
               (id,anonymous_session_id,consent_record_id,consent_analytics_allowed,
                locale,landing_path,source_type,referrer_host,utm_source,utm_medium,
                utm_campaign,first_seen_at,last_seen_at,retention_until,created_at)
           VALUES ($1,$2,$3,true,'en',$4,'referral',$5,'split-source',
                   'split-medium','split-campaign',$6,$7,$8,$6)"#,
    )
    .bind(visit_id)
    .bind(session_id)
    .bind(consent_id)
    .bind(&landing_path)
    .bind(&source_name)
    .bind(first_event_at)
    .bind(second_event_at)
    .bind(first_event_at + Duration::days(30))
    .execute(&pool)
    .await
    .expect("split-cutoff guest visit fixture");
    for (event_id, occurred_at) in event_ids.into_iter().zip([first_event_at, second_event_at]) {
        sqlx::query(
            r#"INSERT INTO analytics_events
                   (id,event_name,source_path,locale,anonymous_session_id,properties,
                    occurred_at,guest_visit_id,consent_record_id)
               VALUES ($1,'pageView',$2,'en',$3,'{}'::jsonb,$4,$5,$6)"#,
        )
        .bind(event_id)
        .bind(&landing_path)
        .bind(session_id)
        .bind(occurred_at)
        .bind(visit_id)
        .bind(consent_id)
        .execute(&pool)
        .await
        .expect("split-cutoff page view fixture");
    }

    let report_before = analytics_rows(&database_url, &landing_path).await;
    assert_eq!(report_before.0.visits, 1);
    assert_eq!(report_before.0.page_views, 2);
    let overview_from =
        DateTime::<Utc>::from_naive_utc_and_offset(bucket_date.and_hms_opt(0, 0, 0).unwrap(), Utc);
    let overview_before = analytics_overview(
        &database_url,
        overview_from,
        overview_from + Duration::days(1),
    )
    .await;
    assert_eq!(overview_before.consented_metrics.visits, 1);
    assert_eq!(overview_before.consented_metrics.page_views, 2);
    assert_eq!(overview_before.consented_metrics.engaged_visit_days, 1);

    let first_result = apply_retention(&pool).await.expect("first cutoff run");
    assert_eq!(first_result["analyticsEventsDeleted"], json!(1));
    let first_state = sqlx::query_as::<_, (i64, i64, bool)>(
        r#"SELECT daily.page_views,daily.engaged_visits,ledger.engaged_materialized
           FROM guest_source_daily AS daily
           INNER JOIN guest_visit_daily_event_materializations AS ledger
             ON ledger.guest_visit_id=$1 AND ledger.bucket_date=daily.bucket_date
           WHERE daily.bucket_date=$2 AND daily.source_name=$3
             AND daily.landing_path=$4"#,
    )
    .bind(visit_id)
    .bind(bucket_date)
    .bind(&source_name)
    .bind(&landing_path)
    .fetch_one(&pool)
    .await
    .expect("first cutoff materialization state");
    assert_eq!(first_state, (1, 0, false));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM analytics_events WHERE id = ANY($1)",)
            .bind(event_ids.as_slice())
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        serde_json::to_value(analytics_rows(&database_url, &landing_path).await).unwrap(),
        serde_json::to_value(&report_before).unwrap(),
        "historical plus live data must remain stable between cutoffs"
    );
    let overview_after_first_cutoff = analytics_overview(
        &database_url,
        overview_from,
        overview_from + Duration::days(1),
    )
    .await;
    assert_eq!(
        serde_json::to_value(&overview_after_first_cutoff.consented_metrics).unwrap(),
        serde_json::to_value(&overview_before.consented_metrics).unwrap(),
        "the ledger must contribute only the unmaterialized engagement delta"
    );

    sqlx::query(
        "UPDATE app_settings SET value='2'::jsonb,updated_at=now(),updated_by='retention-split-test' WHERE key='analyticsEventRetentionDays'",
    )
    .execute(&pool)
    .await
    .expect("advance split-cutoff retention policy");
    let (concurrent_a, concurrent_b) = tokio::join!(apply_retention(&pool), apply_retention(&pool));
    let concurrent_a = concurrent_a.expect("first concurrent retention run");
    let concurrent_b = concurrent_b.expect("second concurrent retention run");
    assert_eq!(
        concurrent_a["analyticsEventsDeleted"].as_u64().unwrap()
            + concurrent_b["analyticsEventsDeleted"].as_u64().unwrap(),
        1,
        "serialized concurrent workers must consume the second event once"
    );

    let final_counters = sqlx::query_as::<_, (i64, i64)>(
        r#"SELECT page_views,engaged_visits FROM guest_source_daily
           WHERE bucket_date=$1 AND source_name=$2 AND landing_path=$3"#,
    )
    .bind(bucket_date)
    .bind(&source_name)
    .bind(&landing_path)
    .fetch_one(&pool)
    .await
    .expect("final split-cutoff aggregate");
    assert_eq!(final_counters, (2, 1));
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM guest_visit_daily_event_materializations WHERE guest_visit_id=$1",
        )
        .bind(visit_id)
        .fetch_one(&pool)
        .await
        .unwrap(),
        0,
        "the short-lived ledger is removed after the UTC day is fully retained"
    );
    assert_eq!(
        serde_json::to_value(analytics_rows(&database_url, &landing_path).await).unwrap(),
        serde_json::to_value(&report_before).unwrap(),
        "the report must remain stable after the second cutoff"
    );
    let overview_after_second_cutoff = analytics_overview(
        &database_url,
        overview_from,
        overview_from + Duration::days(1),
    )
    .await;
    assert_eq!(
        serde_json::to_value(&overview_after_second_cutoff.consented_metrics).unwrap(),
        serde_json::to_value(&overview_before.consented_metrics).unwrap(),
        "fully materialized overview metrics must match the original raw cohort"
    );

    let retry_result = apply_retention(&pool).await.expect("idempotent retry");
    assert_eq!(retry_result["analyticsEventsDeleted"], json!(0));
    let retry_counters = sqlx::query_as::<_, (i64, i64)>(
        r#"SELECT page_views,engaged_visits FROM guest_source_daily
           WHERE bucket_date=$1 AND source_name=$2 AND landing_path=$3"#,
    )
    .bind(bucket_date)
    .bind(&source_name)
    .bind(&landing_path)
    .fetch_one(&pool)
    .await
    .expect("aggregate after idempotent retry");
    assert_eq!(retry_counters, (2, 1));

    sqlx::query("DELETE FROM guest_source_daily WHERE landing_path=$1")
        .bind(&landing_path)
        .execute(&pool)
        .await
        .expect("split-cutoff aggregate cleanup");
    sqlx::query("DELETE FROM guest_visits WHERE id=$1")
        .bind(visit_id)
        .execute(&pool)
        .await
        .expect("split-cutoff visit cleanup");
    sqlx::query("DELETE FROM consent_records WHERE id=$1")
        .bind(consent_id)
        .execute(&pool)
        .await
        .expect("split-cutoff consent cleanup");
    for (key, value) in original_settings {
        sqlx::query(
            "UPDATE app_settings SET value=$2,updated_at=now(),updated_by='retention-split-test-restore' WHERE key=$1",
        )
        .bind(key)
        .bind(value)
        .execute(&pool)
        .await
        .expect("restore retention setting");
    }
}
