fn validate_locale(value: &str) -> Result<(), ApiError> {
    if valid_locale_tag(value) {
        Ok(())
    } else {
        Err(ApiError::bad_request("locale is invalid."))
    }
}

fn valid_locale_tag(value: &str) -> bool {
    (2..=35).contains(&value.len())
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn validate_public_path(path: &str) -> Result<(), ApiError> {
    if path.starts_with('/')
        && path.len() <= 2_048
        && !path.contains('?')
        && !path.contains('#')
        && !path.contains("//")
        && !path.starts_with("/admin")
    {
        Ok(())
    } else {
        Err(ApiError::bad_request("path is not a clean public path."))
    }
}

fn decode_content(payload: Value, entity: &str) -> Result<ContentEntry, ApiError> {
    serde_json::from_value(payload).map_err(|error| {
        tracing::error!(%error, entity, "stored JSON payload is invalid");
        ApiError::service_unavailable(format!("Stored {entity} data is invalid."))
    })
}

fn decode_data_class(value: String) -> DataClass {
    match value.as_str() {
        "developmentFixture" => DataClass::DevelopmentFixture,
        "feishu" => DataClass::Feishu,
        "verifiedCsv" => DataClass::VerifiedCsv,
        _ => DataClass::Editorial,
    }
}

fn content_kind_label(kind: ContentKind) -> &'static str {
    match kind {
        ContentKind::Home => "home",
        ContentKind::Solution => "solution",
        ContentKind::Technology => "technology",
        ContentKind::Article => "article",
        ContentKind::News => "news",
        ContentKind::Faq => "faq",
        ContentKind::CaseStudy => "caseStudy",
        ContentKind::Download => "download",
        ContentKind::Company => "company",
        ContentKind::Legal => "legal",
        ContentKind::Navigation => "navigation",
        ContentKind::Footer => "footer",
    }
}

fn template_key(kind: ContentKind) -> &'static str {
    match kind {
        ContentKind::Home => "home",
        ContentKind::Solution => "solution",
        ContentKind::Technology => "technology",
        ContentKind::Article => "article",
        ContentKind::News => "news",
        ContentKind::Faq => "faq",
        ContentKind::CaseStudy => "caseStudy",
        ContentKind::Download => "download",
        ContentKind::Company => "company",
        ContentKind::Legal => "legal",
        ContentKind::Navigation => "navigation",
        ContentKind::Footer => "footer",
    }
}

fn parse_product_family_code(value: &str) -> Option<ProductFamily> {
    match value {
        "centrifugal" => Some(ProductFamily::Centrifugal),
        "axial" => Some(ProductFamily::Axial),
        "crossFlow" => Some(ProductFamily::CrossFlow),
        "inlineDuct" => Some(ProductFamily::InlineDuct),
        "motors" => Some(ProductFamily::Motors),
        _ => None,
    }
}

fn page_template_key(page: &ContentEntry) -> Option<String> {
    let value = page
        .body
        .doc
        .pointer("/attrs/pageSlots/templateKey")?
        .as_str()?;
    matches!(
        value,
        "home"
            | "products"
            | "productFamily"
            | "productDetail"
            | "selector"
            | "compare"
            | "solutions"
            | "solution"
            | "technology"
            | "articleIndex"
            | "article"
            | "newsIndex"
            | "news"
            | "faq"
            | "caseStudy"
            | "downloads"
            | "about"
            | "contact"
            | "rfq"
            | "search"
            | "legal"
    )
    .then(|| value.to_owned())
}

fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn default_locale() -> String {
    "en".into()
}
