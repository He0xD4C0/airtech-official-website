fn validate_default_seo(value: Option<&Value>, errors: &mut BTreeMap<String, Vec<String>>) {
    let Some(object) = value.and_then(Value::as_object) else {
        insert_validation(errors, "payload.defaultSeo", "Must be a JSON object.");
        return;
    };
    for unknown in object
        .keys()
        .filter(|key| !matches!(key.as_str(), "title" | "description"))
    {
        insert_validation(
            errors,
            &format!("payload.defaultSeo.{unknown}"),
            "Unknown SEO field.",
        );
    }
    validate_nullable_text(object.get("title"), "payload.defaultSeo.title", 300, errors);
    validate_nullable_text(
        object.get("description"),
        "payload.defaultSeo.description",
        1_000,
        errors,
    );
}

fn validate_organization(value: Option<&Value>, errors: &mut BTreeMap<String, Vec<String>>) {
    let Some(object) = value.and_then(Value::as_object) else {
        insert_validation(errors, "payload.organization", "Must be a JSON object.");
        return;
    };
    let allowed = [
        "name",
        "url",
        "logoUrl",
        "legalName",
        "salesEmail",
        "marketingEmail",
        "address",
        "socialLinks",
    ];
    for unknown in object.keys().filter(|key| !allowed.contains(&key.as_str())) {
        insert_validation(
            errors,
            &format!("payload.organization.{unknown}"),
            "Unknown organization field.",
        );
    }
    validate_required_text(object.get("name"), "payload.organization.name", 200, errors);
    validate_nullable_text(
        object.get("legalName"),
        "payload.organization.legalName",
        300,
        errors,
    );
    validate_nullable_text(
        object.get("address"),
        "payload.organization.address",
        1_000,
        errors,
    );
    for field in ["salesEmail", "marketingEmail"] {
        match object.get(field) {
            None | Some(Value::Null) => {}
            Some(Value::String(value))
                if value.len() <= 254
                    && !value.chars().any(char::is_control)
                    && value.split_once('@').is_some_and(|(local, domain)| {
                        !local.is_empty() && !domain.is_empty() && !domain.starts_with('.')
                    }) => {}
            _ => insert_validation(
                errors,
                &format!("payload.organization.{field}"),
                "Must be null or a valid email address.",
            ),
        }
    }
    match object.get("url") {
        None | Some(Value::Null) => {}
        Some(Value::String(value)) if safe_https_url(value) => {}
        _ => insert_validation(
            errors,
            "payload.organization.url",
            "Must be null or an HTTPS URL.",
        ),
    }
    match object.get("logoUrl") {
        None | Some(Value::Null) => {}
        Some(Value::String(value)) if safe_https_url(value) || safe_internal_path(value) => {}
        _ => insert_validation(
            errors,
            "payload.organization.logoUrl",
            "Must be null, an HTTPS URL, or an absolute internal path.",
        ),
    }
    let Some(links) = object.get("socialLinks") else {
        return;
    };
    let Some(links) = links.as_array().filter(|links| links.len() <= 20) else {
        insert_validation(
            errors,
            "payload.organization.socialLinks",
            "Must be an array containing at most 20 links.",
        );
        return;
    };
    for (index, link) in links.iter().enumerate() {
        let path = format!("payload.organization.socialLinks.{index}");
        let Some(link) = link.as_object() else {
            insert_validation(errors, &path, "Must be a JSON object.");
            continue;
        };
        if link.len() != 2 || !link.contains_key("label") || !link.contains_key("url") {
            insert_validation(
                errors,
                &path,
                "Only label and url are allowed and required.",
            );
        }
        validate_required_text(link.get("label"), &format!("{path}.label"), 120, errors);
        match link.get("url").and_then(Value::as_str) {
            Some(url) if safe_https_url(url) => {}
            _ => insert_validation(errors, &format!("{path}.url"), "Must be an HTTPS URL."),
        }
    }
}

fn validate_navigation_cta(value: Option<&Value>, errors: &mut BTreeMap<String, Vec<String>>) {
    let Some(value) = value else { return };
    if value.is_null() {
        return;
    }
    let Some(object) = value.as_object() else {
        insert_validation(
            errors,
            "payload.navigationCta",
            "Must be null or a JSON object.",
        );
        return;
    };
    if object.len() != 2 || !object.contains_key("label") || !object.contains_key("href") {
        insert_validation(
            errors,
            "payload.navigationCta",
            "Only label and href are allowed and required.",
        );
    }
    validate_required_text(
        object.get("label"),
        "payload.navigationCta.label",
        120,
        errors,
    );
    match object.get("href").and_then(Value::as_str) {
        Some(path) if safe_internal_path(path) && (path == "/en" || path.starts_with("/en/")) => {}
        _ => insert_validation(
            errors,
            "payload.navigationCta.href",
            "Must be a safe English public path beginning with /en.",
        ),
    }
}

fn validate_product_categories(value: Option<&Value>, errors: &mut BTreeMap<String, Vec<String>>) {
    let Some(value) = value else { return };
    let Some(categories) = value.as_array().filter(|items| items.len() <= 10) else {
        insert_validation(
            errors,
            "payload.productCategories",
            "Must be an array containing at most 10 category presentations.",
        );
        return;
    };
    let mut codes = HashSet::new();
    for (index, category) in categories.iter().enumerate() {
        let path = format!("payload.productCategories.{index}");
        let Some(category) = category.as_object() else {
            insert_validation(errors, &path, "Must be a JSON object.");
            continue;
        };
        if category.len() != 5
            || !["code", "slug", "name", "description", "sortOrder"]
                .iter()
                .all(|key| category.contains_key(*key))
        {
            insert_validation(
                errors,
                &path,
                "Only code, slug, name, description, and sortOrder are allowed and required.",
            );
        }
        match category.get("code").and_then(Value::as_str) {
            Some(code)
                if matches!(
                    code,
                    "centrifugal" | "axial" | "crossFlow" | "inlineDuct" | "motors"
                ) && codes.insert(code.to_owned()) => {}
            _ => insert_validation(
                errors,
                &format!("{path}.code"),
                "Must be a unique supported product-family code.",
            ),
        }
        match category.get("slug").and_then(Value::as_str) {
            Some(slug)
                if !slug.is_empty()
                    && slug.len() <= 120
                    && slug.bytes().all(|byte| {
                        byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'
                    }) => {}
            _ => insert_validation(errors, &format!("{path}.slug"), "Must be a URL slug."),
        }
        validate_required_text(category.get("name"), &format!("{path}.name"), 120, errors);
        validate_required_text(
            category.get("description"),
            &format!("{path}.description"),
            2_000,
            errors,
        );
        if !category
            .get("sortOrder")
            .and_then(Value::as_i64)
            .is_some_and(|value| (-10_000..=10_000).contains(&value))
        {
            insert_validation(
                errors,
                &format!("{path}.sortOrder"),
                "Must be an integer from -10000 to 10000.",
            );
        }
    }
}

fn validate_required_text(
    value: Option<&Value>,
    path: &str,
    max: usize,
    errors: &mut BTreeMap<String, Vec<String>>,
) {
    match value.and_then(Value::as_str) {
        Some(value)
            if !value.trim().is_empty()
                && value.len() <= max
                && !value.chars().any(char::is_control) => {}
        _ => insert_validation(
            errors,
            path,
            &format!("Must be a non-empty string of at most {max} characters."),
        ),
    }
}

fn validate_nullable_text(
    value: Option<&Value>,
    path: &str,
    max: usize,
    errors: &mut BTreeMap<String, Vec<String>>,
) {
    match value {
        None | Some(Value::Null) => {}
        Some(Value::String(value))
            if value.len() <= max && !value.chars().any(char::is_control) => {}
        _ => insert_validation(
            errors,
            path,
            &format!("Must be null or a string of at most {max} characters."),
        ),
    }
}

fn safe_internal_path(value: &str) -> bool {
    value.starts_with('/')
        && value.len() <= 2_048
        && !value.starts_with("//")
        && !value.contains(['\\', '\r', '\n', '\0'])
        && !value.to_ascii_lowercase().contains("javascript:")
}

fn safe_https_url(value: &str) -> bool {
    value.starts_with("https://")
        && value.len() <= 2_048
        && value[8..].contains('.')
        && !value.chars().any(char::is_control)
}

fn insert_validation(errors: &mut BTreeMap<String, Vec<String>>, path: &str, message: &str) {
    errors
        .entry(path.to_owned())
        .or_default()
        .push(message.to_owned());
}
