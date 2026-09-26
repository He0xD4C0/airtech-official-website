use super::*;

pub(super) fn validate_contact(request: &CreateContactRequest) -> Result<(), ApiError> {
    let mut errors = validate_business_contact(&request.contact);
    if request.topic.trim().is_empty() {
        errors.insert("topic".into(), vec!["Topic is required.".into()]);
    }
    if request.message.trim().len() < 10 || request.message.len() > 10_000 {
        errors.insert(
            "message".into(),
            vec!["Message must contain between 10 and 10,000 characters.".into()],
        );
    }
    if !request.consent {
        errors.insert("consent".into(), vec!["Consent is required.".into()]);
    }
    validate_public_source_and_locale(
        &request.source_path,
        &request.locale,
        "/en/company/contact",
        &mut errors,
    );
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ApiError::validation(errors))
    }
}

pub(super) fn validate_business_contact(
    contact: &airtek_domain::models::BusinessContact,
) -> BTreeMap<String, Vec<String>> {
    let mut errors = BTreeMap::new();
    if !valid_text(&contact.name, 200) {
        errors.insert(
            "contact.name".into(),
            vec!["A valid name is required.".into()],
        );
    }
    let email = contact.email.trim();
    let email_parts = email.split('@').collect::<Vec<_>>();
    if email.len() > 320
        || email_parts.len() != 2
        || email_parts[0].is_empty()
        || !email_parts[1].contains('.')
        || email.chars().any(char::is_whitespace)
        || email.chars().any(char::is_control)
    {
        errors.insert(
            "contact.email".into(),
            vec!["A valid email address is required.".into()],
        );
    }
    if contact.phone.as_deref().is_some_and(|value| {
        !valid_text(value, 50)
            || !(5..=20).contains(&value.bytes().filter(u8::is_ascii_digit).count())
    }) {
        errors.insert(
            "contact.phone".into(),
            vec!["Phone must contain 5 to 20 digits and no more than 50 characters.".into()],
        );
    }
    validate_optional_text(
        "contact.company",
        contact.company.as_deref(),
        200,
        &mut errors,
    );
    validate_optional_text(
        "contact.countryOrRegion",
        contact.country_or_region.as_deref(),
        120,
        &mut errors,
    );
    errors
}

pub(super) fn validate_public_source_and_locale(
    source_path: &str,
    locale: &str,
    expected_prefix: &str,
    errors: &mut BTreeMap<String, Vec<String>>,
) {
    if source_path.len() > 2_048
        || !source_path.starts_with(expected_prefix)
        || source_path.contains(['?', '#'])
        || source_path.chars().any(char::is_control)
    {
        errors.insert(
            "sourcePath".into(),
            vec![
                "Source path must be a canonical public path without query or fragment data."
                    .into(),
            ],
        );
    }
    if locale != "en" {
        errors.insert(
            "locale".into(),
            vec!["Only the published English locale is currently accepted.".into()],
        );
    }
}

pub(super) fn validate_required_text(
    field: &str,
    value: &str,
    maximum_length: usize,
    errors: &mut BTreeMap<String, Vec<String>>,
) {
    if !valid_text(value, maximum_length) {
        errors.insert(
            field.into(),
            vec![format!(
                "Must contain text no longer than {maximum_length} characters."
            )],
        );
    }
}

pub(super) fn validate_optional_text(
    field: &str,
    value: Option<&str>,
    maximum_length: usize,
    errors: &mut BTreeMap<String, Vec<String>>,
) {
    if value.is_some_and(|value| !valid_text(value, maximum_length)) {
        errors.insert(
            field.into(),
            vec![format!(
                "When supplied, must contain text no longer than {maximum_length} characters."
            )],
        );
    }
}

pub(super) fn valid_text(value: &str, maximum_length: usize) -> bool {
    !value.trim().is_empty()
        && value.chars().count() <= maximum_length
        && !value.chars().any(char::is_control)
}
