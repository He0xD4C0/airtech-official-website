use crate::{config::FeishuCredentials, error::ApiError, state::AppState};

use super::{FeishuClient, CONNECTOR_ID};

const OFFICIAL_API_BASE_URL: &str = "https://open.feishu.cn";

pub async fn credentials_configured(state: &AppState) -> Result<bool, ApiError> {
    Ok(sqlx::query_scalar::<_, bool>(
        r#"SELECT app_id IS NOT NULL AND app_secret IS NOT NULL
           FROM feishu_connector_settings WHERE connector_id=$1"#,
    )
    .bind(CONNECTOR_ID)
    .fetch_optional(&state.pool)
    .await?
    .unwrap_or(false))
}

pub async fn load_client(state: &AppState) -> Result<FeishuClient, ApiError> {
    load_client_with_base_url(state, OFFICIAL_API_BASE_URL).await
}

pub(crate) async fn load_client_with_base_url(
    state: &AppState,
    base_url: &str,
) -> Result<FeishuClient, ApiError> {
    let credentials = sqlx::query_as::<_, (String, String)>(
        r#"SELECT app_id,app_secret FROM feishu_connector_settings
           WHERE connector_id=$1 AND app_id IS NOT NULL AND app_secret IS NOT NULL"#,
    )
    .bind(CONNECTOR_ID)
    .fetch_optional(&state.pool)
    .await?
    .map(|(app_id, app_secret)| FeishuCredentials::new(app_id, app_secret))
    .ok_or_else(|| ApiError::service_unavailable("Feishu credentials are not configured."))?;
    Ok(FeishuClient::new(base_url, credentials))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credentials_remain_redacted_in_debug_output() {
        let credentials = FeishuCredentials::new("cli_app".into(), "secret-value".into());
        assert_eq!(format!("{credentials:?}"), "FeishuCredentials([redacted])");
    }
}
