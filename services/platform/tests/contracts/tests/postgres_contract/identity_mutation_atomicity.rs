use super::*;

#[tokio::test]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn identity_mutations_are_idempotent_and_commit_with_their_audit_records() {
    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let sandbox = support::DatabaseClone::create(&database_url).await;
    sandbox.apply_current().await;
    let pool = sandbox.pool().clone();
    support::assert_flyway_schema_current(&pool).await;

    let run_id = Uuid::new_v4();
    let actor_id = Uuid::new_v4();
    let target_id = Uuid::new_v4();
    let custom_role_id = Uuid::new_v4();
    let actor_label = format!("identity-contract-{run_id}@example.com");
    let target_email = format!("identity-target-{run_id}@example.com");
    let custom_role_key = format!("identity-contract-{}", run_id.simple());
    sqlx::query(
        r#"INSERT INTO users
           (id,email,password_hash,display_name,status,totp_confirmed_at,created_at,updated_at)
           VALUES
           ($1,$2,'test-only-not-a-login-hash','TEST ONLY Identity Actor','active',now(),now(),now()),
           ($3,$4,'test-only-not-a-login-hash','TEST ONLY Identity Target','active',now(),now(),now())"#,
    )
    .bind(actor_id)
    .bind(&actor_label)
    .bind(target_id)
    .bind(&target_email)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO user_roles(user_id,role_id)
           SELECT $1,id FROM roles WHERE key='super-admin'
           UNION ALL
           SELECT $2,id FROM roles WHERE key='content-editor'"#,
    )
    .bind(actor_id)
    .bind(target_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO roles(id,key,display_name,system_role,revision)
           VALUES ($1,$2,'TEST ONLY Mutable Role',false,1)"#,
    )
    .bind(custom_role_id)
    .bind(&custom_role_key)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO role_permissions(role_id,permission_key) VALUES ($1,'audit.read')")
        .bind(custom_role_id)
        .execute(&pool)
        .await
        .unwrap();

    let principal = AdminPrincipal {
        user_id: actor_id,
        display_name: "TEST ONLY Identity Actor".into(),
        email: actor_label.clone(),
        role: "super-admin".into(),
        permissions: vec!["identity.manage".into()],
        session_id: Uuid::new_v4(),
        session_token_hash: vec![1; 32],
        csrf_hash: vec![2; 32],
        totp_enabled: true,
        development_password_only: false,
    };
    let app = airtek_http::routes::admin_data::router()
        .layer(Extension(principal))
        .with_state(postgres_state(sandbox.connection_url()));

    let user_key = format!("identity-user-{run_id}");
    let user_request = |actor: &str, key: &str, revision: i64, display_name: &str| {
        Request::patch(format!("/users/{target_id}"))
            .header("x-airtek-authenticated-actor", actor)
            .header("x-request-id", Uuid::new_v4().to_string())
            .header("idempotency-key", key)
            .header(header::IF_MATCH, format!("\"revision-{revision}\""))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({
                    "displayName": display_name,
                    "reason": "Verify atomic administrator update"
                })
                .to_string(),
            ))
            .unwrap()
    };
    let first_user = app
        .clone()
        .oneshot(user_request(
            &actor_label,
            &user_key,
            1,
            "TEST ONLY Updated Identity Target",
        ))
        .await
        .unwrap();
    assert_eq!(first_user.status(), StatusCode::OK);
    assert_eq!(
        first_user.headers().get(header::ETAG).unwrap(),
        "\"revision-2\""
    );
    let first_user_body = first_user.into_body().collect().await.unwrap().to_bytes();
    let replay_user = app
        .clone()
        .oneshot(user_request(
            &actor_label,
            &user_key,
            1,
            "TEST ONLY Updated Identity Target",
        ))
        .await
        .unwrap();
    assert_eq!(replay_user.status(), StatusCode::OK);
    assert_eq!(
        replay_user.into_body().collect().await.unwrap().to_bytes(),
        first_user_body
    );
    let user_revision: i64 = sqlx::query_scalar("SELECT revision FROM users WHERE id=$1")
        .bind(target_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let user_audits: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_log WHERE action='identity.user.update' AND entity_id=$1",
    )
    .bind(target_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(user_revision, 2);
    assert_eq!(user_audits, 1);

    let role_key = format!("identity-role-{run_id}");
    let role_request = |actor: &str, key: &str, revision: i64, display_name: &str| {
        Request::patch(format!("/roles/{custom_role_id}"))
            .header("x-airtek-authenticated-actor", actor)
            .header("x-request-id", Uuid::new_v4().to_string())
            .header("idempotency-key", key)
            .header(header::IF_MATCH, format!("\"revision-{revision}\""))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({
                    "displayName": display_name,
                    "permissions": ["content.read", "audit.read"],
                    "reason": "Verify atomic role update"
                })
                .to_string(),
            ))
            .unwrap()
    };
    let first_role = app
        .clone()
        .oneshot(role_request(
            &actor_label,
            &role_key,
            1,
            "TEST ONLY Updated Mutable Role",
        ))
        .await
        .unwrap();
    assert_eq!(first_role.status(), StatusCode::OK);
    assert_eq!(
        first_role.headers().get(header::ETAG).unwrap(),
        "\"revision-2\""
    );
    let first_role_body = first_role.into_body().collect().await.unwrap().to_bytes();
    let replay_role = app
        .clone()
        .oneshot(role_request(
            &actor_label,
            &role_key,
            1,
            "TEST ONLY Updated Mutable Role",
        ))
        .await
        .unwrap();
    assert_eq!(replay_role.status(), StatusCode::OK);
    assert_eq!(
        replay_role.into_body().collect().await.unwrap().to_bytes(),
        first_role_body
    );
    let role_revision: i64 = sqlx::query_scalar("SELECT revision FROM roles WHERE id=$1")
        .bind(custom_role_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let role_audits: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_log WHERE action='identity.role.update' AND entity_id=$1",
    )
    .bind(custom_role_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(role_revision, 2);
    assert_eq!(role_audits, 1);

    let invitation_email = format!("identity-invite-{run_id}@example.com");
    let expired_invitation_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO user_invitations
           (id,email,display_name,locale,status,token_hash,invited_by,invited_at,expires_at)
           VALUES ($1,$2,'TEST ONLY Expired Invitee','zh-CN','pending',$3,$4,
                   now() - interval '8 days',now() - interval '1 day')"#,
    )
    .bind(expired_invitation_id)
    .bind(&invitation_email)
    .bind(Sha256::digest(format!("expired-{run_id}")).to_vec())
    .bind(actor_id)
    .execute(&pool)
    .await
    .unwrap();
    let content_editor_role: Uuid =
        sqlx::query_scalar("SELECT id FROM roles WHERE key='content-editor'")
            .fetch_one(&pool)
            .await
            .unwrap();
    sqlx::query("INSERT INTO user_invitation_roles(invitation_id,role_id) VALUES ($1,$2)")
        .bind(expired_invitation_id)
        .bind(content_editor_role)
        .execute(&pool)
        .await
        .unwrap();

    let invitation_key = format!("identity-invitation-{run_id}");
    let invitation_request = || {
        Request::post("/user-invitations")
            .header("x-airtek-authenticated-actor", &actor_label)
            .header("x-request-id", Uuid::new_v4().to_string())
            .header("idempotency-key", &invitation_key)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({
                    "email": invitation_email,
                    "displayName": "TEST ONLY Replacement Invitee",
                    "roleKeys": ["content-editor"]
                })
                .to_string(),
            ))
            .unwrap()
    };
    let invitation = app.clone().oneshot(invitation_request()).await.unwrap();
    assert_eq!(invitation.status(), StatusCode::CREATED);
    assert_eq!(
        invitation.headers().get(header::CACHE_CONTROL).unwrap(),
        "private, no-store, max-age=0"
    );
    let invitation: serde_json::Value =
        serde_json::from_slice(&invitation.into_body().collect().await.unwrap().to_bytes())
            .unwrap();
    let invitation_id = Uuid::parse_str(invitation["id"].as_str().unwrap()).unwrap();
    let raw_token = invitation["invitationToken"].as_str().unwrap().to_owned();
    let replay_invitation = app.clone().oneshot(invitation_request()).await.unwrap();
    assert_eq!(replay_invitation.status(), StatusCode::CREATED);
    let replay_invitation: serde_json::Value = serde_json::from_slice(
        &replay_invitation
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes(),
    )
    .unwrap();
    assert_eq!(replay_invitation["id"], invitation["id"]);
    assert_eq!(
        replay_invitation["invitationToken"].as_str(),
        Some(raw_token.as_str())
    );
    let expired_status: String =
        sqlx::query_scalar("SELECT status FROM user_invitations WHERE id=$1")
            .bind(expired_invitation_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let pending_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM user_invitations WHERE lower(email)=lower($1) AND status='pending'",
    )
    .bind(&invitation_email)
    .fetch_one(&pool)
    .await
    .unwrap();
    let invitation_audits: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_log WHERE action='identity.invitation.create' AND entity_id=$1",
    )
    .bind(invitation_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let expired_audits: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_log WHERE action='identity.invitation.expire' AND entity_id=$1",
    )
    .bind(expired_invitation_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let invitation_key_hash = format!("{:x}", Sha256::digest(invitation_key.as_bytes()));
    let stored_replay: serde_json::Value = sqlx::query_scalar(
        "SELECT response_body FROM idempotency_keys WHERE scope='admin.identity.invitation.create' AND key_hash=$1",
    )
    .bind(invitation_key_hash)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(expired_status, "expired");
    assert_eq!(pending_count, 1);
    assert_eq!(invitation_audits, 1);
    assert_eq!(expired_audits, 1);
    assert_eq!(stored_replay["version"], 1);
    assert!(stored_replay["nonce"].is_string());
    assert!(stored_replay["ciphertext"].is_string());
    assert!(stored_replay.get("invitationToken").is_none());
    assert!(!stored_replay.to_string().contains(&raw_token));

    let revoke_key = format!("identity-revoke-invitation-{run_id}");
    let revoke_request = || {
        Request::post(format!("/user-invitations/{invitation_id}/revoke"))
            .header("x-airtek-authenticated-actor", &actor_label)
            .header("x-request-id", Uuid::new_v4().to_string())
            .header("idempotency-key", &revoke_key)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(
                json!({"reason": "Verify atomic invitation revocation"}).to_string(),
            ))
            .unwrap()
    };
    let revoked = app.clone().oneshot(revoke_request()).await.unwrap();
    assert_eq!(revoked.status(), StatusCode::NO_CONTENT);
    let replay_revoked = app.clone().oneshot(revoke_request()).await.unwrap();
    assert_eq!(replay_revoked.status(), StatusCode::NO_CONTENT);
    let invitation_status: String =
        sqlx::query_scalar("SELECT status FROM user_invitations WHERE id=$1")
            .bind(invitation_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let revoke_audits: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_log WHERE action='identity.invitation.revoke' AND entity_id=$1",
    )
    .bind(invitation_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(invitation_status, "revoked");
    assert_eq!(revoke_audits, 1);

    let session_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO sessions
           (id,user_id,token_hash,csrf_hash,created_at,expires_at,last_seen_at)
           VALUES ($1,$2,$3,$4,now(),now() + interval '1 day',now())"#,
    )
    .bind(session_id)
    .bind(target_id)
    .bind(Sha256::digest(format!("session-{run_id}")).to_vec())
    .bind(Sha256::digest(format!("csrf-{run_id}")).to_vec())
    .execute(&pool)
    .await
    .unwrap();
    let sessions_revoked = app
        .clone()
        .oneshot(
            Request::delete(format!("/users/{target_id}/sessions"))
                .header("x-airtek-authenticated-actor", &actor_label)
                .header("x-request-id", Uuid::new_v4().to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(sessions_revoked.status(), StatusCode::NO_CONTENT);
    let revoked_at: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT revoked_at FROM sessions WHERE id=$1")
            .bind(session_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let session_audits: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_log WHERE action='identity.sessions.revoke' AND entity_id=$1",
    )
    .bind(target_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(revoked_at.is_some());
    assert_eq!(session_audits, 1);

    // Force audit insertion to fail for a single test actor. The business
    // changes must roll back with the audit row for all three mutation types.
    let failure_actor = format!("identity-audit-failure-{run_id}@example.com");
    let function_name = format!("test_identity_audit_failure_{}", run_id.simple());
    let trigger_name = format!("test_identity_audit_failure_trigger_{}", run_id.simple());
    sqlx::query(&format!(
        r#"CREATE FUNCTION "{function_name}"() RETURNS trigger LANGUAGE plpgsql AS $$
           BEGIN
             IF NEW.actor = '{failure_actor}' THEN
               RAISE EXCEPTION 'TEST ONLY forced identity audit failure';
             END IF;
             RETURN NEW;
           END $$"#,
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(&format!(
        r#"CREATE TRIGGER "{trigger_name}" BEFORE INSERT ON audit_log
           FOR EACH ROW EXECUTE FUNCTION "{function_name}"()"#,
    ))
    .execute(&pool)
    .await
    .unwrap();

    let failed_user = app
        .clone()
        .oneshot(user_request(
            &failure_actor,
            &format!("identity-user-failure-{run_id}"),
            2,
            "TEST ONLY This Must Roll Back",
        ))
        .await
        .unwrap();
    assert_eq!(failed_user.status(), StatusCode::SERVICE_UNAVAILABLE);
    let (user_name, user_revision): (String, i64) =
        sqlx::query_as("SELECT display_name,revision FROM users WHERE id=$1")
            .bind(target_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(user_name, "TEST ONLY Updated Identity Target");
    assert_eq!(user_revision, 2);

    let failed_role = app
        .clone()
        .oneshot(role_request(
            &failure_actor,
            &format!("identity-role-failure-{run_id}"),
            2,
            "TEST ONLY This Role Must Roll Back",
        ))
        .await
        .unwrap();
    assert_eq!(failed_role.status(), StatusCode::SERVICE_UNAVAILABLE);
    let (role_name, role_revision): (String, i64) =
        sqlx::query_as("SELECT display_name,revision FROM roles WHERE id=$1")
            .bind(custom_role_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(role_name, "TEST ONLY Updated Mutable Role");
    assert_eq!(role_revision, 2);

    let rollback_session_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO sessions
           (id,user_id,token_hash,csrf_hash,created_at,expires_at,last_seen_at)
           VALUES ($1,$2,$3,$4,now(),now() + interval '1 day',now())"#,
    )
    .bind(rollback_session_id)
    .bind(target_id)
    .bind(Sha256::digest(format!("rollback-session-{run_id}")).to_vec())
    .bind(Sha256::digest(format!("rollback-csrf-{run_id}")).to_vec())
    .execute(&pool)
    .await
    .unwrap();
    let failed_revoke = app
        .oneshot(
            Request::delete(format!("/users/{target_id}/sessions"))
                .header("x-airtek-authenticated-actor", &failure_actor)
                .header("x-request-id", Uuid::new_v4().to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(failed_revoke.status(), StatusCode::SERVICE_UNAVAILABLE);
    let rollback_revoked_at: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT revoked_at FROM sessions WHERE id=$1")
            .bind(rollback_session_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(rollback_revoked_at.is_none());

    sqlx::query(&format!(r#"DROP TRIGGER "{trigger_name}" ON audit_log"#))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(&format!(r#"DROP FUNCTION "{function_name}"()"#))
        .execute(&pool)
        .await
        .unwrap();

    sandbox.cleanup().await;
}
