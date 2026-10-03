use super::*;
use airtek_domain::models::SourceMetadataKind;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SourceMetadataQuery {
    kind: Option<String>,
    q: Option<String>,
    cursor: Option<String>,
    limit: Option<usize>,
}

pub(super) async fn list_source_metadata(
    State(state): State<AppState>,
    Query(query): Query<SourceMetadataQuery>,
) -> Result<Json<CursorPage<airtek_domain::models::ProductSourceMetadata>>, ApiError> {
    let kind = query
        .kind
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            SourceMetadataKind::parse(value)
                .ok_or_else(|| ApiError::bad_request("kind must be supplier or brand."))
        })
        .transpose()?;
    let result = airtek_runtime::services::admin_source_metadata::list_source_metadata(
        &state,
        kind,
        query.q,
        CursorQuery {
            cursor: query.cursor,
            limit: query.limit,
        },
    )
    .await?;
    Ok(Json(result))
}
