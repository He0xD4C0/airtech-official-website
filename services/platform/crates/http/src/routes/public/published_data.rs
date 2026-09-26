use super::*;

pub(super) fn with_etag<T: serde::Serialize>(value: T, revision: i64) -> Response {
    let mut response = Json(value).into_response();
    response.headers_mut().insert(
        header::ETAG,
        HeaderValue::from_str(&etag(revision)).expect("revision ETag is valid"),
    );
    response
}

pub(super) fn parse_content_kind(value: &str) -> Result<CmsContentKind, ApiError> {
    match value {
        "home" => Ok(CmsContentKind::Home),
        "solutions" => Ok(CmsContentKind::Solution),
        "technology" => Ok(CmsContentKind::Technology),
        "articles" => Ok(CmsContentKind::Article),
        "news" => Ok(CmsContentKind::News),
        "faqs" => Ok(CmsContentKind::Faq),
        "case-studies" => Ok(CmsContentKind::CaseStudy),
        "downloads" => Ok(CmsContentKind::Download),
        "company" => Ok(CmsContentKind::Company),
        "legal" => Ok(CmsContentKind::Legal),
        _ => Err(ApiError::not_found("Content kind was not found.")),
    }
}
