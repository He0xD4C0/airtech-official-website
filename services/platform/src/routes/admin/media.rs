use axum::body::Bytes;
use crate::services::media;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MediaReviewRequest {
    status: String,
    reason: String,
}

impl MediaReviewRequest {
    fn validated_reason(&self) -> Result<&str, ApiError> {
        let reason = self.reason.trim();
        if reason.is_empty() {
            return Err(ApiError::bad_request(
                "A non-empty reason is required for every human media review decision.",
            ));
        }
        if reason.chars().count() > 500 {
            return Err(ApiError::bad_request(
                "Media review reasons must be 500 characters or fewer.",
            ));
        }
        Ok(reason)
    }
}

async fn upload_media_asset(
    State(state): State<AppState>,
    Extension(principal): Extension<AdminPrincipal>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    require_media_write(&principal)?;
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let asset = media::upload_media_asset(
        &state,
        &principal.email,
        Uuid::new_v4(),
        content_type.as_deref(),
        &body,
    )
    .await?;
    Ok(crate::routes::accepted(StatusCode::CREATED, asset))
}

async fn review_media_asset(
    State(state): State<AppState>,
    Extension(principal): Extension<AdminPrincipal>,
    Path(asset_id): Path<Uuid>,
    payload: Result<Json<MediaReviewRequest>, JsonRejection>,
) -> Result<Json<MediaAssetSummary>, ApiError> {
    require_media_write(&principal)?;
    let Json(request) = payload.map_err(|_| {
        ApiError::bad_request(
            "Media review decisions require a status and a non-empty human review reason.",
        )
    })?;
    let reason = request.validated_reason()?;
    let asset = media::review_media_asset(
        &state,
        &principal.email,
        Uuid::new_v4(),
        asset_id,
        &request.status,
        Some(reason),
    )
    .await?;
    Ok(Json(asset))
}

fn require_media_write(principal: &AdminPrincipal) -> Result<(), ApiError> {
    if principal.has_permission("media.write") {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "The `media.write` permission is required.",
        ))
    }
}

#[cfg(test)]
mod media_review_request_tests {
    use super::*;

    #[test]
    fn clean_and_quarantine_requests_require_a_trimmed_non_empty_reason() {
        assert!(serde_json::from_str::<MediaReviewRequest>(r#"{"status":"clean"}"#).is_err());
        for status in ["clean", "quarantined"] {
            let blank = MediaReviewRequest {
                status: status.to_owned(),
                reason: " \n\t ".to_owned(),
            };
            assert!(blank.validated_reason().is_err());

            let valid = MediaReviewRequest {
                status: status.to_owned(),
                reason: "  Human review evidence  ".to_owned(),
            };
            assert_eq!(valid.validated_reason().unwrap(), "Human review evidence");
        }
    }
}
