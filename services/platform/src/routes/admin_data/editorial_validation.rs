fn normalize_news_ownership(
    production: bool,
    previous: Option<DataClass>,
    input: &mut NewsDraftInput,
) -> Result<(), ApiError> {
    if input.data_class != DataClass::DevelopmentFixture {
        return Ok(());
    }
    if previous.is_none() {
        return Err(ApiError::validation(BTreeMap::from([(
            "dataClass".into(),
            vec![
                "Development fixtures can only be created by the development-only seed command."
                    .into(),
            ],
        )])));
    }
    if !input.content.is_placeholder {
        // Clearing the placeholder flag is the explicit Admin takeover signal.
        input.data_class = DataClass::Editorial;
        return Ok(());
    }
    if previous == Some(DataClass::Editorial) {
        return Err(ApiError::validation(BTreeMap::from([(
            "dataClass".into(),
            vec!["Editorial News cannot be reassigned to development fixture ownership.".into()],
        )])));
    }
    if production {
        return Err(ApiError::validation(BTreeMap::from([(
            "dataClass".into(),
            vec!["Development fixture News cannot be written by the production Admin API.".into()],
        )])));
    }
    Ok(())
}

fn validate_news_input(input: &mut NewsDraftInput) -> Result<(), ApiError> {
    input.content.kind = ContentKind::News;
    super::admin::validate_content_input(&input.content)?;
    let mut errors = BTreeMap::new();
    if input.content.slug.is_empty()
        || input.content.slug.len() > 200
        || !input
            .content
            .slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        errors.insert(
            "content.slug".into(),
            vec!["Must be a lowercase URL slug.".into()],
        );
    }
    if input.content.title.trim().is_empty() || input.content.title.len() > 300 {
        errors.insert(
            "content.title".into(),
            vec!["Must contain 1 to 300 characters.".into()],
        );
    }
    if input.category.trim().is_empty() || input.category.len() > 80 {
        errors.insert(
            "category".into(),
            vec!["Must contain 1 to 80 characters.".into()],
        );
    }
    if input.author_display_name.trim().is_empty() || input.author_display_name.len() > 160 {
        errors.insert(
            "authorDisplayName".into(),
            vec!["Must contain 1 to 160 characters.".into()],
        );
    }
    if matches!(input.data_class, DataClass::Feishu | DataClass::VerifiedCsv) {
        errors.insert(
            "dataClass".into(),
            vec!["News may be editorial or a development fixture only.".into()],
        );
    }
    let expected_canonical = format!("/en/resources/news/{}", input.content.slug);
    if input.content.seo.canonical_path.as_deref() != Some(expected_canonical.as_str()) {
        errors.insert(
            "content.seo.canonicalPath".into(),
            vec![format!("Must exactly equal `{expected_canonical}`.")],
        );
    }
    if input.data_class == DataClass::DevelopmentFixture {
        input.content.is_placeholder = true;
        input.content.seo.indexable = false;
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ApiError::validation(errors))
    }
}

fn validate_general_information(input: &GeneralInformationDraftInput) -> Result<(), ApiError> {
    if !valid_locale(&input.locale) {
        return Err(ApiError::bad_request("locale is invalid."));
    }
    let object = input
        .payload
        .as_object()
        .ok_or_else(|| ApiError::bad_request("payload must be a JSON object."))?;
    if serde_json::to_vec(&input.payload)
        .map_err(|_| ApiError::bad_request("payload is not serializable."))?
        .len()
        > 256 * 1024
    {
        return Err(ApiError::bad_request("payload must not exceed 256 KiB."));
    }
    let allowed = [
        "brandName",
        "brandLine",
        "homePath",
        "footerStatement",
        "copyrightText",
        "defaultSeo",
        "organization",
        "navigationCta",
        "productCategories",
    ];
    let mut errors = BTreeMap::<String, Vec<String>>::new();
    for unknown in object.keys().filter(|key| !allowed.contains(&key.as_str())) {
        errors.insert(
            format!("payload.{unknown}"),
            vec!["Unknown General Information field.".into()],
        );
    }
    for &required in &allowed[..7] {
        if !object.contains_key(required) {
            errors.insert(
                format!("payload.{required}"),
                vec!["This General Information field is required.".into()],
            );
        }
    }
    validate_required_text(
        object.get("brandName"),
        "payload.brandName",
        120,
        &mut errors,
    );
    validate_nullable_text(
        object.get("brandLine"),
        "payload.brandLine",
        300,
        &mut errors,
    );
    validate_nullable_text(
        object.get("footerStatement"),
        "payload.footerStatement",
        2_000,
        &mut errors,
    );
    validate_nullable_text(
        object.get("copyrightText"),
        "payload.copyrightText",
        500,
        &mut errors,
    );
    match object.get("homePath").and_then(Value::as_str) {
        Some(path) if safe_internal_path(path) && (path == "/en" || path.starts_with("/en/")) => {}
        _ => insert_validation(
            &mut errors,
            "payload.homePath",
            "Must be a safe English public path beginning with /en.",
        ),
    }
    validate_default_seo(object.get("defaultSeo"), &mut errors);
    validate_organization(object.get("organization"), &mut errors);
    validate_navigation_cta(object.get("navigationCta"), &mut errors);
    validate_product_categories(object.get("productCategories"), &mut errors);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ApiError::validation(errors))
    }
}
