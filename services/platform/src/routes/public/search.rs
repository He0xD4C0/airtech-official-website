use std::collections::BTreeMap;

use super::*;
use crate::models::{
    ProductFacetCount, PublicSearchEntityType, PublicSearchItem, PublicSearchPage,
    PublicSearchQuery, PublicSearchType,
};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchCursor {
    version: u8,
    path: String,
    id: Uuid,
    q: Option<String>,
    content_type: Option<PublicSearchType>,
}

pub(super) async fn search_public_site(
    State(state): State<AppState>,
    Query(query): Query<PublicSearchQuery>,
) -> Result<Json<PublicSearchPage>, ApiError> {
    let limit = query.limit.unwrap_or(20);
    if !(1..=100).contains(&limit) {
        return Err(ApiError::bad_request("limit must be between 1 and 100."));
    }
    let q = normalize_query(query.q.as_deref())?;
    let after = query
        .cursor
        .as_deref()
        .map(|value| decode_cursor(value, q.as_deref(), query.content_type))
        .transpose()?;
    let placeholder_shell =
        crate::routes::public_data::published_site_shell_has_placeholder(&state, "en").await?;
    let entries = if placeholder_shell {
        Vec::new()
    } else {
        crate::services::public_content::load_discovery_entries(&state.pool).await?
    };
    let tokens = q
        .as_deref()
        .map(|value| value.split_whitespace().collect::<Vec<_>>())
        .unwrap_or_default();
    let mut matched = entries
        .into_iter()
        .map(|entry| PublicSearchItem {
            entity_type: if entry.entity_type == "product" {
                PublicSearchEntityType::Product
            } else {
                PublicSearchEntityType::Content
            },
            entity_id: entry.entity_id,
            title: entry.title,
            summary: entry.summary,
            display_type: display_type(entry.entity_type, &entry.path),
            canonical_path: entry.path,
        })
        .filter(|entry| matches_tokens(entry, &tokens))
        .collect::<Vec<_>>();
    matched.sort_by(|left, right| {
        (&left.canonical_path, left.entity_id).cmp(&(&right.canonical_path, right.entity_id))
    });
    let type_counts = facet_counts(&matched);
    matched.retain(|entry| {
        query
            .content_type
            .is_none_or(|content_type| entry.display_type == content_type)
    });
    let total = matched.len();
    if let Some(after) = &after {
        matched.retain(|entry| {
            (entry.canonical_path.as_str(), entry.entity_id) > (after.path.as_str(), after.id)
        });
    }
    let has_more = matched.len() > limit;
    matched.truncate(limit);
    let next_cursor = has_more
        .then(|| matched.last())
        .flatten()
        .map(|entry| encode_cursor(entry, q.clone(), query.content_type))
        .transpose()?;
    Ok(Json(PublicSearchPage {
        items: matched,
        next_cursor,
        total,
        type_counts,
    }))
}

fn normalize_query(value: Option<&str>) -> Result<Option<String>, ApiError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return Ok(None);
    }
    if normalized.len() > 200 || normalized.chars().any(char::is_control) {
        return Err(ApiError::bad_request(
            "q must contain at most 200 printable characters.",
        ));
    }
    Ok(Some(normalized.to_lowercase()))
}

fn matches_tokens(entry: &PublicSearchItem, tokens: &[&str]) -> bool {
    if tokens.is_empty() {
        return true;
    }
    let haystack = format!(
        "{} {} {} {}",
        entry.title,
        entry.summary.as_deref().unwrap_or_default(),
        entry.canonical_path,
        entry.display_type.label()
    )
    .to_lowercase();
    tokens.iter().all(|token| haystack.contains(token))
}

fn display_type(entity_type: &str, path: &str) -> PublicSearchType {
    if entity_type == "product" {
        return PublicSearchType::Product;
    }
    for (prefix, content_type) in [
        ("/en/solutions/", PublicSearchType::Solution),
        ("/en/technology/", PublicSearchType::Technology),
        ("/en/resources/articles/", PublicSearchType::Article),
        ("/en/resources/news/", PublicSearchType::News),
        ("/en/resources/faqs/", PublicSearchType::Faq),
        ("/en/resources/case-studies/", PublicSearchType::CaseStudy),
        ("/en/resources/downloads/", PublicSearchType::Download),
        ("/en/company/", PublicSearchType::Company),
    ] {
        if path.starts_with(prefix) {
            return content_type;
        }
    }
    PublicSearchType::Page
}

fn facet_counts(entries: &[PublicSearchItem]) -> Vec<ProductFacetCount> {
    let mut counts = BTreeMap::new();
    for entry in entries {
        *counts.entry(entry.display_type).or_insert(0usize) += 1;
    }
    counts
        .into_iter()
        .map(|(value, count)| ProductFacetCount {
            value: value.label().into(),
            count,
        })
        .collect()
}

fn encode_cursor(
    entry: &PublicSearchItem,
    q: Option<String>,
    content_type: Option<PublicSearchType>,
) -> Result<String, ApiError> {
    serde_json::to_vec(&SearchCursor {
        version: 1,
        path: entry.canonical_path.clone(),
        id: entry.entity_id,
        q,
        content_type,
    })
    .map(|value| URL_SAFE_NO_PAD.encode(value))
    .map_err(|_| ApiError::internal("Search cursor serialization failed."))
}

fn decode_cursor(
    value: &str,
    q: Option<&str>,
    content_type: Option<PublicSearchType>,
) -> Result<SearchCursor, ApiError> {
    if value.is_empty() || value.len() > 2_048 {
        return Err(ApiError::bad_request("cursor is invalid."));
    }
    URL_SAFE_NO_PAD
        .decode(value)
        .ok()
        .and_then(|value| serde_json::from_slice::<SearchCursor>(&value).ok())
        .filter(|cursor| {
            cursor.version == 1
                && cursor.path.starts_with("/en/")
                && cursor.path.len() <= 2_048
                && cursor.q.as_deref() == q
                && cursor.content_type == content_type
        })
        .ok_or_else(|| {
            ApiError::bad_request("cursor is invalid or belongs to a different search query.")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_cursor_is_bound_to_normalized_query_and_type() {
        let entry = PublicSearchItem {
            entity_type: PublicSearchEntityType::Product,
            entity_id: Uuid::new_v4(),
            title: "Published model".into(),
            summary: None,
            canonical_path: "/en/products/axial/published-model".into(),
            display_type: PublicSearchType::Product,
        };
        let cursor = encode_cursor(
            &entry,
            Some("published model".into()),
            Some(PublicSearchType::Product),
        )
        .unwrap();
        assert!(decode_cursor(
            &cursor,
            Some("published model"),
            Some(PublicSearchType::Product)
        )
        .is_ok());
        assert!(
            decode_cursor(&cursor, Some("different"), Some(PublicSearchType::Product)).is_err()
        );
    }
}
