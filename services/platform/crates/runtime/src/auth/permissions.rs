use super::*;

pub(super) fn super_admin_permissions() -> Vec<String> {
    #[allow(unused_mut)]
    let mut values = [
        "dashboard.read",
        "content.read",
        "content.write",
        "content.publish",
        "product.read",
        "product.pricing.read",
        "product.write",
        "product.publish",
        "integration.run",
        "media.write",
        "rfq.read",
        "rfq.read_pii",
        "rfq.assign",
        "analytics.read",
        "identity.manage",
        "audit.read",
        "settings.manage",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<Vec<_>>();
    #[cfg(feature = "devtools")]
    values.push("devtools.shell".into());
    values
}

pub(super) const MEDIA_ASSET_LIST_PERMISSIONS: &[&str] = &["content.read", "media.write"];
pub(super) const CONTENT_DRAFT_READ_PERMISSIONS: &[&str] = &["content.read", "content.publish"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdminPermissionPolicy {
    Authenticated,
    Exact(&'static str),
    Any(&'static [&'static str]),
}

impl AdminPermissionPolicy {
    pub fn allows(self, principal: &AdminPrincipal) -> bool {
        match self {
            Self::Authenticated => true,
            Self::Exact(permission) => principal.has_permission(permission),
            Self::Any(permissions) => permissions
                .iter()
                .any(|permission| principal.has_permission(permission)),
        }
    }

    pub fn denied_detail(self) -> String {
        match self {
            Self::Authenticated => "An authenticated Admin session is required.".into(),
            Self::Exact(permission) => format!("The `{permission}` permission is required."),
            Self::Any(_) => "One of the listed permissions is required.".into(),
        }
    }
}

pub fn permission_policy(path: &str, method: &axum::http::Method) -> Option<AdminPermissionPolicy> {
    if path.ends_with("/dashboard/summary") && *method == axum::http::Method::GET {
        Some(AdminPermissionPolicy::Authenticated)
    } else if path.ends_with("/media/assets") && *method == axum::http::Method::GET {
        Some(AdminPermissionPolicy::Any(MEDIA_ASSET_LIST_PERMISSIONS))
    } else if path.contains("/content-drafts") && *method == axum::http::Method::GET {
        Some(AdminPermissionPolicy::Any(CONTENT_DRAFT_READ_PERMISSIONS))
    } else {
        required_permission(path, method).map(AdminPermissionPolicy::Exact)
    }
}

pub fn required_permission(path: &str, method: &axum::http::Method) -> Option<&'static str> {
    let write = !matches!(
        *method,
        axum::http::Method::GET | axum::http::Method::HEAD | axum::http::Method::OPTIONS
    );
    if path.ends_with("/media/assets") && !write {
        // `permission_policy` handles the read-only union before reaching this
        // exact-permission mapper.
        None
    } else if path.contains("/media/") {
        Some(if write { "media.write" } else { "content.read" })
    } else if path.contains("/content-reviews") {
        Some("content.publish")
    } else if path.contains("/site-singletons/") {
        Some("content.read")
    } else if path.contains("/published-content") {
        Some(if write && path.ends_with("/drafts") {
            "content.write"
        } else {
            "content.read"
        })
    } else if path.contains("/content-drafts") {
        Some(if write {
            "content.write"
        } else {
            "content.read"
        })
    } else if (path.contains("/content/")
        || path.contains("/news/")
        || path.contains("/general-information/"))
        && (path.ends_with("/publish")
            || path.ends_with("/unpublish")
            || path.ends_with("/rollback"))
    {
        Some("content.publish")
    } else if path.ends_with("/content")
        || path.contains("/content/")
        || path.ends_with("/news")
        || path.contains("/news/")
        || path.contains("/general-information")
    {
        Some(if write {
            "content.write"
        } else {
            "content.read"
        })
    } else if path.contains("/products/") && path.ends_with("/private-pricing") {
        Some("product.pricing.read")
    } else if path.ends_with("/source-metadata") {
        Some("product.read")
    } else if path.contains("/products/") && path.ends_with("/publish") {
        Some("product.publish")
    } else if path.ends_with("/products") || path.contains("/products/") {
        Some(if write {
            "product.write"
        } else {
            "product.read"
        })
    } else if path.contains("/feishu/settings") {
        Some(if write {
            "settings.manage"
        } else {
            "integration.run"
        })
    } else if path.contains("/feishu/") {
        Some("integration.run")
    } else if (path.contains("/rfqs/") || path.contains("/contacts/")) && path.ends_with("/pii") {
        Some("rfq.read_pii")
    } else if path.contains("/rfqs") || path.contains("/contacts") {
        Some(if write { "rfq.assign" } else { "rfq.read" })
    } else if path.contains("/analytics") {
        Some("analytics.read")
    } else if path.ends_with("/users")
        || path.contains("/users/")
        || path.ends_with("/roles")
        || path.contains("/roles/")
        || path.contains("/user-invitations")
    {
        Some("identity.manage")
    } else if path.ends_with("/settings") || path.contains("/settings/") {
        Some("settings.manage")
    } else if path.contains("/operations/") {
        Some("product.write")
    } else if path.contains("/audit") {
        Some("audit.read")
    } else {
        None
    }
}
