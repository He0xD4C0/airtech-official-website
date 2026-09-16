use super::*;

pub(super) fn validate_locale(value: &str) -> Result<(), ApiError> {
    if valid_locale_tag(value) {
        Ok(())
    } else {
        Err(ApiError::bad_request("locale is invalid."))
    }
}

pub(super) fn valid_locale_tag(value: &str) -> bool {
    (2..=35).contains(&value.len())
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

pub(super) fn validate_public_path(path: &str) -> Result<(), ApiError> {
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

pub(super) fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

pub(super) fn default_locale() -> String {
    "en".into()
}
