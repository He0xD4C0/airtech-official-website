use super::*;

pub(super) use crate::{
    models::{
        CmsContentKind, CmsDraftPage, CmsDraftSharesRequest, CmsPrivateDraft, CmsPublishResult,
        CmsPublishedContent, CmsPublishedPage, CmsRejectRequest, CmsReviewPage,
        CmsSiteSingletonState, CmsSubmitResult,
    },
    services::cms_workflow::{self, CmsListQuery},
};

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ContentTemplateListPage {
    pub(super) items: Vec<crate::models::ContentTemplateDefinition>,
}

#[derive(Debug, serde::Deserialize)]
pub(super) struct SiteSingletonQuery {
    locale: String,
}

pub(super) async fn list_content_templates() -> Result<Json<ContentTemplateListPage>, ApiError> {
    Ok(Json(ContentTemplateListPage {
        items: crate::services::cms_templates::template_registry().to_vec(),
    }))
}

pub(super) async fn get_site_singleton(
    State(state): State<AppState>,
    Extension(principal): Extension<AdminPrincipal>,
    Path(kind): Path<CmsContentKind>,
    Query(query): Query<SiteSingletonQuery>,
) -> Result<Json<CmsSiteSingletonState>, ApiError> {
    Ok(Json(
        cms_workflow::get_site_singleton(&state, &principal, kind, &query.locale).await?,
    ))
}

pub(super) async fn list_private_drafts(
    State(state): State<AppState>,
    Extension(principal): Extension<AdminPrincipal>,
    Query(query): Query<CmsListQuery>,
) -> Result<Json<CmsDraftPage>, ApiError> {
    Ok(Json(
        cms_workflow::list_drafts(&state, &principal, query).await?,
    ))
}

pub(super) async fn create_private_draft(
    State(state): State<AppState>,
    Extension(principal): Extension<AdminPrincipal>,
    Json(document): Json<ContentDraftV2>,
) -> Result<Response, ApiError> {
    let draft = cms_workflow::create_draft(&state, &principal, document).await?;
    Ok(private_draft_response(StatusCode::CREATED, &draft))
}

pub(super) async fn get_private_draft(
    State(state): State<AppState>,
    Extension(principal): Extension<AdminPrincipal>,
    Path(draft_id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let draft = cms_workflow::get_draft(&state, &principal, draft_id).await?;
    Ok(private_draft_response(StatusCode::OK, &draft))
}

pub(super) async fn save_private_draft(
    State(state): State<AppState>,
    Extension(principal): Extension<AdminPrincipal>,
    Path(draft_id): Path<Uuid>,
    headers: HeaderMap,
    Json(document): Json<ContentDraftV2>,
) -> Result<Response, ApiError> {
    let version = parse_content_if_match(&headers)?;
    let draft = cms_workflow::save_draft(&state, &principal, draft_id, version, document).await?;
    Ok(private_draft_response(StatusCode::OK, &draft))
}

pub(super) async fn set_private_draft_shares(
    State(state): State<AppState>,
    Extension(principal): Extension<AdminPrincipal>,
    Path(draft_id): Path<Uuid>,
    Json(request): Json<CmsDraftSharesRequest>,
) -> Result<Response, ApiError> {
    let draft =
        cms_workflow::set_draft_shares(&state, &principal, draft_id, request.user_ids).await?;
    Ok(private_draft_response(StatusCode::OK, &draft))
}

pub(super) async fn claim_private_draft(
    State(state): State<AppState>,
    Extension(principal): Extension<AdminPrincipal>,
    Path(draft_id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let draft = cms_workflow::claim_unassigned_draft(&state, &principal, draft_id).await?;
    Ok(private_draft_response(StatusCode::OK, &draft))
}

pub(super) async fn submit_private_draft(
    State(state): State<AppState>,
    Extension(principal): Extension<AdminPrincipal>,
    Path(draft_id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<CmsSubmitResult>, ApiError> {
    let version = parse_content_if_match(&headers)?;
    Ok(Json(
        cms_workflow::submit_draft(&state, &principal, draft_id, version).await?,
    ))
}

pub(super) async fn withdraw_private_draft(
    State(state): State<AppState>,
    Extension(principal): Extension<AdminPrincipal>,
    Path(draft_id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let draft = cms_workflow::withdraw_draft(&state, &principal, draft_id).await?;
    Ok(private_draft_response(StatusCode::OK, &draft))
}

pub(super) async fn list_content_reviews(
    State(state): State<AppState>,
    Query(query): Query<CmsListQuery>,
) -> Result<Json<CmsReviewPage>, ApiError> {
    Ok(Json(cms_workflow::list_reviews(&state, query).await?))
}

pub(super) async fn approve_content_review(
    State(state): State<AppState>,
    Extension(principal): Extension<AdminPrincipal>,
    Path(draft_id): Path<Uuid>,
) -> Result<Json<CmsPublishResult>, ApiError> {
    Ok(Json(
        cms_workflow::approve_draft(&state, &principal, draft_id).await?,
    ))
}

pub(super) async fn reject_content_review(
    State(state): State<AppState>,
    Extension(principal): Extension<AdminPrincipal>,
    Path(draft_id): Path<Uuid>,
    Json(request): Json<CmsRejectRequest>,
) -> Result<Response, ApiError> {
    let draft = cms_workflow::reject_draft(&state, &principal, draft_id, request.reason).await?;
    Ok(private_draft_response(StatusCode::OK, &draft))
}

pub(super) async fn list_published_content(
    State(state): State<AppState>,
    Query(query): Query<CmsListQuery>,
) -> Result<Json<CmsPublishedPage>, ApiError> {
    Ok(Json(cms_workflow::list_published(&state, query).await?))
}

pub(super) async fn get_published_content(
    State(state): State<AppState>,
    Path(content_id): Path<Uuid>,
) -> Result<Json<CmsPublishedContent>, ApiError> {
    Ok(Json(cms_workflow::get_published(&state, content_id).await?))
}

pub(super) async fn copy_published_content_to_draft(
    State(state): State<AppState>,
    Extension(principal): Extension<AdminPrincipal>,
    Path(content_id): Path<Uuid>,
) -> Result<Response, ApiError> {
    let draft = cms_workflow::copy_published_to_draft(&state, &principal, content_id).await?;
    Ok(private_draft_response(StatusCode::CREATED, &draft))
}

pub(super) fn private_draft_response(status: StatusCode, draft: &CmsPrivateDraft) -> Response {
    let mut response = (status, Json(draft)).into_response();
    if let Ok(value) = HeaderValue::from_str(&cms_content::draft_etag(draft.draft_version)) {
        response.headers_mut().insert(header::ETAG, value);
    }
    add_private_no_store_headers(&mut response);
    response
}
