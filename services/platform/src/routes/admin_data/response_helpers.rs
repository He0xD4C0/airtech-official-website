fn validate_reason(reason: &str) -> Result<(), ApiError> {
    if reason.trim().len() < 10 || reason.len() > 1000 {
        Err(ApiError::bad_request(
            "reason must contain 10 to 1000 characters.",
        ))
    } else {
        Ok(())
    }
}

fn decode_publication_status(value: String) -> PublicationStatus {
    match value.as_str() {
        "scheduled" => PublicationStatus::Scheduled,
        "published" => PublicationStatus::Published,
        "archived" => PublicationStatus::Archived,
        _ => PublicationStatus::Draft,
    }
}

fn decode_json<T: serde::de::DeserializeOwned>(value: Value, entity: &str) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|error| {
        tracing::error!(%error, entity, "stored JSON payload is invalid");
        ApiError::service_unavailable(format!("Stored {entity} data is invalid."))
    })
}

fn entity_response<T: Serialize>(status: StatusCode, value: &T, revision: i64) -> Response {
    let mut response = (status, Json(value)).into_response();
    response.headers_mut().insert(
        header::ETAG,
        HeaderValue::from_str(&etag(revision)).expect("revision ETag is valid"),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store, max-age=0"),
    );
    response
}

fn request_id(headers: &HeaderMap) -> Uuid {
    headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| Uuid::parse_str(value).ok())
        .unwrap_or_else(Uuid::new_v4)
}

fn valid_locale(value: &str) -> bool {
    (2..=35).contains(&value.len())
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn default_locale() -> String {
    "en".into()
}
