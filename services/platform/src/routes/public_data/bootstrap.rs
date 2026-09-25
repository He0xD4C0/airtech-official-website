use super::*;

pub(super) async fn site_bootstrap(
    State(state): State<AppState>,
    Query(query): Query<LocaleQuery>,
) -> Result<Json<SiteBootstrap>, ApiError> {
    validate_locale(&query.locale)?;
    let general_information =
        load_v2_singleton(&state, CmsContentKind::GeneralInformation, &query.locale)
            .await?
            .ok_or_else(incomplete_site_shell)?;
    let navigation = load_v2_singleton(&state, CmsContentKind::Navigation, &query.locale)
        .await?
        .ok_or_else(incomplete_site_shell)?;
    let footer = load_v2_singleton(&state, CmsContentKind::Footer, &query.locale)
        .await?
        .ok_or_else(incomplete_site_shell)?;
    validate_site_shell(&general_information, &navigation, &footer, &query.locale)?;
    let categories = match &general_information.type_fields {
        ContentTypeFields::GeneralInformation(fields) => Some(fields.product_categories.as_slice()),
        _ => None,
    }
    .unwrap_or_default();
    let product_families = product_family_presentations(categories);
    let motor_technologies = published_motor_technologies(&state, &query.locale).await?;
    Ok(Json(SiteBootstrap {
        general_information: Some(general_information),
        navigation: Some(navigation),
        footer: Some(footer),
        product_families,
        motor_technologies,
        generated_at: Utc::now(),
    }))
}

pub(crate) async fn published_site_shell_has_placeholder(
    state: &AppState,
    locale: &str,
) -> Result<bool, ApiError> {
    let information = load_v2_singleton(state, CmsContentKind::GeneralInformation, locale)
        .await?
        .ok_or_else(incomplete_site_shell)?;
    let navigation = load_v2_singleton(state, CmsContentKind::Navigation, locale)
        .await?
        .ok_or_else(incomplete_site_shell)?;
    let footer = load_v2_singleton(state, CmsContentKind::Footer, locale)
        .await?
        .ok_or_else(incomplete_site_shell)?;

    validate_site_shell(&information, &navigation, &footer, locale)
}

fn validate_site_shell(
    information: &PublicContentProjection,
    navigation: &PublicContentProjection,
    footer: &PublicContentProjection,
    locale: &str,
) -> Result<bool, ApiError> {
    if !valid_shell_information(information, locale)
        || !valid_shell_content(navigation, CmsContentKind::Navigation, locale)
        || !valid_shell_content(footer, CmsContentKind::Footer, locale)
    {
        return Err(incomplete_site_shell());
    }

    Ok(information.is_placeholder || navigation.is_placeholder || footer.is_placeholder)
}

pub(super) fn valid_shell_information(information: &PublicContentProjection, locale: &str) -> bool {
    information.schema_version == crate::models::CMS_V2_SCHEMA_VERSION
        && information.kind == CmsContentKind::GeneralInformation
        && information.locale == locale
        && information.published_revision > 0
        && matches!(
            &information.type_fields,
            ContentTypeFields::GeneralInformation(fields)
                if valid_required_text(fields.organization_name.as_deref(), 200)
                    && fields.home_path.as_deref().is_some_and(|path| valid_site_home_path(path, locale))
        )
}

pub(super) fn valid_shell_content(
    content: &PublicContentProjection,
    kind: CmsContentKind,
    locale: &str,
) -> bool {
    content.schema_version == crate::models::CMS_V2_SCHEMA_VERSION
        && content.kind == kind
        && content.locale == locale
        && content.published_revision > 0
        && match kind {
            CmsContentKind::Navigation => {
                matches!(&content.type_fields, ContentTypeFields::Navigation(_))
            }
            CmsContentKind::Footer => {
                matches!(&content.type_fields, ContentTypeFields::Footer(_))
            }
            _ => false,
        }
}

pub(super) fn incomplete_site_shell() -> ApiError {
    ApiError::service_unavailable("The public site shell projection is incomplete.")
}

pub(super) fn valid_required_text(value: Option<&str>, maximum_length: usize) -> bool {
    value.is_some_and(|value| {
        let value = value.trim();
        !value.is_empty() && value.len() <= maximum_length && !value.chars().any(char::is_control)
    })
}

pub(super) fn valid_site_home_path(path: &str, locale: &str) -> bool {
    let path = path.trim();
    let locale_root = format!("/{locale}");
    (path == locale_root || path.starts_with(&format!("{locale_root}/")))
        && path.len() <= 2_048
        && !path.contains(['?', '#', '\\'])
        && !path.contains("//")
        && !path.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use serde_json::{json, Value};
    use uuid::Uuid;

    use super::*;

    fn projection(kind: &str, type_fields: Value) -> PublicContentProjection {
        serde_json::from_value(json!({
            "schemaVersion": 2,
            "id": Uuid::new_v4(),
            "kind": kind,
            "locale": "en",
            "templateKey": kind,
            "title": kind,
            "slug": null,
            "summary": null,
            "isPlaceholder": true,
            "typeFields": type_fields,
            "body": null,
            "composition": { "blocks": [] },
            "seo": { "title": null, "description": null, "indexable": false, "socialImage": null },
            "publishedRevision": 1,
            "updatedAt": "2026-09-18T00:00:00Z",
            "resolvedRelations": [],
            "resolvedLinks": [],
            "resolvedMedia": []
        }))
        .unwrap()
    }

    fn information(organization_name: Value) -> PublicContentProjection {
        projection(
            "generalInformation",
            json!({
                "type": "generalInformation",
                "organizationName": organization_name,
                "brandLine": null,
                "homePath": "/en",
                "footerStatement": null,
                "copyrightTemplate": null,
                "contact": { "email": null, "phone": null, "addressLines": [], "locality": null, "region": null, "postalCode": null, "countryCode": null },
                "socialLinks": [],
                "defaultSeo": { "title": null, "description": null, "indexable": false, "socialImage": null },
                "productCategories": [],
                "navigationCta": null
            }),
        )
    }

    #[test]
    fn complete_placeholder_shell_is_available_but_invalid_shell_is_not() {
        let navigation = projection("navigation", json!({ "type": "navigation", "items": [] }));
        let footer = projection(
            "footer",
            json!({ "type": "footer", "columns": [], "legalLinks": [] }),
        );
        assert!(validate_site_shell(
            &information(json!("AIRTEKPOWER Development Preview")),
            &navigation,
            &footer,
            "en"
        )
        .unwrap());
        let error =
            validate_site_shell(&information(Value::Null), &navigation, &footer, "en").unwrap_err();
        assert_eq!(error.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
