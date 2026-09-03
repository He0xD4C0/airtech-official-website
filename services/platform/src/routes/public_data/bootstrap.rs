async fn site_bootstrap(
    State(state): State<AppState>,
    Query(query): Query<LocaleQuery>,
) -> Result<Json<SiteBootstrap>, ApiError> {
    validate_locale(&query.locale)?;
    let general_information = published_general_information(&state, &query.locale).await?;
    let navigation =
        published_shell_content(&state, ContentKind::Navigation, &query.locale).await?;
    let footer = published_shell_content(&state, ContentKind::Footer, &query.locale).await?;
    let product_families = product_family_presentations(general_information.as_ref());
    let motor_technologies = published_motor_technologies(&state, &query.locale).await?;
    Ok(Json(SiteBootstrap {
        general_information,
        navigation,
        footer,
        product_families,
        motor_technologies,
        generated_at: Utc::now(),
    }))
}

pub(super) async fn published_site_shell_has_placeholder(
    state: &AppState,
    locale: &str,
) -> Result<bool, ApiError> {
    let information = published_general_information(state, locale)
        .await?
        .ok_or_else(incomplete_site_shell)?;
    let navigation = published_shell_content(state, ContentKind::Navigation, locale)
        .await?
        .ok_or_else(incomplete_site_shell)?;
    let footer = published_shell_content(state, ContentKind::Footer, locale)
        .await?
        .ok_or_else(incomplete_site_shell)?;

    // Placeholders deliberately fail closed without requiring their temporary
    // payloads to satisfy the public rendering contract.
    if information.is_placeholder || navigation.is_placeholder || footer.is_placeholder {
        return Ok(true);
    }

    if !valid_published_general_information(&information, locale)
        || !valid_published_shell_content(&navigation, ContentKind::Navigation, locale)
        || !valid_published_shell_content(&footer, ContentKind::Footer, locale)
    {
        return Err(incomplete_site_shell());
    }

    Ok(false)
}

fn incomplete_site_shell() -> ApiError {
    ApiError::service_unavailable("The public site shell projection is incomplete.")
}

fn valid_published_general_information(information: &GeneralInformation, locale: &str) -> bool {
    let Some(payload) = information.payload.as_object() else {
        return false;
    };
    let brand_name = payload.get("brandName").and_then(Value::as_str);
    let home_path = payload.get("homePath").and_then(Value::as_str);
    let organization_name = payload
        .get("organization")
        .and_then(Value::as_object)
        .and_then(|organization| organization.get("name"))
        .and_then(Value::as_str);

    information.locale == locale
        && information.status == PublicationStatus::Published
        && information
            .published_revision
            .is_some_and(|revision| revision > 0)
        && valid_required_text(brand_name, 160)
        && home_path.is_some_and(|path| valid_site_home_path(path, locale))
        && valid_required_text(organization_name, 200)
}

fn valid_published_shell_content(content: &ContentEntry, kind: ContentKind, locale: &str) -> bool {
    content.kind == kind
        && content.locale == locale
        && content.status == PublicationStatus::Published
        && content
            .published_revision
            .is_some_and(|revision| revision > 0)
        && content.body.schema_version == 1
        && content
            .body
            .doc
            .as_object()
            .and_then(|document| document.get("type"))
            .and_then(Value::as_str)
            == Some("doc")
}

fn valid_required_text(value: Option<&str>, maximum_length: usize) -> bool {
    value.is_some_and(|value| {
        let value = value.trim();
        !value.is_empty() && value.len() <= maximum_length && !value.chars().any(char::is_control)
    })
}

fn valid_site_home_path(path: &str, locale: &str) -> bool {
    let path = path.trim();
    let locale_root = format!("/{locale}");
    (path == locale_root || path.starts_with(&format!("{locale_root}/")))
        && path.len() <= 2_048
        && !path.contains(['?', '#', '\\'])
        && !path.contains("//")
        && !path.chars().any(char::is_control)
}
