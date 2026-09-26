use super::*;

pub(super) async fn load_published_news(
    state: &AppState,
    locale: &str,
    category: Option<&str>,
    slug: Option<&str>,
    after: Option<(DateTime<Utc>, Uuid)>,
    limit: usize,
) -> Result<Vec<NewsEntry>, ApiError> {
    load_v2_news(state, locale, category, slug, after, limit).await
}

pub(super) async fn resolve_legacy_news_cursor(
    state: &AppState,
    locale: &str,
    category: Option<&str>,
    id: Uuid,
) -> Result<Option<DateTime<Utc>>, ApiError> {
    load_v2_news_cursor_position(state, locale, category, id).await
}
