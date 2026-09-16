use super::*;

pub(super) fn product_family_presentations(
    categories: &[crate::models::ProductCategoryPresentationInput],
) -> Vec<ProductFamilyPresentation> {
    let mut presentations = categories
        .iter()
        .filter_map(|category| {
            let code_enum = category.code;
            let slug = category.slug.as_str();
            let name = category.name.as_str();
            if !valid_slug(slug) || name.trim().is_empty() || name.len() > 120 {
                return None;
            }
            Some(ProductFamilyPresentation {
                code: code_enum,
                slug: slug.to_owned(),
                name: name.to_owned(),
                description: category.description.clone(),
                sort_order: category.sort_order,
            })
        })
        .collect::<Vec<_>>();
    presentations.sort_by_key(|value| value.sort_order);
    presentations.dedup_by_key(|value| value.code);
    presentations
}

pub(super) fn validate_guest_visit(
    config: &Config,
    request: &CreateGuestVisit,
) -> Result<(), ApiError> {
    let mut errors = BTreeMap::new();
    if !valid_guest_landing_path(&request.landing_path) {
        errors.insert(
            "landingPath".into(),
            vec![
                "Must be a clean public path without query, fragment, encoded sensitive markers, or PII-like segments."
                    .into(),
            ],
        );
    }
    if request
        .referrer_domain
        .as_deref()
        .is_some_and(|value| !valid_referrer_hostname(value))
    {
        errors.insert(
            "referrerDomain".into(),
            vec![
                "Must be a hostname without a URL, IP address, control character, or PII-like value."
                    .into(),
            ],
        );
    }
    for (name, value, maximum, allowlist) in [
        (
            "source",
            request.source.as_deref(),
            128_usize,
            &config.analytics_utm_source_allowlist,
        ),
        (
            "medium",
            request.medium.as_deref(),
            128,
            &config.analytics_utm_medium_allowlist,
        ),
        (
            "campaign",
            request.campaign.as_deref(),
            200,
            &config.analytics_utm_campaign_allowlist,
        ),
    ] {
        if value.is_some_and(|value| !valid_guest_attribution(value, maximum, allowlist)) {
            errors.insert(
                name.into(),
                vec![format!(
                    "Must be an approved non-PII analytics identifier no longer than {maximum} characters."
                )],
            );
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ApiError::validation(errors))
    }
}
