use airtek_domain::models::{
    ContentDraftV2, EditorialAction, LinkTargetReference, MediaUseReference, SeoInput,
};

mod blocks;
mod fields;

pub(super) type ContractIssue = (String, String);

pub(super) fn validate(draft: &ContentDraftV2) -> Vec<ContractIssue> {
    let mut issues = Vec::new();
    validate_locale(&draft.locale, &mut issues);
    string(&draft.title, 300, "title", &mut issues);
    optional_string(draft.slug.as_deref(), 180, "slug", &mut issues);
    if draft.slug.as_deref().is_some_and(|slug| !valid_slug(slug)) {
        invalid(
            &mut issues,
            "slug",
            "Slug must contain lowercase ASCII words separated by single hyphens.",
        );
    }
    optional_string(draft.summary.as_deref(), 2_000, "summary", &mut issues);
    validate_seo(&draft.seo, "seo", &mut issues);

    for (index, relation) in draft.relations.iter().enumerate() {
        string(
            &relation.slot,
            120,
            &format!("relations.{index}.slot"),
            &mut issues,
        );
    }
    for (index, block) in draft.composition.blocks.iter().enumerate() {
        blocks::validate(block, &format!("composition.blocks.{index}"), &mut issues);
    }
    fields::validate(&draft.type_fields, &mut issues);
    issues
}

fn validate_locale(locale: &str, issues: &mut Vec<ContractIssue>) {
    let segments = locale.split('-').collect::<Vec<_>>();
    let language_valid = segments
        .first()
        .is_some_and(|value| (2..=3).contains(&value.len()) && value.bytes().all(is_ascii_alpha));
    let extensions_valid = segments
        .iter()
        .skip(1)
        .all(|value| (2..=8).contains(&value.len()) && value.bytes().all(is_ascii_alphanumeric));
    if !(2..=35).contains(&locale.chars().count()) || !language_valid || !extensions_valid {
        invalid(
            issues,
            "locale",
            "Locale must be a 2-3 letter language followed by optional 2-8 character subtags.",
        );
    }
}

fn is_ascii_alpha(value: u8) -> bool {
    value.is_ascii_alphabetic()
}

fn is_ascii_alphanumeric(value: u8) -> bool {
    value.is_ascii_alphanumeric()
}

pub(super) fn valid_slug(value: &str) -> bool {
    value.is_empty()
        || value.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
        })
}

pub(super) fn validate_seo(seo: &SeoInput, path: &str, issues: &mut Vec<ContractIssue>) {
    optional_string(seo.title.as_deref(), 300, &format!("{path}.title"), issues);
    optional_string(
        seo.description.as_deref(),
        1_000,
        &format!("{path}.description"),
        issues,
    );
    optional_media(
        seo.social_image.as_ref(),
        &format!("{path}.socialImage"),
        issues,
    );
}

pub(super) fn optional_media(
    media: Option<&MediaUseReference>,
    path: &str,
    issues: &mut Vec<ContractIssue>,
) {
    if let Some(media) = media {
        validate_media(media, path, issues);
    }
}

pub(super) fn validate_media(
    media: &MediaUseReference,
    path: &str,
    issues: &mut Vec<ContractIssue>,
) {
    optional_string(
        media.alt_text.as_deref(),
        500,
        &format!("{path}.altText"),
        issues,
    );
}

pub(super) fn validate_action(
    action: &EditorialAction,
    path: &str,
    issues: &mut Vec<ContractIssue>,
) {
    string(&action.label, 120, &format!("{path}.label"), issues);
    validate_target(&action.target, &format!("{path}.target"), issues);
}

pub(super) fn validate_target(
    target: &LinkTargetReference,
    path: &str,
    issues: &mut Vec<ContractIssue>,
) {
    match target {
        LinkTargetReference::Content { .. } => {}
        LinkTargetReference::Route { path: value } => {
            string(value, 2_048, &format!("{path}.path"), issues);
        }
        LinkTargetReference::External { url } => {
            string(url, 2_048, &format!("{path}.url"), issues);
        }
    }
}

pub(super) fn optional_string(
    value: Option<&str>,
    max_length: usize,
    path: &str,
    issues: &mut Vec<ContractIssue>,
) {
    if let Some(value) = value {
        string(value, max_length, path, issues);
    }
}

pub(super) fn string(value: &str, max_length: usize, path: &str, issues: &mut Vec<ContractIssue>) {
    if value.chars().count() > max_length {
        invalid(
            issues,
            path,
            &format!("Value exceeds the maximum length of {max_length} characters."),
        );
    }
}

pub(super) fn unique<T: PartialEq>(
    values: &[T],
    path: &str,
    message: &str,
    issues: &mut Vec<ContractIssue>,
) {
    if values
        .iter()
        .enumerate()
        .any(|(index, value)| values[..index].contains(value))
    {
        invalid(issues, path, message);
    }
}

pub(super) fn invalid(issues: &mut Vec<ContractIssue>, path: &str, message: &str) {
    issues.push((path.into(), message.into()));
}
