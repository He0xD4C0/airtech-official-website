use serde_json::{Map, Value};
use uuid::Uuid;

use crate::models::{
    EditorialAction, GeneralInformationTypeFields, ProductCategoryPresentationInput, SeoInput,
};

use super::{
    conversion::issue,
    general_information_contact::{contact_information, social_links},
    support::link_target,
    types::{CmsPreflightIssue, CmsPreflightIssueCode, CmsPreflightSeverity, CmsPreflightSource},
};

#[derive(Clone, Copy)]
pub(super) struct Context {
    pub(super) id: Uuid,
    pub(super) revision: i64,
}

pub(super) fn convert_fields(
    payload: &Map<String, Value>,
    placeholder: bool,
    id: Uuid,
    revision: i64,
    issues: &mut Vec<CmsPreflightIssue>,
) -> GeneralInformationTypeFields {
    let context = Context { id, revision };
    check_keys(
        payload,
        &[
            "brandName",
            "brandLine",
            "homePath",
            "footerStatement",
            "copyrightText",
            "defaultSeo",
            "organization",
            "navigationCta",
            "productCategories",
        ],
        "payload",
        context,
        issues,
    );
    check_text_fields(
        payload,
        &[
            "brandName",
            "brandLine",
            "homePath",
            "footerStatement",
            "copyrightText",
        ],
        "payload",
        context,
        issues,
    );
    if let Some(path) = text(payload.get("homePath")) {
        if !matches!(
            link_target(&path),
            Some(crate::models::LinkTargetReference::Route { .. })
        ) {
            push(
                issues,
                CmsPreflightSeverity::Blocking,
                CmsPreflightIssueCode::TypeFieldMismatch,
                context,
                "payload.homePath",
                "homePath must be a safe absolute local route.",
            );
        }
    }
    let organization = object_field(
        payload.get("organization"),
        "payload.organization",
        context,
        issues,
    );
    if let Some(organization) = organization {
        check_keys(
            organization,
            &[
                "name",
                "url",
                "logoUrl",
                "legalName",
                "salesEmail",
                "marketingEmail",
                "address",
                "socialLinks",
            ],
            "payload.organization",
            context,
            issues,
        );
        check_text_fields(
            organization,
            &[
                "name",
                "url",
                "logoUrl",
                "legalName",
                "salesEmail",
                "marketingEmail",
                "address",
            ],
            "payload.organization",
            context,
            issues,
        );
        for field in ["url", "logoUrl", "legalName"] {
            if organization
                .get(field)
                .is_some_and(|value| !value.is_null())
            {
                push(
                    issues,
                    CmsPreflightSeverity::Blocking,
                    CmsPreflightIssueCode::TypeFieldMismatch,
                    context,
                    &format!("payload.organization.{field}"),
                    "Legacy organization field has no lossless CMS V2 destination.",
                );
            }
        }
    }
    let brand_name = text(payload.get("brandName"));
    let organization_name = organization.and_then(|value| text(value.get("name")));
    if brand_name.is_some() && organization_name.is_some() && brand_name != organization_name {
        push(
            issues,
            CmsPreflightSeverity::Blocking,
            CmsPreflightIssueCode::TypeFieldMismatch,
            context,
            "payload.brandName",
            "brandName and organization.name conflict; precedence cannot be guessed.",
        );
    }
    GeneralInformationTypeFields {
        organization_name: brand_name.or(organization_name),
        brand_line: text(payload.get("brandLine")),
        home_path: text(payload.get("homePath")),
        footer_statement: text(payload.get("footerStatement")),
        copyright_template: text(payload.get("copyrightText")),
        contact: contact_information(organization, context, issues),
        social_links: social_links(organization, context, issues),
        default_seo: default_seo(payload.get("defaultSeo"), placeholder, context, issues),
        product_categories: product_categories(payload.get("productCategories"), context, issues),
        navigation_cta: navigation_cta(payload.get("navigationCta"), context, issues),
    }
}

fn default_seo(
    value: Option<&Value>,
    placeholder: bool,
    context: Context,
    issues: &mut Vec<CmsPreflightIssue>,
) -> SeoInput {
    let object = object_field(value, "payload.defaultSeo", context, issues);
    if let Some(object) = object {
        check_keys(
            object,
            &["title", "description", "indexable"],
            "payload.defaultSeo",
            context,
            issues,
        );
        check_text_fields(
            object,
            &["title", "description"],
            "payload.defaultSeo",
            context,
            issues,
        );
        if object
            .get("indexable")
            .is_some_and(|value| !value.is_boolean())
        {
            push(
                issues,
                CmsPreflightSeverity::Blocking,
                CmsPreflightIssueCode::TypeFieldMismatch,
                context,
                "payload.defaultSeo.indexable",
                "defaultSeo.indexable must be a boolean.",
            );
        }
        if !object.contains_key("indexable") {
            missing_indexable(context, issues);
        }
        if placeholder && object.get("indexable").and_then(Value::as_bool) == Some(true) {
            push(
                issues,
                CmsPreflightSeverity::Warning,
                CmsPreflightIssueCode::TypeFieldMismatch,
                context,
                "payload.defaultSeo.indexable",
                "Placeholder General Information is hard-normalized to noindex in CMS V2.",
            );
        }
    } else if value.is_none() {
        missing_indexable(context, issues);
    }
    SeoInput {
        title: object.and_then(|value| text(value.get("title"))),
        description: object.and_then(|value| text(value.get("description"))),
        indexable: object
            .and_then(|value| value.get("indexable"))
            .and_then(Value::as_bool)
            .unwrap_or(false)
            && !placeholder,
        social_image: None,
    }
}

fn missing_indexable(context: Context, issues: &mut Vec<CmsPreflightIssue>) {
    push(
        issues,
        CmsPreflightSeverity::Warning,
        CmsPreflightIssueCode::InvalidLegacyPayload,
        context,
        "payload.defaultSeo.indexable",
        "Legacy General Information had no indexable flag; CMS V2 defaults it to false.",
    );
}

fn navigation_cta(
    value: Option<&Value>,
    context: Context,
    issues: &mut Vec<CmsPreflightIssue>,
) -> Option<EditorialAction> {
    let value = value?;
    let Some(object) = value.as_object() else {
        push(
            issues,
            CmsPreflightSeverity::Blocking,
            CmsPreflightIssueCode::TypeFieldMismatch,
            context,
            "payload.navigationCta",
            "navigationCta must be an object.",
        );
        return None;
    };
    check_keys(
        object,
        &["label", "href"],
        "payload.navigationCta",
        context,
        issues,
    );
    let (Some(label), Some(href)) = (text(object.get("label")), text(object.get("href"))) else {
        push(
            issues,
            CmsPreflightSeverity::Blocking,
            CmsPreflightIssueCode::TypeFieldMismatch,
            context,
            "payload.navigationCta",
            "Navigation CTA requires string label and href.",
        );
        return None;
    };
    let Some(target) = link_target(&href) else {
        push(
            issues,
            CmsPreflightSeverity::Blocking,
            CmsPreflightIssueCode::MissingRelationTarget,
            context,
            "payload.navigationCta.href",
            "Navigation CTA target is unsafe or unsupported.",
        );
        return None;
    };
    Some(EditorialAction { label, target })
}

fn product_categories(
    value: Option<&Value>,
    context: Context,
    issues: &mut Vec<CmsPreflightIssue>,
) -> Vec<ProductCategoryPresentationInput> {
    let Some(value) = value else {
        return Vec::new();
    };
    let Some(values) = value.as_array() else {
        push(
            issues,
            CmsPreflightSeverity::Blocking,
            CmsPreflightIssueCode::TypeFieldMismatch,
            context,
            "payload.productCategories",
            "productCategories must be an array.",
        );
        return Vec::new();
    };
    values
        .iter()
        .enumerate()
        .filter_map(|(index, value)| {
            match serde_json::from_value::<ProductCategoryPresentationInput>(value.clone()) {
                Ok(value) => Some(value),
                Err(_) => {
                    push(
                        issues,
                        CmsPreflightSeverity::Blocking,
                        CmsPreflightIssueCode::TypeFieldMismatch,
                        context,
                        &format!("payload.productCategories.{index}"),
                        "Product category does not satisfy the strict V2 presentation contract.",
                    );
                    None
                }
            }
        })
        .collect()
}

fn object_field<'a>(
    value: Option<&'a Value>,
    path: &str,
    context: Context,
    issues: &mut Vec<CmsPreflightIssue>,
) -> Option<&'a Map<String, Value>> {
    match value {
        None => None,
        Some(Value::Object(value)) => Some(value),
        Some(_) => {
            push(
                issues,
                CmsPreflightSeverity::Blocking,
                CmsPreflightIssueCode::TypeFieldMismatch,
                context,
                path,
                "Field must be an object.",
            );
            None
        }
    }
}

fn check_text_fields(
    object: &Map<String, Value>,
    fields: &[&str],
    path: &str,
    context: Context,
    issues: &mut Vec<CmsPreflightIssue>,
) {
    for field in fields {
        if object
            .get(*field)
            .is_some_and(|value| !value.is_null() && !value.is_string())
        {
            push(
                issues,
                CmsPreflightSeverity::Blocking,
                CmsPreflightIssueCode::TypeFieldMismatch,
                context,
                &format!("{path}.{field}"),
                "Field must be a string or null.",
            );
        }
    }
}

pub(super) fn check_keys(
    object: &Map<String, Value>,
    allowed: &[&str],
    path: &str,
    context: Context,
    issues: &mut Vec<CmsPreflightIssue>,
) {
    for key in object.keys().filter(|key| !allowed.contains(&key.as_str())) {
        push(
            issues,
            CmsPreflightSeverity::Blocking,
            CmsPreflightIssueCode::TypeFieldMismatch,
            context,
            &format!("{path}.{key}"),
            "Unknown field has no lossless CMS V2 mapping.",
        );
    }
}

pub(super) fn text(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(Into::into)
}

pub(super) fn push(
    issues: &mut Vec<CmsPreflightIssue>,
    severity: CmsPreflightSeverity,
    code: CmsPreflightIssueCode,
    context: Context,
    path: &str,
    message: &str,
) {
    issues.push(issue(
        severity,
        code,
        CmsPreflightSource::GeneralInformation,
        Some(context.id),
        Some(context.revision),
        path,
        message,
    ));
}
