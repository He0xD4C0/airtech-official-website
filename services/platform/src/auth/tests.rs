#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn password_hashes_use_argon2id() {
        let hash = hash_password("a-long-password-123").unwrap();
        assert!(hash.starts_with("$argon2id$"));
        assert!(verify_password(&hash, "a-long-password-123"));
        assert!(!verify_password(&hash, "incorrect-password"));
    }

    #[test]
    fn invitation_password_policy_is_bounded_and_requires_letters_and_digits() {
        assert!(validate_strong_password("correct-horse-123").is_ok());
        assert!(validate_strong_password("short-1").is_err());
        assert!(validate_strong_password("onlylettersforever").is_err());
        assert!(validate_strong_password("1234567890123456").is_err());
        assert!(validate_strong_password(&format!("A1{}", "x".repeat(255))).is_err());
    }

    #[test]
    fn permission_mapping_distinguishes_edit_and_publish() {
        assert_eq!(
            required_permission("/api/admin/v1/content/abc", &axum::http::Method::PATCH),
            Some("content.write")
        );
        assert_eq!(
            required_permission(
                "/api/admin/v1/content/abc/publish",
                &axum::http::Method::POST
            ),
            Some("content.publish")
        );
        assert_eq!(
            required_permission(
                "/api/admin/v1/content/abc/preview",
                &axum::http::Method::POST
            ),
            Some("content.write")
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
    }

    #[test]
    fn permissions_are_unique() {
        let values = super_admin_permissions();
        let set = values.iter().collect::<HashSet<_>>();
        assert_eq!(set.len(), values.len());
    }

    #[test]
    fn https_cookie_is_secure_strict_and_host_only() {
        let mut config = crate::Config::for_test();
        config.admin_origin = "https://admin.example.com".into();
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
}
