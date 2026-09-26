use super::*;

pub(super) async fn published_motor_technologies(
    state: &AppState,
    locale: &str,
) -> Result<Vec<String>, ApiError> {
    airtek_runtime::services::public_content::published_motor_technologies(&state.pool, locale)
        .await
}
