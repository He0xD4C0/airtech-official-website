use super::*;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct DiscoveryDocument {
    pub(super) generated_at: chrono::DateTime<Utc>,
    pub(super) entries: Vec<airtek_runtime::services::public_content::DiscoveryEntry>,
}

pub(super) async fn discovery(
    State(state): State<AppState>,
) -> Result<Json<DiscoveryDocument>, ApiError> {
    if crate::routes::public_data::published_site_shell_has_placeholder(&state, "en").await? {
        return Ok(Json(DiscoveryDocument {
            generated_at: Utc::now(),
            entries: Vec::new(),
        }));
    }
    let entries =
        airtek_runtime::services::public_content::load_discovery_entries(&state.pool).await?;
    Ok(Json(DiscoveryDocument {
        generated_at: Utc::now(),
        entries,
    }))
}

#[derive(Debug, Deserialize)]
pub(super) struct ContentQuery {
    #[serde(default = "default_locale")]
    pub(super) locale: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct ProductDetailQuery {
    pub(super) family: Option<ProductFamily>,
}
