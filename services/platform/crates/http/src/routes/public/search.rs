use super::*;
use airtek_domain::models::{PublicSearchPage, PublicSearchQuery};

pub(super) async fn search_public_site(
    State(state): State<AppState>,
    Query(query): Query<PublicSearchQuery>,
) -> Result<Json<PublicSearchPage>, ApiError> {
    crate::routes::public_data::published_site_shell_has_placeholder(&state, "en").await?;
    Ok(Json(
        airtek_runtime::services::public_content::search_public_site(&state.pool, query).await?,
    ))
}
