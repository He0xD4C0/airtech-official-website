#[tokio::test]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn preview_ticket_revalidates_postgres_identity_session_and_permission() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let sandbox = support::MigrationSandbox::create(&database_url).await;
    sandbox.apply_current().await;
    let pool = sandbox.pool().clone();
    support::assert_flyway_schema_current(&pool).await;

    let user_id = Uuid::new_v4();
    let role_id = Uuid::new_v4();
    let session_id = Uuid::new_v4();
    let suffix = user_id.simple();
    sqlx::query(
        r#"INSERT INTO roles(id,key,display_name,system_role)
           VALUES ($1,$2,'TEST ONLY Preview Reader',false)"#,
    )
    .bind(role_id)
    .bind(format!("preview-reader-{suffix}"))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO users
           (id,email,password_hash,display_name,status,totp_confirmed_at,created_at,updated_at)
           VALUES ($1,$2,'test-only-not-a-login-hash','TEST ONLY Preview Reader',
                   'active',now(),now(),now())"#,
    )
    .bind(user_id)
    .bind(format!("preview-reader-{suffix}@example.com"))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO user_roles(user_id,role_id) VALUES ($1,$2)")
        .bind(user_id)
        .bind(role_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO role_permissions(role_id,permission_key) VALUES ($1,'content.read')")
        .bind(role_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT INTO sessions
           (id,user_id,token_hash,csrf_hash,created_at,expires_at,last_seen_at)
           VALUES ($1,$2,$3,$4,now(),now() + interval '1 hour',now())"#,
    )
    .bind(session_id)
    .bind(user_id)
    .bind(Sha256::digest(format!("preview-token-{session_id}")).to_vec())
    .bind(Sha256::digest(format!("preview-csrf-{session_id}")).to_vec())
    .execute(&pool)
    .await
    .unwrap();

    let state = postgres_state(sandbox.connection_url());
    let content_id = Uuid::new_v4();
    let slug = format!("preview-pg-{suffix}");
    let updated_at = chrono::Utc::now();
    let legacy_payload = json!({
        "id": content_id,
        "kind": "article",
        "slug": slug,
        "locale": "en",
        "title": "LEGACY PAYLOAD MUST NOT RENDER",
        "summary": null,
        "body": {"schemaVersion": 1, "doc": {"type": "doc", "content": []}},
        "seo": {"title": null, "description": null, "canonicalPath": null, "indexable": false},
        "status": "draft",
        "isPlaceholder": false,
        "currentRevision": 1,
        "publishedRevision": null,
        "scheduledFor": null,
        "updatedAt": updated_at
    });
    let document = json!({
        "schemaVersion": 2,
        "kind": "article",
        "locale": "en",
        "templateKey": "articleDetail",
        "title": "PostgreSQL-bound CMS V2 private preview",
        "slug": slug,
        "summary": "Exact immutable V2 revision",
        "isPlaceholder": false,
        "typeFields": {
            "type": "article", "category": "Engineering",
            "authorDisplayName": "AIRTEKPOWER", "publicationAt": null,
            "cover": null, "featured": false
        },
        "body": {"type": "doc", "content": [{"type": "paragraph"}]},
        "composition": {"blocks": [
            {"type": "hero", "id": Uuid::new_v4(), "eyebrow": "Preview",
             "heading": "V2 preview heading", "lead": null, "media": null,
             "actions": [], "variant": "standard"},
            {"type": "body", "id": Uuid::new_v4(), "width": "standard"}
        ]},
        "seo": {"title": null, "description": null, "indexable": false, "socialImage": null},
        "relations": [],
        "draftVersion": 1
    });
    sqlx::query(
        r#"INSERT INTO content_entries
           (id,kind,slug,locale,title,status,is_placeholder,current_revision,
            published_revision,scheduled_for,payload,updated_at,data_origin,
            template_key,latest_revision,cms_published_revision,cms_created_at,cms_updated_by)
           VALUES ($1,'article',$2,'en',$3,'draft',false,2,NULL,NULL,$4,$5,
                   'editorial','articleDetail',2,NULL,$5,'postgres-contract')"#,
    )
    .bind(content_id)
    .bind(&slug)
    .bind("PostgreSQL-bound CMS V2 private preview")
    .bind(&legacy_payload)
    .bind(updated_at)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO content_revisions
           (content_id,revision,payload,document,source_draft_version,revision_kind,
            reason,created_by,created_at)
           VALUES ($1,1,$2,$3,1,'manual','Preview the exact CMS V2 revision',
                   'postgres-contract',$4)"#,
    )
    .bind(content_id)
    .bind(&legacy_payload)
    .bind(document)
    .bind(updated_at)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO content_revisions
           (content_id,revision,payload,created_by,created_at)
           VALUES ($1,2,$2,'postgres-contract',$3)"#,
    )
    .bind(content_id)
    .bind(&legacy_payload)
    .bind(updated_at)
    .execute(&pool)
    .await
    .unwrap();
    let token = airtek_platform::preview_token::issue(
        state.config.preview_signing_key.as_ref().unwrap(),
        content_id,
        1,
        user_id,
        session_id,
        600,
    )
    .unwrap()
    .token;
    let legacy_only_token = airtek_platform::preview_token::issue(
        state.config.preview_signing_key.as_ref().unwrap(),
        content_id,
        2,
        user_id,
        session_id,
        600,
    )
    .unwrap()
    .token;
    let app = build_router(state);
    let preview_request = |preview_token: &str| {
        Request::get("/api/public/v1/content-preview")
            .header(header::AUTHORIZATION, format!("Bearer {preview_token}"))
            .body(Body::empty())
            .unwrap()
    };

    let valid = app.clone().oneshot(preview_request(&token)).await.unwrap();
    assert_eq!(valid.status(), StatusCode::OK);
    assert_eq!(
        valid.headers().get(header::CACHE_CONTROL).unwrap(),
        "private, no-store, max-age=0"
    );
    assert_eq!(
        valid.headers().get("x-robots-tag").unwrap(),
        "noindex, nofollow, noarchive"
    );
    let preview: Value =
        serde_json::from_slice(&valid.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(preview["content"]["schemaVersion"], 2);
    assert_eq!(
        preview["content"]["title"],
        "PostgreSQL-bound CMS V2 private preview"
    );
    assert_eq!(preview["content"]["publishedRevision"], 1);
    assert!(preview["content"].get("payload").is_none());
    assert!(preview["content"].get("currentRevision").is_none());
    assert_eq!(
        app.clone()
            .oneshot(preview_request(&legacy_only_token))
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND,
        "a signed token must not revive a payload-only legacy revision"
    );

    sqlx::query("UPDATE users SET status='disabled' WHERE id=$1")
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        app.clone()
            .oneshot(preview_request(&token))
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    sqlx::query("UPDATE users SET status='active' WHERE id=$1")
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM role_permissions WHERE role_id=$1 AND permission_key='content.read'")
        .bind(role_id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        app.clone()
            .oneshot(preview_request(&token))
            .await
            .unwrap()
            .status(),
        StatusCode::NOT_FOUND
    );
    sqlx::query("INSERT INTO role_permissions(role_id,permission_key) VALUES ($1,'content.read')")
        .bind(role_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE sessions SET revoked_at=now() WHERE id=$1")
        .bind(session_id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        app.oneshot(preview_request(&token)).await.unwrap().status(),
        StatusCode::NOT_FOUND
    );

    sandbox.cleanup().await;
}

#[tokio::test]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn invitation_acceptance_is_one_time_atomic_and_hash_only() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let sandbox = support::MigrationSandbox::create(&database_url).await;
    sandbox.apply_current().await;
    let pool = sandbox.pool().clone();
    support::assert_flyway_schema_current(&pool).await;

    let inviter_id = Uuid::new_v4();
    let suffix = inviter_id.simple();
    sqlx::query(
        r#"INSERT INTO users(id,email,password_hash,display_name,status,created_at,updated_at)
           VALUES ($1,$2,'test-only-not-a-login-hash','TEST ONLY Inviter','active',now(),now())"#,
    )
    .bind(inviter_id)
    .bind(format!("inviter-{suffix}@example.com"))
    .execute(&pool)
    .await
    .unwrap();
    let role_id: Uuid = sqlx::query_scalar("SELECT id FROM roles WHERE key='content-editor'")
        .fetch_one(&pool)
        .await
        .unwrap();

    let insert_invitation = |id: Uuid,
                             email: String,
                             token: String,
                             invited_at: chrono::DateTime<chrono::Utc>,
                             expires_at: chrono::DateTime<chrono::Utc>,
                             status: &'static str| {
        let pool = pool.clone();
        async move {
            let token_hash = Sha256::digest(token.as_bytes()).to_vec();
            if status == "revoked" {
                sqlx::query(
                    r#"INSERT INTO user_invitations
                       (id,email,display_name,locale,status,token_hash,invited_by,invited_at,
                        expires_at,revoked_at,revoked_by,revoke_reason)
                       VALUES ($1,$2,'TEST ONLY Invitee','zh-CN','revoked',$3,$4,$5,$6,$7,$4,
                               'TEST ONLY revoked invitation')"#,
                )
                .bind(id)
                .bind(email)
                .bind(token_hash)
                .bind(inviter_id)
                .bind(invited_at)
                .bind(expires_at)
                .bind(invited_at + chrono::Duration::minutes(1))
                .execute(&pool)
                .await
                .unwrap();
            } else {
                sqlx::query(
                    r#"INSERT INTO user_invitations
                       (id,email,display_name,locale,status,token_hash,invited_by,invited_at,expires_at)
                       VALUES ($1,$2,'TEST ONLY Invitee','zh-CN','pending',$3,$4,$5,$6)"#,
                )
                .bind(id)
                .bind(email)
                .bind(token_hash)
                .bind(inviter_id)
                .bind(invited_at)
                .bind(expires_at)
                .execute(&pool)
                .await
                .unwrap();
            }
            sqlx::query("INSERT INTO user_invitation_roles(invitation_id,role_id) VALUES ($1,$2)")
                .bind(id)
                .bind(role_id)
                .execute(&pool)
                .await
                .unwrap();
        }
    };
    let acceptance_request = |token: &str, password: &str, request_id: Uuid| {
        Request::post("/api/admin/v1/auth/invitations/accept")
            .header(header::ORIGIN, "http://localhost:3100")
            .header(header::CONTENT_TYPE, "application/json")
            .header("x-request-id", request_id.to_string())
            .body(Body::from(
                json!({"token": token, "password": password}).to_string(),
            ))
            .unwrap()
    };

    let now = chrono::Utc::now();
    let invitation_id = Uuid::new_v4();
    let email = format!("invitee-{suffix}@example.com");
    let token = URL_SAFE_NO_PAD.encode(Sha256::digest(invitation_id.as_bytes()));
    let password = "new-admin-password-123";
    insert_invitation(
        invitation_id,
        email.clone(),
        token.clone(),
        now,
        now + chrono::Duration::days(7),
        "pending",
    )
    .await;

    let app = build_router(postgres_state(sandbox.connection_url()));
    let request_id = Uuid::new_v4();
    let accepted = app
        .clone()
        .oneshot(acceptance_request(&token, password, request_id))
        .await
        .unwrap();
    assert_eq!(accepted.status(), StatusCode::CREATED);
    assert_eq!(
        accepted.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store, max-age=0"
    );
    assert!(accepted.headers().get(header::SET_COOKIE).is_none());
    let accepted_body = accepted.into_body().collect().await.unwrap().to_bytes();
    let accepted_body: serde_json::Value = serde_json::from_slice(&accepted_body).unwrap();
    assert_eq!(accepted_body["email"], email);
    assert_eq!(accepted_body["status"], "active");
    assert_eq!(accepted_body["roleKeys"], json!(["content-editor"]));
    let user_id = Uuid::parse_str(accepted_body["userId"].as_str().unwrap()).unwrap();

    let (status, password_hash): (String, String) = sqlx::query_as(
        "SELECT status,password_hash FROM users WHERE id=$1 AND lower(email)=lower($2)",
    )
    .bind(user_id)
    .bind(&email)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(status, "active");
    assert!(password_hash.starts_with("$argon2id$"));
    let role_count: i64 = sqlx::query_scalar(
        r#"SELECT count(*) FROM user_roles assignment JOIN roles role ON role.id=assignment.role_id
           WHERE assignment.user_id=$1 AND role.key='content-editor'"#,
    )
    .bind(user_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(role_count, 1);
    let history_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM user_status_history WHERE user_id=$1 AND from_status='invited' AND to_status='active' AND request_id=$2",
    )
    .bind(user_id)
    .bind(request_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(history_count, 1);
    let audit_payload: String = sqlx::query_scalar(
        "SELECT concat_ws(' ',actor,before_value::text,after_value::text,reason) FROM audit_log WHERE action='identity.invitation.accept' AND entity_id=$1 AND request_id=$2",
    )
    .bind(invitation_id)
    .bind(request_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!audit_payload.contains(&token));
    assert!(!audit_payload.contains(password));

    let reused = app
        .clone()
        .oneshot(acceptance_request(
            &token,
            "another-password-456",
            Uuid::new_v4(),
        ))
        .await
        .unwrap();
    assert_eq!(reused.status(), StatusCode::UNAUTHORIZED);

    let expired_id = Uuid::new_v4();
    let expired_token = URL_SAFE_NO_PAD.encode(Sha256::digest(expired_id.as_bytes()));
    insert_invitation(
        expired_id,
        format!("expired-{suffix}@example.com"),
        expired_token.clone(),
        now - chrono::Duration::days(2),
        now - chrono::Duration::days(1),
        "pending",
    )
    .await;
    let expired = app
        .clone()
        .oneshot(acceptance_request(
            &expired_token,
            "expired-password-123",
            Uuid::new_v4(),
        ))
        .await
        .unwrap();
    assert_eq!(expired.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM user_invitations WHERE id=$1")
            .bind(expired_id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "expired"
    );

    let revoked_id = Uuid::new_v4();
    let revoked_token = URL_SAFE_NO_PAD.encode(Sha256::digest(revoked_id.as_bytes()));
    insert_invitation(
        revoked_id,
        format!("revoked-{suffix}@example.com"),
        revoked_token.clone(),
        now,
        now + chrono::Duration::days(7),
        "revoked",
    )
    .await;
    let revoked = app
        .clone()
        .oneshot(acceptance_request(
            &revoked_token,
            "revoked-password-123",
            Uuid::new_v4(),
        ))
        .await
        .unwrap();
    assert_eq!(revoked.status(), StatusCode::UNAUTHORIZED);

    let conflict_id = Uuid::new_v4();
    let conflict_token = URL_SAFE_NO_PAD.encode(Sha256::digest(conflict_id.as_bytes()));
    insert_invitation(
        conflict_id,
        email,
        conflict_token.clone(),
        now,
        now + chrono::Duration::days(7),
        "pending",
    )
    .await;
    let conflict = app
        .oneshot(acceptance_request(
            &conflict_token,
            "conflict-password-123",
            Uuid::new_v4(),
        ))
        .await
        .unwrap();
    assert_eq!(conflict.status(), StatusCode::CONFLICT);

    sandbox.cleanup().await;
}
