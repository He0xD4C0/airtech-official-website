async fn create_content_preview(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Extension(principal): Extension<AdminPrincipal>,
    headers: HeaderMap,
    Json(request): Json<CreateContentPreviewRequest>,
) -> Result<Response, ApiError> {
    let expires_in_seconds = request
        .expires_in_seconds
        .unwrap_or(crate::preview_token::MAX_PREVIEW_TTL_SECONDS);
    if request.revision < 1 {
        return Err(ApiError::bad_request("revision must be at least 1."));
    }
    if !(1..=crate::preview_token::MAX_PREVIEW_TTL_SECONDS).contains(&expires_in_seconds) {
        return Err(ApiError::bad_request(format!(
            "expiresInSeconds must contain 1 to {}.",
            crate::preview_token::MAX_PREVIEW_TTL_SECONDS
        )));
    }

    let current = state
        .load_working_content(id)
        .await?
        .ok_or_else(|| ApiError::not_found("Content entry was not found."))?;
    if headers.contains_key(header::IF_MATCH) {
        let expected_revision = parse_if_match(&headers)?;
        if expected_revision != current.current_revision {
            return Err(ApiError::conflict(
                "The content entry changed; reload before creating a preview.",
            ));
        }
    }

    let existing_snapshot = state.load_content_revision(id, request.revision).await?;
    if existing_snapshot.is_none() && request.revision == current.current_revision {
        state
            .snapshot_working_content(&current, &actor(&headers))
            .await?;
    } else if existing_snapshot.is_none() {
        return Err(ApiError::not_found(
            "The requested content revision was not found.",
        ));
    }
    let key = state.config.preview_signing_key.as_ref().ok_or_else(|| {
        ApiError::service_unavailable("Content preview signing is not configured.")
    })?;
    let issued = crate::preview_token::issue(
        key,
        id,
        request.revision,
        principal.user_id,
        principal.session_id,
        expires_in_seconds,
    )?;
    let preview = ContentPreviewLink {
        url: format!(
            "{}/en/preview?token={}",
            state.config.public_origin, issued.token
        ),
        content_id: id,
        revision: request.revision,
        issued_at: issued.issued_at,
        expires_at: issued.expires_at,
    };

    let actor = actor(&headers);
    audit(
        &state,
        &headers,
        &actor,
        "content.preview.issue",
        "content",
        Some(id),
        None,
        Some(json!({
            "revision": preview.revision,
            "issuedAt": preview.issued_at,
            "expiresAt": preview.expires_at,
        })),
        Some("Create a short-lived content preview link".into()),
    )
    .await?;

    let mut response = (StatusCode::CREATED, Json(preview.clone())).into_response();
    response.headers_mut().insert(
        header::LOCATION,
        HeaderValue::from_str(&preview.url)
            .map_err(|_| ApiError::internal("Preview URL is not a valid Location header."))?,
    );
    add_private_no_store_headers(&mut response);
    Ok(response)
}
