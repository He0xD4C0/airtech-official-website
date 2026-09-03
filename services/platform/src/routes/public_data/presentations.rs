fn product_family_presentations(
    general_information: Option<&GeneralInformation>,
) -> Vec<ProductFamilyPresentation> {
    let categories = general_information
        .and_then(|information| information.payload.get("productCategories"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut presentations = categories
        .into_iter()
        .filter_map(|category| {
            let code = category.get("code")?.as_str()?;
            let code_enum = parse_product_family_code(code)?;
            let slug = category.get("slug")?.as_str()?;
            let name = category.get("name")?.as_str()?;
            if !valid_slug(slug) || name.trim().is_empty() || name.len() > 120 {
                return None;
            }
            Some(ProductFamilyPresentation {
                code: code_enum,
                slug: slug.to_owned(),
                name: name.to_owned(),
                description: category
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                sort_order: category
                    .get("sortOrder")
                    .and_then(Value::as_i64)
                    .and_then(|value| i32::try_from(value).ok())
                    .unwrap_or(i32::MAX),
            })
        })
        .collect::<Vec<_>>();
    presentations.sort_by_key(|value| value.sort_order);
    presentations.dedup_by_key(|value| value.code);
    presentations
}

fn decode_guest_visit(row: &sqlx::postgres::PgRow) -> Result<GuestVisit, ApiError> {
    Ok(GuestVisit {
        id: row.try_get("id")?,
        anonymous_session_id: row.try_get("anonymous_session_id")?,
        landing_path: row.try_get("landing_path")?,
        referrer_domain: row.try_get("referrer_host")?,
        source: row.try_get("source_type")?,
        medium: row.try_get("utm_medium")?,
        campaign: row.try_get("utm_campaign")?,
        first_seen_at: row.try_get("first_seen_at")?,
        last_seen_at: row.try_get("last_seen_at")?,
        retention_until: row.try_get("retention_until")?,
    })
}

fn validate_guest_visit(config: &Config, request: &CreateGuestVisit) -> Result<(), ApiError> {
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
