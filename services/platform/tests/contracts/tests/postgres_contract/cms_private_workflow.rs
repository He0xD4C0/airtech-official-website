use super::*;

fn cms_workflow_principal(
    user_id: Uuid,
    email: String,
    role: &str,
    permissions: &[&str],
) -> AdminPrincipal {
    AdminPrincipal {
        user_id,
        display_name: "TEST ONLY CMS user".into(),
        email,
        role: role.into(),
        role_keys: vec![role.into()],
        permissions: permissions.iter().map(|value| (*value).into()).collect(),
        session_id: Uuid::new_v4(),
        session_token_hash: vec![1; 32],
        csrf_hash: vec![2; 32],
        totp_enabled: true,
        must_change_password: false,
        must_confirm_recovery_key: false,
        phone_verified: false,
    }
}

fn cms_workflow_document(suffix: &str) -> airtek_domain::models::ContentDraftV2 {
    serde_json::from_value(json!({
        "schemaVersion": 2,
        "kind": "news",
        "locale": "en",
        "templateKey": "newsDetail",
        "title": format!("TEST ONLY News {suffix}"),
        "slug": format!("test-only-news-{suffix}"),
        "summary": null,
        "isPlaceholder": true,
        "typeFields": {
            "type": "news", "category": "Company", "authorDisplayName": "AIRTEKPOWER",
            "publicationAt": null, "cover": null, "featured": false
        },
        "body": {"type": "doc", "content": [{"type": "paragraph"}]},
        "composition": {"blocks": [
            {"type": "hero", "id": Uuid::new_v4(), "eyebrow": null,
             "heading": "TEST ONLY", "lead": null, "media": null,
             "actions": [], "variant": "standard"},
            {"type": "body", "id": Uuid::new_v4(), "width": "standard"}
        ]},
        "seo": {"title": null, "description": null, "indexable": false, "socialImage": null},
        "relations": [],
        "draftVersion": 1
    }))
    .unwrap()
}

async fn insert_cms_user(pool: &sqlx::PgPool, id: Uuid, email: &str) {
    sqlx::query(
        r#"INSERT INTO users(id,email,password_hash,display_name,status,created_at,updated_at)
           VALUES ($1,$2,'test-only-not-a-login-hash','TEST ONLY CMS user','active',now(),now())"#,
    )
    .bind(id)
    .bind(email)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn private_draft_review_and_overwrite_are_current_state_only() {
    use airtek_runtime::services::cms_workflow;

    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL")
        .expect("AIRTEK_TEST_DATABASE_URL must point to disposable PostgreSQL");
    let sandbox = support::DatabaseClone::create(&database_url).await;
    sandbox.apply_current().await;
    let state = postgres_state(sandbox.connection_url());
    let pool = sandbox.pool();
    let suffix = Uuid::new_v4().simple().to_string();
    let owner_id = Uuid::new_v4();
    let peer_id = Uuid::new_v4();
    let manager_id = Uuid::new_v4();
    let senior_id = Uuid::new_v4();
    let reviewer_id = Uuid::new_v4();
    let stranger_id = Uuid::new_v4();
    for (id, label) in [
        (owner_id, "owner"),
        (peer_id, "peer"),
        (manager_id, "manager"),
        (senior_id, "senior"),
        (reviewer_id, "reviewer"),
        (stranger_id, "stranger"),
    ] {
        insert_cms_user(pool, id, &format!("{label}-{suffix}@example.com")).await;
    }
    sqlx::query("UPDATE users SET manager_user_id=$2 WHERE id=$1")
        .bind(owner_id)
        .bind(manager_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("UPDATE users SET manager_user_id=$2 WHERE id=$1")
        .bind(manager_id)
        .bind(senior_id)
        .execute(pool)
        .await
        .unwrap();
    let cycle = sqlx::query("UPDATE users SET manager_user_id=$2 WHERE id=$1")
        .bind(senior_id)
        .bind(owner_id)
        .execute(pool)
        .await
        .unwrap_err();
    assert_eq!(
        cycle
            .as_database_error()
            .and_then(|error| error.code())
            .as_deref(),
        Some("23514")
    );

    let owner = cms_workflow_principal(
        owner_id,
        format!("owner-{suffix}@example.com"),
        "content-editor",
        &["content.read", "content.write"],
    );
    let peer = cms_workflow_principal(
        peer_id,
        format!("peer-{suffix}@example.com"),
        "content-editor",
        &["content.read"],
    );
    let manager = cms_workflow_principal(
        manager_id,
        format!("manager-{suffix}@example.com"),
        "manager",
        &["content.read"],
    );
    let senior = cms_workflow_principal(
        senior_id,
        format!("senior-{suffix}@example.com"),
        "manager",
        &["content.read"],
    );
    let reviewer = cms_workflow_principal(
        reviewer_id,
        format!("reviewer-{suffix}@example.com"),
        "publisher",
        &["content.publish"],
    );
    let stranger = cms_workflow_principal(
        stranger_id,
        format!("stranger-{suffix}@example.com"),
        "reader",
        &["content.read"],
    );

    let draft = cms_workflow::create_draft(&state, &owner, cms_workflow_document(&suffix))
        .await
        .unwrap();
    cms_workflow::set_draft_shares(&state, &owner, draft.draft_id, vec![peer_id])
        .await
        .unwrap();
    for visible in [&peer, &manager, &senior] {
        assert_eq!(
            cms_workflow::get_draft(&state, visible, draft.draft_id)
                .await
                .unwrap()
                .draft_id,
            draft.draft_id
        );
    }
    assert_eq!(
        cms_workflow::get_draft(&state, &stranger, draft.draft_id)
            .await
            .unwrap_err()
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        cms_workflow::save_draft(&state, &peer, draft.draft_id, 1, draft.document.clone())
            .await
            .unwrap_err()
            .status(),
        StatusCode::NOT_FOUND
    );

    let submitted = cms_workflow::submit_draft(&state, &owner, draft.draft_id, 1)
        .await
        .unwrap();
    assert_eq!(submitted.status, "pendingReview");
    assert_eq!(
        cms_workflow::save_draft(&state, &owner, draft.draft_id, 1, draft.document.clone())
            .await
            .unwrap_err()
            .status(),
        StatusCode::CONFLICT
    );
    cms_workflow::withdraw_draft(&state, &owner, draft.draft_id)
        .await
        .unwrap();
    cms_workflow::submit_draft(&state, &owner, draft.draft_id, 1)
        .await
        .unwrap();
    let rejected = cms_workflow::reject_draft(
        &state,
        &reviewer,
        draft.draft_id,
        "Needs a current correction".into(),
    )
    .await
    .unwrap();
    assert_eq!(
        rejected.rejection_reason.as_deref(),
        Some("Needs a current correction")
    );
    let mut corrected = rejected.document;
    corrected.title = "TEST ONLY corrected".into();
    let saved = cms_workflow::save_draft(&state, &owner, draft.draft_id, 1, corrected)
        .await
        .unwrap();
    assert!(saved.rejection_reason.is_none());
    cms_workflow::submit_draft(&state, &owner, draft.draft_id, 2)
        .await
        .unwrap();
    let first = cms_workflow::approve_draft(&state, &reviewer, draft.draft_id)
        .await
        .unwrap();
    assert_eq!(first.publication_version, 1);
    assert_eq!(
        cms_workflow::get_published(&state, draft.content_id)
            .await
            .unwrap()
            .document
            .title,
        "TEST ONLY corrected"
    );
    for table in ["cms_drafts", "cms_draft_shares", "cms_review_queue"] {
        let count: i64 =
            sqlx::query_scalar(&format!("SELECT count(*) FROM {table} WHERE draft_id=$1"))
                .bind(draft.draft_id)
                .fetch_one(pool)
                .await
                .unwrap();
        assert_eq!(count, 0, "{table} must be deleted after publication");
    }
    assert!(sqlx::query_scalar::<_, Option<String>>(
        "SELECT to_regclass('content_revisions')::text"
    )
    .fetch_one(pool)
    .await
    .unwrap()
    .is_none());

    let stale = cms_workflow::copy_published_to_draft(&state, &peer, draft.content_id)
        .await
        .unwrap();
    cms_workflow::submit_draft(
        &state,
        &cms_workflow_principal(
            peer_id,
            format!("peer-{suffix}@example.com"),
            "editor",
            &["content.write"],
        ),
        stale.draft_id,
        1,
    )
    .await
    .unwrap();
    let current = cms_workflow::copy_published_to_draft(&state, &owner, draft.content_id)
        .await
        .unwrap();
    let publisher = cms_workflow_principal(
        owner_id,
        format!("owner-{suffix}@example.com"),
        "publisher",
        &["content.write", "content.publish"],
    );
    let mut changed = current.document;
    changed.title = "TEST ONLY newest publication".into();
    cms_workflow::save_draft(&state, &publisher, current.draft_id, 1, changed)
        .await
        .unwrap();
    let automatic = cms_workflow::submit_draft(&state, &publisher, current.draft_id, 2)
        .await
        .unwrap();
    assert_eq!(automatic.publication.unwrap().publication_version, 2);
    assert_eq!(
        cms_workflow::approve_draft(&state, &reviewer, stale.draft_id)
            .await
            .unwrap_err()
            .status(),
        StatusCode::CONFLICT
    );
    assert_eq!(
        cms_workflow::get_draft(&state, &peer, stale.draft_id)
            .await
            .unwrap()
            .state,
        airtek_domain::models::CmsDraftState::Editing
    );

    sqlx::query("UPDATE app_settings SET value='false'::jsonb WHERE key='contentReviewRequired'")
        .execute(pool)
        .await
        .unwrap();
    let no_review = cms_workflow::create_draft(
        &state,
        &owner,
        cms_workflow_document(&format!("off-{suffix}")),
    )
    .await
    .unwrap();
    let published = cms_workflow::submit_draft(&state, &owner, no_review.draft_id, 1)
        .await
        .unwrap();
    assert_eq!(published.status, "published");
    let leaked: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_log WHERE action LIKE 'content%' AND (before_value IS NOT NULL OR after_value IS NOT NULL OR reason IS NOT NULL OR current_version IS NULL)",
    ).fetch_one(pool).await.unwrap();
    assert_eq!(leaked, 0);
    state.pool.close().await;
    sandbox.cleanup().await;
}

#[tokio::test]
#[ignore = "requires AIRTEK_TEST_DATABASE_URL pointing to disposable PostgreSQL"]
async fn five_thousand_private_drafts_page_without_duplicates() {
    use airtek_runtime::services::cms_workflow::{self, CmsListQuery};

    let database_url = std::env::var("AIRTEK_TEST_DATABASE_URL").unwrap();
    let sandbox = support::DatabaseClone::create(&database_url).await;
    let state = postgres_state(sandbox.connection_url());
    let owner_id = Uuid::new_v4();
    let suffix = Uuid::new_v4().simple().to_string();
    let email = format!("paging-{suffix}@example.com");
    insert_cms_user(sandbox.pool(), owner_id, &email).await;
    let base = serde_json::to_value(cms_workflow_document(&format!("page-{suffix}"))).unwrap();
    sqlx::query(
        r#"WITH generated AS (SELECT n,(md5($3||n::text))::uuid AS id FROM generate_series(1,5000) n),
           entries AS (INSERT INTO content_entries(id,kind,slug,locale,is_placeholder,data_origin,template_key,created_at)
             SELECT id,'news',$3||'-'||n,'en',true,'editorial','newsDetail',now() FROM generated RETURNING id)
           INSERT INTO cms_drafts(draft_id,content_id,owner_user_id,document,draft_version,base_publication_version,state,created_at,updated_at)
           SELECT id,id,$1,jsonb_set(jsonb_set($2::jsonb,'{title}',to_jsonb($3||'-'||n)),
                  '{slug}',to_jsonb($3||'-'||n)),1,0,'editing',now(),now()
           FROM generated"#,
    ).bind(owner_id).bind(base).bind(&suffix).execute(sandbox.pool()).await.unwrap();
    let principal = cms_workflow_principal(owner_id, email, "editor", &["content.read"]);
    let mut cursor = None;
    let mut seen = std::collections::HashSet::new();
    loop {
        let page = cms_workflow::list_drafts(
            &state,
            &principal,
            CmsListQuery {
                cursor,
                limit: Some(100),
                q: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(page.total, 5000);
        for item in page.items {
            assert!(seen.insert(item.draft_id));
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(seen.len(), 5000);
    state.pool.close().await;
    sandbox.cleanup().await;
}
