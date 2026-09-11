async fn load_published_news(
    state: &AppState,
    locale: &str,
    category: Option<&str>,
) -> Result<Vec<NewsEntry>, ApiError> {
    load_v2_news(state, locale, category).await
}
