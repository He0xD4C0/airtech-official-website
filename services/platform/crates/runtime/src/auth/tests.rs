#[cfg(test)]
mod cases {
    use super::super::*;
    use std::collections::HashSet;

    fn principal(totp_enabled: bool, development_password_only: bool) -> AdminPrincipal {
        AdminPrincipal {
            user_id: Uuid::nil(),
            display_name: "Local administrator".into(),
            email: crate::config::DEVELOPMENT_ADMIN_EMAIL.into(),
            role: "Super Admin".into(),
            permissions: vec!["content.write".into()],
            session_id: Uuid::nil(),
            session_token_hash: Vec::new(),
            csrf_hash: Vec::new(),
            totp_enabled,
            development_password_only,
        }
    }

    #[test]
    pub(super) fn local_password_only_session_can_use_normal_permissions() {
        assert!(principal(false, true).has_permission("content.write"));
        assert!(!principal(false, false).has_permission("content.write"));
        assert!(principal(true, false).has_permission("content.write"));
    }

    #[test]
    pub(super) fn password_hashes_use_argon2id() {
        let hash = hash_password("a-long-password-123").unwrap();
        assert!(hash.starts_with("$argon2id$"));
        assert!(verify_password(&hash, "a-long-password-123"));
        assert!(!verify_password(&hash, "incorrect-password"));
    }

    #[test]
    pub(super) fn invitation_password_policy_is_bounded_and_requires_letters_and_digits() {
        assert!(validate_strong_password("correct-horse-123").is_ok());
        assert!(validate_strong_password("short-1").is_err());
        assert!(validate_strong_password("onlylettersforever").is_err());
        assert!(validate_strong_password("1234567890123456").is_err());
        assert!(validate_strong_password(&format!("A1{}", "x".repeat(255))).is_err());
    }

    #[test]
    pub(super) fn permission_mapping_distinguishes_edit_and_publish() {
        assert_eq!(
            required_permission(
                "/api/admin/v1/content-drafts/abc",
                &axum::http::Method::PATCH
            ),
            Some("content.write")
        );
        assert_eq!(
            required_permission(
                "/api/admin/v1/content-drafts/abc/submit",
                &axum::http::Method::POST
            ),
            Some("content.write")
        );
        assert_eq!(
            required_permission(
                "/api/admin/v1/content-reviews/abc/approve",
                &axum::http::Method::POST
            ),
            Some("content.publish")
        );
        assert_eq!(
            permission_policy("/api/admin/v1/content-drafts/abc", &axum::http::Method::GET),
            Some(AdminPermissionPolicy::Any(CONTENT_DRAFT_READ_PERMISSIONS))
        );
        assert_eq!(
            required_permission("/api/admin/v1/settings", &axum::http::Method::GET),
            Some("settings.manage")
        );
        assert_eq!(
            required_permission("/api/admin/v1/settings", &axum::http::Method::PATCH),
            Some("settings.manage")
        );
        assert_eq!(
            required_permission(
                "/api/admin/v1/settings/object-storage/test",
                &axum::http::Method::POST
            ),
            Some("settings.manage")
        );
        assert_eq!(
            required_permission(
                "/api/admin/v1/roles/00000000-0000-0000-0000-000000000001",
                &axum::http::Method::PATCH
            ),
            Some("identity.manage")
        );
        assert_eq!(
            required_permission(
                "/api/admin/v1/products/00000000-0000-0000-0000-000000000001/private-pricing",
                &axum::http::Method::GET
            ),
            Some("product.pricing.read")
        );
        assert_eq!(
            required_permission("/api/admin/v1/analytics/overview", &axum::http::Method::GET),
            Some("analytics.read")
        );
        assert_eq!(
            permission_policy("/api/admin/v1/dashboard/summary", &axum::http::Method::GET),
            Some(AdminPermissionPolicy::Authenticated)
        );
        assert_eq!(
            required_permission(
                "/api/admin/v1/media/assets/00000000-0000-0000-0000-000000000001/references",
                &axum::http::Method::GET
            ),
            Some("content.read")
        );
        assert_eq!(
            required_permission("/api/admin/v1/media/assets", &axum::http::Method::GET),
            None
        );
        assert_eq!(
            permission_policy("/api/admin/v1/media/assets", &axum::http::Method::GET),
            Some(AdminPermissionPolicy::Any(&["content.read", "media.write"]))
        );
        assert_eq!(
            permission_policy("/api/admin/v1/media/assets", &axum::http::Method::POST),
            Some(AdminPermissionPolicy::Exact("media.write"))
        );
        assert_eq!(
            required_permission(
                "/api/admin/v1/rfqs/00000000-0000-0000-0000-000000000001/pii",
                &axum::http::Method::GET
            ),
            Some("rfq.read_pii")
        );
        assert_eq!(
            required_permission(
                "/api/admin/v1/rfqs/00000000-0000-0000-0000-000000000001/status",
                &axum::http::Method::POST
            ),
            Some("rfq.assign")
        );
        assert_eq!(
            required_permission("/api/admin/v1/feishu/settings", &axum::http::Method::GET),
            Some("integration.run")
        );
        assert_eq!(
            required_permission("/api/admin/v1/feishu/settings", &axum::http::Method::PUT),
            Some("settings.manage")
        );
    }

    #[test]
    pub(super) fn permissions_are_unique() {
        let values = super_admin_permissions();
        let set = values.iter().collect::<HashSet<_>>();
        assert_eq!(set.len(), values.len());
    }

    #[tokio::test]
    pub(super) async fn https_cookie_is_secure_strict_and_host_only() {
        let mut config = crate::Config::for_test();
        config.admin_origin = "https://admin.example.com".into();
        config.database_url = Some("postgresql://localhost/unused".into());
        let state = AppState::new(config).unwrap();
        let session = session_cookie(&state, "token", 60);
        let csrf = csrf_cookie(&state, "csrf", 60);
        assert!(session.contains("HttpOnly"));
        assert!(session.contains("SameSite=Strict"));
        assert!(session.contains("Secure"));
        assert!(csrf.contains("SameSite=Strict"));
        assert!(csrf.contains("Secure"));
        assert!(!session.contains("Domain="));
        assert!(!csrf.contains("Domain="));
    }

    #[tokio::test]
    pub(super) async fn http_acceptance_cookie_remains_browser_storable() {
        let mut config = crate::Config::for_test();
        config.admin_origin = "http://admin.10.211.55.33.sslip.io:8088".into();
        config.database_url = Some("postgresql://localhost/unused".into());
        let state = AppState::new(config).unwrap();
        assert!(!session_cookie(&state, "token", 60).contains("Secure"));
        assert!(!csrf_cookie(&state, "csrf", 60).contains("Secure"));
    }
}
