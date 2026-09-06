use serde_json::{Map, Value};

use crate::models::{ContactInformationInput, SocialLinkInput};

use super::{
    general_information_fields::{check_keys, push, text, Context},
    types::{CmsPreflightIssue, CmsPreflightIssueCode, CmsPreflightSeverity},
};

pub(super) fn contact_information(
    organization: Option<&Map<String, Value>>,
    context: Context,
    issues: &mut Vec<CmsPreflightIssue>,
) -> ContactInformationInput {
    let sales = organization.and_then(|value| text(value.get("salesEmail")));
    let marketing = organization.and_then(|value| text(value.get("marketingEmail")));
    if sales.is_some() && marketing.is_some() && sales != marketing {
        push(
            issues,
            CmsPreflightSeverity::Blocking,
            CmsPreflightIssueCode::TypeFieldMismatch,
            context,
            "payload.organization",
            "Different sales and marketing emails cannot collapse into one V2 contact email.",
        );
    }
    let address_lines = organization
        .and_then(|value| text(value.get("address")))
        .map(|value| {
            value
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(Into::into)
                .collect()
        })
        .unwrap_or_default();
    ContactInformationInput {
        email: sales.or(marketing),
        phone: None,
        address_lines,
        locality: None,
        region: None,
        postal_code: None,
        country_code: None,
    }
}

pub(super) fn social_links(
    organization: Option<&Map<String, Value>>,
    context: Context,
    issues: &mut Vec<CmsPreflightIssue>,
) -> Vec<SocialLinkInput> {
    let Some(value) = organization.and_then(|value| value.get("socialLinks")) else {
        return Vec::new();
    };
    let Some(values) = value.as_array() else {
        push(
            issues,
            CmsPreflightSeverity::Blocking,
            CmsPreflightIssueCode::TypeFieldMismatch,
            context,
            "payload.organization.socialLinks",
            "socialLinks must be an array.",
        );
        return Vec::new();
    };
    values
        .iter()
        .enumerate()
        .filter_map(|(index, value)| {
            let path = format!("payload.organization.socialLinks.{index}");
            let Some(object) = value.as_object() else {
                push(
                    issues,
                    CmsPreflightSeverity::Blocking,
                    CmsPreflightIssueCode::TypeFieldMismatch,
                    context,
                    &path,
                    "Social link must be an object.",
                );
                return None;
            };
            check_keys(object, &["label", "url"], &path, context, issues);
            match (text(object.get("label")), text(object.get("url"))) {
                (Some(service), Some(url)) => Some(SocialLinkInput { service, url }),
                _ => {
                    push(
                        issues,
                        CmsPreflightSeverity::Blocking,
                        CmsPreflightIssueCode::TypeFieldMismatch,
                        context,
                        &path,
                        "Social link requires string label and URL.",
                    );
                    None
                }
            }
        })
        .collect()
}
