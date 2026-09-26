use serde_json::{Map, Value};
use uuid::Uuid;

use airtek_domain::models::CMS_V2_SCHEMA_VERSION;

use super::{
    sort_issues, DependencyBlockingIssue, DependencyGate, DependencyIssueCode, ExtractedDependency,
    ExtractedDependencyTarget, ExtractedPublicationDependencies, PublicationDependencyKind,
    PublicationLockTarget, PublicationLockTargetKind,
};

#[path = "extraction/fields.rs"]
mod fields;

pub(super) fn extract(document: &Value) -> ExtractedPublicationDependencies {
    let mut output = ExtractedPublicationDependencies {
        source_locale: None,
        references: Vec::new(),
        blocking_issues: Vec::new(),
        lock_targets: Vec::new(),
    };
    let Some(root) = document.as_object() else {
        invalid_type(&mut output, "", "CMS V2 document must be an object.");
        return output;
    };
    if root.get("schemaVersion").and_then(Value::as_u64) != Some(u64::from(CMS_V2_SCHEMA_VERSION)) {
        output.blocking_issues.push(DependencyBlockingIssue::new(
            DependencyIssueCode::InvalidSchemaVersion,
            "/schemaVersion",
            DependencyGate::DocumentShape,
            None,
            "schemaVersion must be the integer 2.",
        ));
    }
    output.source_locale = match root.get("locale").and_then(Value::as_str) {
        Some(locale) if !locale.trim().is_empty() => Some(locale.to_owned()),
        _ => {
            invalid_type(&mut output, "/locale", "locale must be a non-empty string.");
            None
        }
    };

    scan_relations(root.get("relations"), &mut output);
    scan_composition(root.get("composition"), &mut output);
    scan_seo(root.get("seo"), "/seo", &mut output);
    fields::scan_type_fields(root.get("typeFields"), &mut output);

    output.references.sort_by(|left, right| {
        (&left.reference_path, left.kind).cmp(&(&right.reference_path, right.kind))
    });
    output.lock_targets.sort_unstable();
    output.lock_targets.dedup();
    sort_issues(&mut output.blocking_issues);
    output
}

fn scan_relations(value: Option<&Value>, output: &mut ExtractedPublicationDependencies) {
    let Some(relations) = required_array(value, "/relations", output) else {
        return;
    };
    for (index, value) in relations.iter().enumerate() {
        let path = format!("/relations/{index}");
        let Some(relation) = object(value, &path, output) else {
            continue;
        };
        let _ = uuid(relation.get("id"), &format!("{path}/id"), output);
        let target_path = format!("{path}/target");
        let Some(target) = required_object(relation.get("target"), &target_path, output) else {
            continue;
        };
        match target.get("targetType").and_then(Value::as_str) {
            Some("content") => {
                let id_path = format!("{target_path}/contentId");
                if let Some(id) = uuid(target.get("contentId"), &id_path, output) {
                    push_content(
                        output,
                        id_path,
                        PublicationDependencyKind::RelationContent,
                        id,
                    );
                }
            }
            Some("product") => {
                let id_path = format!("{target_path}/productId");
                if let Some(id) = uuid(target.get("productId"), &id_path, output) {
                    output.references.push(ExtractedDependency {
                        reference_path: id_path,
                        kind: PublicationDependencyKind::RelationProduct,
                        target: ExtractedDependencyTarget::Product(id),
                    });
                    push_lock(output, PublicationLockTargetKind::Product, id);
                }
            }
            Some(other) => invalid_target_type(
                output,
                &format!("{target_path}/targetType"),
                format!("Unsupported relation targetType {other:?}."),
            ),
            None => invalid_type(
                output,
                &format!("{target_path}/targetType"),
                "Relation targetType must be a string.",
            ),
        }
    }
}

fn scan_composition(value: Option<&Value>, output: &mut ExtractedPublicationDependencies) {
    let Some(composition) = required_object(value, "/composition", output) else {
        return;
    };
    let Some(blocks) = required_array(composition.get("blocks"), "/composition/blocks", output)
    else {
        return;
    };
    for (index, value) in blocks.iter().enumerate() {
        let path = format!("/composition/blocks/{index}");
        let Some(block) = object(value, &path, output) else {
            continue;
        };
        match block.get("type").and_then(Value::as_str) {
            Some("hero") => {
                scan_optional_media(block.get("media"), &format!("{path}/media"), output);
                scan_actions(block.get("actions"), &format!("{path}/actions"), output);
            }
            Some("media") => scan_media_use(block.get("media"), &format!("{path}/media"), output),
            Some("featureGrid") => scan_feature_items(block.get("items"), &path, output),
            Some("cta") => scan_action(block.get("action"), &format!("{path}/action"), output),
            Some("downloadAsset") => scan_asset(
                block.get("asset"),
                &format!("{path}/asset"),
                PublicationDependencyKind::MediaDownload,
                output,
            ),
            Some("contactBlock") => {
                if block.get("action").is_some_and(|action| !action.is_null()) {
                    scan_action(block.get("action"), &format!("{path}/action"), output);
                }
            }
            Some("body" | "evidence" | "relationCollection" | "faqCollection") => {}
            Some(other) => invalid_target_type(
                output,
                &format!("{path}/type"),
                format!("Unsupported CMS V2 block type {other:?}."),
            ),
            None => invalid_type(
                output,
                &format!("{path}/type"),
                "Block type must be a string.",
            ),
        }
    }
}

fn scan_feature_items(
    value: Option<&Value>,
    block_path: &str,
    output: &mut ExtractedPublicationDependencies,
) {
    let path = format!("{block_path}/items");
    let Some(items) = required_array(value, &path, output) else {
        return;
    };
    for (index, value) in items.iter().enumerate() {
        let item_path = format!("{path}/{index}");
        let Some(item) = object(value, &item_path, output) else {
            continue;
        };
        scan_optional_media(item.get("icon"), &format!("{item_path}/icon"), output);
    }
}

fn scan_actions(value: Option<&Value>, path: &str, output: &mut ExtractedPublicationDependencies) {
    let Some(actions) = required_array(value, path, output) else {
        return;
    };
    for (index, action) in actions.iter().enumerate() {
        scan_action(Some(action), &format!("{path}/{index}"), output);
    }
}

pub(super) fn scan_action(
    value: Option<&Value>,
    path: &str,
    output: &mut ExtractedPublicationDependencies,
) {
    let Some(action) = required_object(value, path, output) else {
        return;
    };
    scan_link_target(action.get("target"), &format!("{path}/target"), output);
}

pub(super) fn scan_link_target(
    value: Option<&Value>,
    path: &str,
    output: &mut ExtractedPublicationDependencies,
) {
    let Some(target) = required_object(value, path, output) else {
        return;
    };
    match target.get("targetType").and_then(Value::as_str) {
        Some("content") => {
            let id_path = format!("{path}/contentId");
            if let Some(id) = uuid(target.get("contentId"), &id_path, output) {
                push_content(output, id_path, PublicationDependencyKind::ContentLink, id);
            }
        }
        Some("route" | "external") => {}
        Some(other) => invalid_target_type(
            output,
            &format!("{path}/targetType"),
            format!("Unsupported link targetType {other:?}."),
        ),
        None => invalid_type(
            output,
            &format!("{path}/targetType"),
            "Link targetType must be a string.",
        ),
    }
}

pub(super) fn scan_seo(
    value: Option<&Value>,
    path: &str,
    output: &mut ExtractedPublicationDependencies,
) {
    let Some(seo) = required_object(value, path, output) else {
        return;
    };
    scan_optional_media(
        seo.get("socialImage"),
        &format!("{path}/socialImage"),
        output,
    );
}

pub(super) fn scan_optional_media(
    value: Option<&Value>,
    path: &str,
    output: &mut ExtractedPublicationDependencies,
) {
    if value.is_some_and(|media| !media.is_null()) {
        scan_media_use(value, path, output);
    }
}

fn scan_media_use(
    value: Option<&Value>,
    path: &str,
    output: &mut ExtractedPublicationDependencies,
) {
    let Some(media) = required_object(value, path, output) else {
        return;
    };
    scan_asset(
        media.get("asset"),
        &format!("{path}/asset"),
        PublicationDependencyKind::MediaInline,
        output,
    );
}

pub(super) fn scan_optional_asset(
    value: Option<&Value>,
    path: &str,
    kind: PublicationDependencyKind,
    output: &mut ExtractedPublicationDependencies,
) {
    if value.is_some_and(|asset| !asset.is_null()) {
        scan_asset(value, path, kind, output);
    }
}

fn scan_asset(
    value: Option<&Value>,
    path: &str,
    kind: PublicationDependencyKind,
    output: &mut ExtractedPublicationDependencies,
) {
    let Some(asset) = required_object(value, path, output) else {
        return;
    };
    let asset_path = format!("{path}/assetId");
    let asset_id = uuid(asset.get("assetId"), &asset_path, output);
    if let Some(asset_id) = asset_id {
        push_lock(output, PublicationLockTargetKind::Media, asset_id);
        output.references.push(ExtractedDependency {
            reference_path: asset_path,
            kind,
            target: ExtractedDependencyTarget::Media(asset_id),
        });
    }
}

fn push_content(
    output: &mut ExtractedPublicationDependencies,
    path: String,
    kind: PublicationDependencyKind,
    id: Uuid,
) {
    output.references.push(ExtractedDependency {
        reference_path: path,
        kind,
        target: ExtractedDependencyTarget::Content(id),
    });
    push_lock(output, PublicationLockTargetKind::Content, id);
}

fn push_lock(
    output: &mut ExtractedPublicationDependencies,
    kind: PublicationLockTargetKind,
    id: Uuid,
) {
    output.lock_targets.push(PublicationLockTarget { kind, id });
}

fn uuid(
    value: Option<&Value>,
    path: &str,
    output: &mut ExtractedPublicationDependencies,
) -> Option<Uuid> {
    let Some(value) = value else {
        invalid_type(output, path, "UUID field is required.");
        return None;
    };
    let Some(value) = value.as_str() else {
        invalid_type(output, path, "UUID field must be a string.");
        return None;
    };
    match Uuid::parse_str(value) {
        Ok(value) => Some(value),
        Err(_) => {
            output.blocking_issues.push(DependencyBlockingIssue::new(
                DependencyIssueCode::InvalidUuid,
                path,
                DependencyGate::TargetIdentity,
                None,
                "UUID field is malformed.",
            ));
            None
        }
    }
}

pub(super) fn required_array<'a>(
    value: Option<&'a Value>,
    path: &str,
    output: &mut ExtractedPublicationDependencies,
) -> Option<&'a Vec<Value>> {
    match value.and_then(Value::as_array) {
        Some(value) => Some(value),
        None => {
            invalid_type(output, path, "Dependency container must be an array.");
            None
        }
    }
}

pub(super) fn required_object<'a>(
    value: Option<&'a Value>,
    path: &str,
    output: &mut ExtractedPublicationDependencies,
) -> Option<&'a Map<String, Value>> {
    match value.and_then(Value::as_object) {
        Some(value) => Some(value),
        None => {
            invalid_type(output, path, "Dependency container must be an object.");
            None
        }
    }
}

pub(super) fn object<'a>(
    value: &'a Value,
    path: &str,
    output: &mut ExtractedPublicationDependencies,
) -> Option<&'a Map<String, Value>> {
    required_object(Some(value), path, output)
}

pub(super) fn invalid_type(
    output: &mut ExtractedPublicationDependencies,
    path: &str,
    detail: impl Into<String>,
) {
    output.blocking_issues.push(DependencyBlockingIssue::new(
        DependencyIssueCode::InvalidType,
        path,
        DependencyGate::DocumentShape,
        None,
        detail,
    ));
}

pub(super) fn invalid_target_type(
    output: &mut ExtractedPublicationDependencies,
    path: &str,
    detail: impl Into<String>,
) {
    output.blocking_issues.push(DependencyBlockingIssue::new(
        DependencyIssueCode::InvalidTargetType,
        path,
        DependencyGate::TargetIdentity,
        None,
        detail,
    ));
}
