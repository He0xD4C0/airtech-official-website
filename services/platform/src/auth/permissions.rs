fn super_admin_permissions() -> Vec<String> {
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
        "operations.run",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<Vec<_>>();
    #[cfg(feature = "devtools")]
    values.push("devtools.shell".into());
    values
}

pub fn required_permission(path: &str, method: &axum::http::Method) -> Option<&'static str> {
    let write = !matches!(
        *method,
        axum::http::Method::GET | axum::http::Method::HEAD | axum::http::Method::OPTIONS
    );
    if (path.contains("/content/")
        || path.contains("/news/")
        || path.contains("/general-information/"))
        && (path.ends_with("/publish") || path.ends_with("/rollback"))
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
    } else if path.contains("/products/") && path.ends_with("/publish") {
        Some("product.publish")
    } else if path.ends_with("/products") || path.contains("/products/") {
        Some(if write {
            "product.write"
        } else {
            "product.read"
        })
    } else if path.contains("/feishu/") {
        Some("integration.run")
    } else if path.contains("/rfqs") || path.contains("/contacts") {
        Some("rfq.read")
    } else if path.contains("/analytics") {
        Some("analytics.read")
    } else if path.ends_with("/users")
        || path.contains("/users/")
        || path.ends_with("/roles")
        || path.contains("/roles/")
        || path.contains("/user-invitations")
    {
        Some("identity.manage")
    } else if path.ends_with("/settings") {
        Some("settings.manage")
    } else if path.contains("/operations") {
        Some("operations.run")
    } else if path.contains("/audit") {
        Some("audit.read")
    } else {
        None
    }
}
