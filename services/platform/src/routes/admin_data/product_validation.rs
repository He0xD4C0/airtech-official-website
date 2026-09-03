fn validate_product_presentation(input: &UpdateProductPresentation) -> Result<(), ApiError> {
    let mut errors = BTreeMap::new();
    if !valid_locale(&input.locale) {
        errors.insert("locale".into(), vec!["Must be a valid locale tag.".into()]);
    }
    if input.slug.is_empty()
        || input.slug.len() > 200
        || !input
            .slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        errors.insert("slug".into(), vec!["Must be a lowercase URL slug.".into()]);
    }
    if input.title.trim().is_empty() || input.title.len() > 300 {
        errors.insert(
            "title".into(),
            vec!["Must contain 1 to 300 characters.".into()],
        );
    }
    if input
        .summary
        .as_ref()
        .is_some_and(|value| value.len() > 2_000)
    {
        errors.insert(
            "summary".into(),
            vec!["Must not exceed 2,000 characters.".into()],
        );
    }
    if input.seo.canonical_path.as_ref().is_some_and(|path| {
        !path.starts_with("/en/")
            || path.contains(['?', '#', '\\', '\r', '\n', '\0'])
            || path.starts_with("//")
            || path.len() > 2_048
    }) {
        errors.insert(
            "seo.canonicalPath".into(),
            vec!["Must be an English public path beginning with /en/.".into()],
        );
    }
    if !(-10_000..=10_000).contains(&input.sort_order) {
        errors.insert(
            "sortOrder".into(),
            vec!["Must be from -10000 to 10000.".into()],
        );
    }
    if input.related_content_ids.len() > 100
        || input
            .related_content_ids
            .iter()
            .collect::<HashSet<_>>()
            .len()
            != input.related_content_ids.len()
    {
        errors.insert(
            "relatedContentIds".into(),
            vec!["Must contain at most 100 unique content ids.".into()],
        );
    }
    validate_reason(&input.reason)?;
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ApiError::validation(errors))
    }
}

fn validate_product_canonical(
    input: &UpdateProductPresentation,
    family: crate::models::ProductFamily,
) -> Result<(), ApiError> {
    let Some(canonical_path) = input.seo.canonical_path.as_deref() else {
        if input.indexable {
            return Err(ApiError::validation(BTreeMap::from([(
                "seo.canonicalPath".into(),
                vec!["An indexable product requires a canonical path.".into()],
            )])));
        }
        return Ok(());
    };
    let family = match family {
        crate::models::ProductFamily::Centrifugal => "centrifugal",
        crate::models::ProductFamily::Axial => "axial",
        crate::models::ProductFamily::CrossFlow => "cross-flow",
        crate::models::ProductFamily::InlineDuct => "inline-duct",
        crate::models::ProductFamily::Motors => "motors",
    };
    let expected = format!("/en/products/{family}/{}", input.slug);
    if canonical_path != expected {
        return Err(ApiError::validation(BTreeMap::from([(
            "seo.canonicalPath".into(),
            vec![format!("Must exactly match {expected}.")],
        )])));
    }
    Ok(())
}

const INVITATION_REPLAY_ENVELOPE_VERSION: u8 = 1;
const INVITATION_REPLAY_NONCE_BYTES: usize = 12;
