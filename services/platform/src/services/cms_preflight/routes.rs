use std::collections::{BTreeMap, BTreeSet};

use crate::services::cms_templates::template_definition;

use super::{conversion::issue, types::*};

pub(super) fn validate_public_routes(
    snapshot: &LegacySnapshot,
    records: &[CmsPreflightRecord],
    issues: &mut Vec<CmsPreflightIssue>,
) -> usize {
    let mut invalid = BTreeSet::new();
    let content = snapshot
        .content_entries
        .iter()
        .map(|entry| (entry.id, entry))
        .collect::<BTreeMap<_, _>>();
    let products = snapshot
        .products
        .iter()
        .map(|product| (product.id, product))
        .collect::<BTreeMap<_, _>>();
    let candidates = records
        .iter()
        .filter(|record| record.role == CmsPreflightRecordRole::Revision)
        .map(|record| {
            (
                (record.entity_id, record.source_revision),
                &record.candidate,
            )
        })
        .collect::<BTreeMap<_, _>>();

    let mut paths: BTreeMap<&str, Vec<&LegacyPublicRoute>> = BTreeMap::new();
    for route in &snapshot.public_routes {
        paths.entry(&route.canonical_path).or_default().push(route);
        if !safe_path(&route.canonical_path) {
            invalid.insert(route.id);
            push_route(
                issues,
                CmsPreflightIssueCode::CanonicalPathMismatch,
                route,
                "canonicalPath",
                "Public route must be a safe absolute path without query or fragment.",
            );
        }
        match route.entity_type.as_str() {
            "content" if !content.contains_key(&route.entity_id) => {
                invalid.insert(route.id);
                push_route(
                    issues,
                    CmsPreflightIssueCode::RouteOwnershipMismatch,
                    route,
                    "entityId",
                    "Public route references missing content.",
                );
            }
            "product" if !products.contains_key(&route.entity_id) => {
                invalid.insert(route.id);
                push_route(
                    issues,
                    CmsPreflightIssueCode::RouteOwnershipMismatch,
                    route,
                    "entityId",
                    "Public route references a missing product.",
                );
            }
            "content" | "product" => {}
            _ => {
                invalid.insert(route.id);
                push_route(
                    issues,
                    CmsPreflightIssueCode::RouteOwnershipMismatch,
                    route,
                    "entityType",
                    "Public route entity type is not recognized.",
                );
            }
        }
    }
    for routes in paths.values().filter(|routes| routes.len() > 1) {
        for route in routes {
            invalid.insert(route.id);
            push_route(
                issues,
                CmsPreflightIssueCode::DuplicatePublicPath,
                route,
                "canonicalPath",
                "Multiple legacy routes claim the same canonical path.",
            );
        }
    }

    for entry in &snapshot.content_entries {
        let owned = snapshot
            .public_routes
            .iter()
            .filter(|route| route.entity_type == "content" && route.entity_id == entry.id)
            .collect::<Vec<_>>();
        let Some(published_revision) = entry.published_revision else {
            for route in owned {
                invalid.insert(route.id);
                push_route(
                    issues,
                    CmsPreflightIssueCode::RouteOwnershipMismatch,
                    route,
                    "entityId",
                    "Unpublished content must not retain a public route.",
                );
            }
            continue;
        };
        let Some(candidate) = candidates.get(&(entry.id, published_revision)) else {
            for route in owned {
                invalid.insert(route.id);
                push_route(
                    issues,
                    CmsPreflightIssueCode::RouteOwnershipMismatch,
                    route,
                    "entityId",
                    "Public route owner has no convertible immutable published revision.",
                );
            }
            continue;
        };
        let routable = template_definition(candidate.template_key)
            .is_some_and(|definition| definition.routable);
        if !routable {
            for route in owned {
                invalid.insert(route.id);
                push_route(
                    issues,
                    CmsPreflightIssueCode::RouteOwnershipMismatch,
                    route,
                    "entityId",
                    "Non-routable CMS configuration must not own a public route.",
                );
            }
            continue;
        }
        if owned.is_empty() {
            issues.push(issue(
                CmsPreflightSeverity::Blocking,
                CmsPreflightIssueCode::MissingPublicRoute,
                CmsPreflightSource::Route,
                Some(entry.id),
                Some(published_revision),
                "publicRoutes",
                "Published routable content has no canonical public route.",
            ));
            continue;
        }
        if owned.len() > 1 {
            for route in &owned {
                invalid.insert(route.id);
                push_route(
                    issues,
                    CmsPreflightIssueCode::RouteOwnershipMismatch,
                    route,
                    "entityId",
                    "Content owns more than one canonical route.",
                );
            }
        }
        for route in owned {
            if route.locale != candidate.locale {
                invalid.insert(route.id);
                push_route(
                    issues,
                    CmsPreflightIssueCode::RouteOwnershipMismatch,
                    route,
                    "locale",
                    "Content route locale differs from the published revision.",
                );
            }
            if let Some(canonical) = published_canonical(snapshot, entry.id, published_revision) {
                if canonical != route.canonical_path {
                    invalid.insert(route.id);
                    push_route(
                        issues,
                        CmsPreflightIssueCode::CanonicalPathMismatch,
                        route,
                        "canonicalPath",
                        "Public route differs from the published revision canonicalPath.",
                    );
                }
            }
            let expected_indexable = candidate.seo.indexable
                && !candidate.is_placeholder
                && !matches!(
                    candidate.template_key,
                    crate::models::ContentTemplateKey::Compare
                        | crate::models::ContentTemplateKey::Search
                );
            if route.indexable != expected_indexable {
                invalid.insert(route.id);
                push_route(
                    issues,
                    CmsPreflightIssueCode::RouteOwnershipMismatch,
                    route,
                    "indexable",
                    "Route indexing state disagrees with the published CMS V2 candidate.",
                );
            }
        }
    }

    for product in &snapshot.products {
        let owned = snapshot
            .public_routes
            .iter()
            .filter(|route| route.entity_type == "product" && route.entity_id == product.id)
            .collect::<Vec<_>>();
        if product.published_revision.is_none() {
            for route in owned {
                invalid.insert(route.id);
                push_route(
                    issues,
                    CmsPreflightIssueCode::RouteOwnershipMismatch,
                    route,
                    "entityId",
                    "Unpublished product must not retain a public route.",
                );
            }
            continue;
        }
        if !product.has_verified_published_localization {
            for route in owned {
                invalid.insert(route.id);
                push_route(
                    issues,
                    CmsPreflightIssueCode::RouteOwnershipMismatch,
                    route,
                    "entityId",
                    "Product route has no verified localization for the exact published revision and locale.",
                );
            }
            continue;
        }
        if owned.is_empty() {
            issues.push(issue(
                CmsPreflightSeverity::Blocking,
                CmsPreflightIssueCode::MissingPublicRoute,
                CmsPreflightSource::Route,
                Some(product.id),
                product.published_revision,
                "publicRoutes",
                "Published product has no canonical public route.",
            ));
        }
        if owned.len() > 1 {
            for route in &owned {
                invalid.insert(route.id);
                push_route(
                    issues,
                    CmsPreflightIssueCode::RouteOwnershipMismatch,
                    route,
                    "entityId",
                    "Product owns more than one canonical route.",
                );
            }
        }
        for route in owned {
            let ownership_mismatch = route.locale != product.locale
                || route.indexable != (product.indexable && !product.is_placeholder);
            if ownership_mismatch {
                invalid.insert(route.id);
                push_route(
                    issues,
                    CmsPreflightIssueCode::RouteOwnershipMismatch,
                    route,
                    "publicRoutes",
                    "Product route locale or indexing state disagrees with its owner.",
                );
            }
            if product.canonical_path.as_deref() != Some(route.canonical_path.as_str()) {
                invalid.insert(route.id);
                push_route(
                    issues,
                    CmsPreflightIssueCode::CanonicalPathMismatch,
                    route,
                    "canonicalPath",
                    "Product route differs from its verified published localization canonicalPath.",
                );
            }
        }
    }

    snapshot.public_routes.len() - invalid.len()
}

fn published_canonical(
    snapshot: &LegacySnapshot,
    content_id: uuid::Uuid,
    revision: i64,
) -> Option<&str> {
    snapshot
        .content_revisions
        .iter()
        .find(|value| value.content_id == content_id && value.revision == revision)
        .and_then(|value| value.payload.pointer("/seo/canonicalPath"))
        .and_then(serde_json::Value::as_str)
}

fn safe_path(path: &str) -> bool {
    path.starts_with('/')
        && !path.starts_with("//")
        && !path.contains(['?', '#', '\\', '\r', '\n', '\0'])
        && !path.chars().any(char::is_whitespace)
}

fn push_route(
    issues: &mut Vec<CmsPreflightIssue>,
    code: CmsPreflightIssueCode,
    route: &LegacyPublicRoute,
    path: &str,
    message: &str,
) {
    issues.push(issue(
        CmsPreflightSeverity::Blocking,
        code,
        CmsPreflightSource::Route,
        Some(route.entity_id),
        None,
        path,
        message,
    ));
}
