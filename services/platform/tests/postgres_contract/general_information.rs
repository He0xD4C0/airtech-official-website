#[tokio::test]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn general_information_revision_pointers_are_deferred_and_locale_bound() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .expect("PostgreSQL connection");
    support::assert_flyway_schema_current(&pool).await;

    let id = Uuid::new_v4();
    let locale = format!("x-test-{}", &id.simple().to_string()[..8]);
    let now = chrono::Utc::now();
    let mut initial = pool.begin().await.unwrap();
    sqlx::query(
        r#"INSERT INTO general_information
               (id,scope,locale,status,is_placeholder,data_origin,current_revision,
                published_revision,payload,updated_by,updated_at)
           VALUES ($1,'site',$2,'draft',false,'editorial',1,NULL,$3,
                   'postgres-contract',$4)"#,
    )
    .bind(id)
    .bind(&locale)
    .bind(json!({"brandDisplayName": "GI pointer contract"}))
    .bind(now)
    .execute(&mut *initial)
    .await
    .expect("deferred current pointer permits parent-first insertion");
    sqlx::query(
        r#"INSERT INTO general_information_revisions
               (general_information_id,revision,payload,created_by,created_at,
                locale,is_placeholder,data_origin)
           VALUES ($1,1,$2,'postgres-contract',$3,$4,false,'editorial')"#,
    )
    .bind(id)
    .bind(json!({"brandDisplayName": "GI pointer contract"}))
    .bind(now)
    .bind(&locale)
    .execute(&mut *initial)
    .await
    .unwrap();
    initial.commit().await.expect("matching deferred pointer");

    let app = airtek_platform::routes::admin::router().with_state(postgres_state(&database_url));
    let immutable_locale = app
        .oneshot(
            Request::patch(format!("/general-information/{id}"))
                .header(
                    "x-airtek-authenticated-actor",
                    "postgres-contract@example.com",
                )
                .header(
                    "idempotency-key",
                    format!("postgres-general-information-locale-{id}"),
                )
                .header(header::IF_MATCH, "\"revision-1\"")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "locale": format!("{locale}-api-mismatch"),
                        "payload": {
                            "brandName": "AIRTEKPOWER",
                            "brandLine": null,
                            "homePath": "/en",
                            "footerStatement": null,
                            "copyrightText": null,
                            "defaultSeo": {"title": null, "description": null},
                            "organization": {"name": "AIRTEKPOWER"}
                        },
                        "isPlaceholder": false
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(immutable_locale.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = immutable_locale
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes();
    let problem: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        problem["errors"]["locale"],
        json!(["Locale is immutable after General Information is created."])
    );
    let unchanged = sqlx::query_as::<_, (String, i64)>(
        "SELECT locale,current_revision FROM general_information WHERE id=$1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(unchanged, (locale.clone(), 1));

    let wrong_locale = format!("{locale}-other");
    let mut mismatch = pool.begin().await.unwrap();
    sqlx::query(
        r#"INSERT INTO general_information_revisions
               (general_information_id,revision,payload,created_by,created_at,
                locale,is_placeholder,data_origin)
           VALUES ($1,2,$2,'postgres-contract',$3,$4,false,'editorial')"#,
    )
    .bind(id)
    .bind(json!({"brandDisplayName": "Wrong locale"}))
    .bind(now)
    .bind(&wrong_locale)
    .execute(&mut *mismatch)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE general_information SET current_revision=2,published_revision=2 WHERE id=$1",
    )
    .bind(id)
    .execute(&mut *mismatch)
    .await
    .expect("the deferred constraint is checked at commit");
    let mismatch_error = mismatch
        .commit()
        .await
        .expect_err("a pointer cannot resolve to a different revision locale");
    assert_eq!(
        mismatch_error
            .as_database_error()
            .and_then(|error| error.code())
            .as_deref(),
        Some("23503")
    );

    sqlx::query("DELETE FROM general_information WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
}
