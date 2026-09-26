use std::process::Command;

use axum::http::HeaderMap;
use uuid::Uuid;

use airtek_runtime::error::ApiError;
use airtek_runtime::state::AppState;

pub(super) fn ensure_non_root() -> Result<(), ApiError> {
    #[cfg(unix)]
    {
        let uid = current_uid()?;
        if uid == "0" {
            return Err(ApiError::new(
                axum::http::StatusCode::FORBIDDEN,
                "Root is not allowed",
                "Development PTY sessions must run as a non-root operating-system user.",
            ));
        }
    }
    Ok(())
}

pub(super) fn current_uid() -> Result<String, ApiError> {
    let output = Command::new("id")
        .arg("-u")
        .output()
        .map_err(|_| ApiError::internal("Unable to verify the operating-system user."))?;
    let uid = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if !output.status.success() || uid.is_empty() {
        return Err(ApiError::internal(
            "Unable to verify the operating-system user.",
        ));
    }
    Ok(uid)
}

pub(super) fn request_id(headers: &HeaderMap) -> Uuid {
    headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| Uuid::parse_str(value).ok())
        .unwrap_or_else(Uuid::new_v4)
}

pub(super) fn reject_non_admin_origin(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(), ApiError> {
    if let Some(origin) = headers
        .get(axum::http::header::ORIGIN)
        .and_then(|value| value.to_str().ok())
    {
        if origin != state.config.admin_origin {
            return Err(ApiError::forbidden(
                "Development terminal authorization is accepted only from the configured Admin Web origin.",
            ));
        }
    }
    Ok(())
}
