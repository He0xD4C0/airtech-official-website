use serde_json::Value;

use super::super::{
    DependencyBlockingIssue, DependencyGate, DependencyIssueCode, ExtractedPublicationDependencies,
};
use super::{
    invalid_target_type, invalid_type, object, required_array, required_object, scan_action,
    scan_link_target, scan_optional_media, scan_seo,
};

const MAX_NAVIGATION_DEPTH: usize = 32;

pub(super) fn scan_type_fields(
    value: Option<&Value>,
    output: &mut ExtractedPublicationDependencies,
) {
    let Some(fields) = required_object(value, "/typeFields", output) else {
        return;
    };
    match fields.get("type").and_then(Value::as_str) {
        Some("article" | "news") => {
            scan_optional_media(fields.get("cover"), "/typeFields/cover", output)
        }
        Some("generalInformation") => {
            if fields
                .get("navigationCta")
                .is_some_and(|action| !action.is_null())
            {
                scan_action(
                    fields.get("navigationCta"),
                    "/typeFields/navigationCta",
                    output,
                );
            }
            if let Some(default_seo) = fields.get("defaultSeo") {
                scan_seo(Some(default_seo), "/typeFields/defaultSeo", output);
            }
        }
        Some("navigation") => {
            scan_navigation_items(fields.get("items"), "/typeFields/items", 0, output)
        }
        Some("footer") => {
            scan_footer_columns(fields.get("columns"), output);
            scan_navigation_items(
                fields.get("legalLinks"),
                "/typeFields/legalLinks",
                0,
                output,
            );
        }
        Some(
            "home" | "page" | "solution" | "technology" | "faq" | "caseStudy" | "download"
            | "company" | "legal",
        ) => {}
        Some(other) => invalid_target_type(
            output,
            "/typeFields/type",
            format!("Unsupported CMS V2 typeFields type {other:?}."),
        ),
        None => invalid_type(
            output,
            "/typeFields/type",
            "typeFields.type must be a string.",
        ),
    }
}

fn scan_footer_columns(value: Option<&Value>, output: &mut ExtractedPublicationDependencies) {
    let Some(columns) = required_array(value, "/typeFields/columns", output) else {
        return;
    };
    for (index, value) in columns.iter().enumerate() {
        let path = format!("/typeFields/columns/{index}");
        let Some(column) = object(value, &path, output) else {
            continue;
        };
        scan_navigation_items(column.get("links"), &format!("{path}/links"), 0, output);
    }
}

fn scan_navigation_items(
    value: Option<&Value>,
    path: &str,
    depth: usize,
    output: &mut ExtractedPublicationDependencies,
) {
    if depth > MAX_NAVIGATION_DEPTH {
        output.blocking_issues.push(DependencyBlockingIssue::new(
            DependencyIssueCode::NavigationDepthExceeded,
            path,
            DependencyGate::DocumentShape,
            None,
            "Navigation nesting exceeds the supported dependency scan depth.",
        ));
        return;
    }
    let Some(items) = required_array(value, path, output) else {
        return;
    };
    for (index, value) in items.iter().enumerate() {
        let item_path = format!("{path}/{index}");
        let Some(item) = object(value, &item_path, output) else {
            continue;
        };
        if item.get("target").is_some_and(|target| !target.is_null()) {
            scan_link_target(item.get("target"), &format!("{item_path}/target"), output);
        }
        scan_navigation_items(
            item.get("children"),
            &format!("{item_path}/children"),
            depth + 1,
            output,
        );
    }
}
