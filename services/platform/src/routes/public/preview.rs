async fn get_content_preview(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let result = load_content_preview(&state, &headers).await;
    let mut response = match result {
        Ok(preview) => (StatusCode::OK, Json(preview)).into_response(),
        Err(error) => error.into_response(),
    };
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store, max-age=0"),
    );
    response
        .headers_mut()
        .insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    response.headers_mut().insert(
        "x-robots-tag",
        HeaderValue::from_static("noindex, nofollow, noarchive"),
    );
    response
}

async fn load_content_preview(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<ContentPreviewResponse, ApiError> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| !value.is_empty())
        .ok_or_else(content_preview_not_found)?;
    if token.len() > crate::preview_token::MAX_PREVIEW_TOKEN_LENGTH {
        return Err(content_preview_not_found());
    }
    let key = state
        .config
        .preview_signing_key
        .as_ref()
        .ok_or_else(content_preview_not_found)?;
    let verified = match crate::preview_token::verify(key, token) {
        Ok(verified) => verified,
        Err(PreviewTokenError::Expired) => {
            return Err(ApiError::gone("Content preview has expired."));
        }
        Err(PreviewTokenError::Invalid) => return Err(content_preview_not_found()),
    };
    if !crate::auth::preview_session_is_authorized(
        state,
        verified.admin_user_id,
        verified.admin_session_id,
    )
    .await?
    {
        return Err(content_preview_not_found());
    }
    let content = state
        .load_content_revision(verified.content_id, verified.revision)
        .await?
        .filter(|content| {
            content.id == verified.content_id && content.current_revision == verified.revision
        })
        .ok_or_else(content_preview_not_found)?;
    Ok(ContentPreviewResponse {
        content,
        preview_expires_at: verified.expires_at,
    })
}

fn content_preview_not_found() -> ApiError {
    ApiError::not_found("Content preview was not found.")
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiscoveryDocument {
    generated_at: chrono::DateTime<Utc>,
    entries: Vec<DiscoveryEntry>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DiscoveryEntry {
    entity_type: &'static str,
    entity_id: Uuid,
    path: String,
    locale: String,
    title: String,
    summary: Option<String>,
    updated_at: chrono::DateTime<Utc>,
}
