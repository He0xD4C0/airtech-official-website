use std::{sync::Arc, time::Duration};

use reqwest::{header, Method, StatusCode};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{Map, Value};
use tokio::sync::Mutex;

use crate::config::FeishuCredentials;
use crate::error::ApiError;

const MAX_ATTEMPTS: usize = 4;

#[path = "client_download.rs"]
mod download;
#[cfg(test)]
use download::validate_attachment_length;
pub use download::{AssetProbe, DownloadedAsset};

#[derive(Clone)]
pub struct FeishuClient {
    http: reqwest::Client,
    base_url: String,
    credentials: FeishuCredentials,
    token: Arc<Mutex<Option<CachedToken>>>,
}

#[derive(Clone)]
struct CachedToken {
    value: String,
    expires_at: std::time::Instant,
}

impl Drop for CachedToken {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.value.zeroize();
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct FeishuField {
    pub field_id: String,
    pub field_name: String,
    #[serde(rename = "type")]
    pub field_type: i32,
    #[serde(default)]
    pub is_primary: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct FeishuRecord {
    pub record_id: String,
    #[serde(default)]
    pub fields: Map<String, Value>,
    #[serde(default)]
    pub created_time: Option<String>,
    #[serde(default)]
    pub last_modified_time: Option<String>,
}

#[derive(Clone, Debug)]
pub struct FeishuRecordPage {
    pub items: Vec<FeishuRecord>,
    pub next_page_token: Option<String>,
    pub has_more: bool,
}

#[derive(Deserialize)]
struct TokenResponse {
    code: i32,
    #[allow(dead_code)]
    msg: String,
    tenant_access_token: Option<String>,
    expire: Option<u64>,
}

#[derive(Deserialize)]
struct Envelope<T> {
    code: i32,
    #[allow(dead_code)]
    msg: String,
    data: Option<T>,
}

#[derive(Deserialize)]
struct WikiData {
    node: WikiNode,
}

#[derive(Deserialize)]
struct WikiNode {
    obj_token: String,
}

#[derive(Deserialize)]
struct FieldPage {
    #[serde(default)]
    items: Vec<FeishuField>,
    #[serde(default)]
    has_more: bool,
    page_token: Option<String>,
}

#[derive(Deserialize)]
struct RecordPage {
    #[serde(default)]
    items: Vec<FeishuRecord>,
    #[serde(default)]
    has_more: bool,
    page_token: Option<String>,
}

impl FeishuClient {
    pub fn new(base_url: &str, credentials: FeishuCredentials) -> Self {
        Self::with_timeout(base_url, credentials, Duration::from_secs(120))
    }

    fn with_timeout(
        base_url: &str,
        credentials: FeishuCredentials,
        request_timeout: Duration,
    ) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .timeout(request_timeout)
            .user_agent("AIRTEKPOWER-Feishu-Sync/1")
            .build()
            .expect("static Feishu HTTP client configuration is valid");
        Self {
            http,
            base_url: base_url.trim_end_matches('/').to_owned(),
            credentials,
            token: Arc::new(Mutex::new(None)),
        }
    }

    #[cfg(test)]
    pub(super) fn new_with_timeout(
        base_url: &str,
        credentials: FeishuCredentials,
        request_timeout: Duration,
    ) -> Self {
        Self::with_timeout(base_url, credentials, request_timeout)
    }

    pub async fn test_token(&self) -> Result<(), ApiError> {
        self.tenant_token(false).await.map(|_| ())
    }

    pub async fn resolve_app_token(&self, wiki_token: &str) -> Result<String, ApiError> {
        let data: WikiData = self
            .get_json(
                "/open-apis/wiki/v2/spaces/get_node",
                &[("token", wiki_token)],
            )
            .await?;
        non_empty(
            data.node.obj_token,
            "Feishu wiki node has no Bitable token.",
        )
    }

    pub async fn list_fields(
        &self,
        app_token: &str,
        table_id: &str,
    ) -> Result<Vec<FeishuField>, ApiError> {
        let path = format!("/open-apis/bitable/v1/apps/{app_token}/tables/{table_id}/fields");
        let mut fields = Vec::new();
        let mut page_token: Option<String> = None;
        loop {
            let mut query = vec![("page_size", "100".to_owned())];
            if let Some(value) = page_token.as_ref() {
                query.push(("page_token", value.clone()));
            }
            let refs = query
                .iter()
                .map(|(name, value)| (*name, value.as_str()))
                .collect::<Vec<_>>();
            let page: FieldPage = self.get_json(&path, &refs).await?;
            fields.extend(page.items);
            if !page.has_more {
                break;
            }
            page_token = Some(non_empty(
                page.page_token.unwrap_or_default(),
                "Feishu field pagination token is missing.",
            )?);
        }
        Ok(fields)
    }

    pub async fn list_records_page(
        &self,
        app_token: &str,
        table_id: &str,
        page_token: Option<&str>,
    ) -> Result<FeishuRecordPage, ApiError> {
        let path = format!("/open-apis/bitable/v1/apps/{app_token}/tables/{table_id}/records");
        let mut query = vec![("page_size", "500")];
        if let Some(value) = page_token {
            query.push(("page_token", value));
        }
        let page: RecordPage = self.get_json(&path, &query).await?;
        Ok(FeishuRecordPage {
            items: page.items,
            next_page_token: page.page_token,
            has_more: page.has_more,
        })
    }

    pub async fn probe_records(&self, app_token: &str, table_id: &str) -> Result<(), ApiError> {
        let path = format!("/open-apis/bitable/v1/apps/{app_token}/tables/{table_id}/records");
        self.get_json::<RecordPage>(&path, &[("page_size", "1")])
            .await
            .map(|_| ())
    }

    pub async fn download_asset(
        &self,
        file_token: &str,
        permission_extra: Option<&str>,
    ) -> Result<DownloadedAsset, ApiError> {
        let path = format!("/open-apis/drive/v1/medias/{file_token}/download");
        let query = permission_extra
            .map(|extra| vec![("extra", extra)])
            .unwrap_or_default();
        let response = self.authorized_response(Method::GET, &path, &query).await?;
        download::stage_asset(response).await
    }

    async fn get_json<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<T, ApiError> {
        for attempt in 0..MAX_ATTEMPTS {
            let response = self.authorized_response(Method::GET, path, query).await?;
            let envelope: Envelope<T> = response.json().await.map_err(|error| {
                tracing::warn!(%error, "Feishu response JSON was invalid");
                ApiError::service_unavailable("Feishu returned an invalid response.")
            })?;
            if envelope.code == 0 {
                return envelope.data.ok_or_else(|| {
                    ApiError::service_unavailable("Feishu response data is missing.")
                });
            }
            tracing::warn!(code = envelope.code, "Feishu API rejected a request");
            if transient_application_code(envelope.code) {
                if attempt + 1 < MAX_ATTEMPTS {
                    tokio::time::sleep(backoff(attempt)).await;
                    continue;
                }
                return Err(ApiError::service_unavailable(format!(
                    "Feishu retry budget was exhausted for code {}.",
                    envelope.code
                )));
            }
            return Err(ApiError::conflict(format!(
                "Feishu API rejected the request with code {}.",
                envelope.code
            )));
        }
        Err(ApiError::service_unavailable(
            "Feishu retry budget was exhausted.",
        ))
    }

    async fn authorized_response(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<reqwest::Response, ApiError> {
        let url = format!("{}{path}", self.base_url);
        let mut refreshed = false;
        for attempt in 0..MAX_ATTEMPTS {
            let token = self.tenant_token(false).await?;
            let result = self
                .http
                .request(method.clone(), &url)
                .query(query)
                .bearer_auth(token)
                .send()
                .await;
            let response = match result {
                Ok(response) => response,
                Err(error) if attempt + 1 < MAX_ATTEMPTS => {
                    tracing::warn!(attempt, %error, "Feishu network request will be retried");
                    tokio::time::sleep(backoff(attempt)).await;
                    continue;
                }
                Err(error) => return Err(network_error(error)),
            };
            if response.status() == StatusCode::UNAUTHORIZED && !refreshed {
                self.clear_token().await;
                refreshed = true;
                continue;
            }
            if response.status() == StatusCode::TOO_MANY_REQUESTS && attempt + 1 < MAX_ATTEMPTS {
                let delay = retry_after(&response).unwrap_or_else(|| backoff(attempt));
                tokio::time::sleep(delay.min(Duration::from_secs(30))).await;
                continue;
            }
            if response.status().is_server_error() && attempt + 1 < MAX_ATTEMPTS {
                tokio::time::sleep(backoff(attempt)).await;
                continue;
            }
            if !response.status().is_success() {
                let status = response.status();
                tracing::warn!(%status, "Feishu HTTP request failed");
                return Err(feishu_http_error(status));
            }
            return Ok(response);
        }
        Err(ApiError::service_unavailable(
            "Feishu retry budget was exhausted.",
        ))
    }

    async fn tenant_token(&self, force_refresh: bool) -> Result<String, ApiError> {
        let mut cached = self.token.lock().await;
        if !force_refresh {
            if let Some(token) = cached.as_ref() {
                if token.expires_at > std::time::Instant::now() {
                    return Ok(token.value.clone());
                }
            }
        }
        let credentials = self.credentials.clone();
        let response = self.request_token(&credentials).await?;
        if response.code != 0 {
            tracing::warn!(code = response.code, "Feishu token request was rejected");
            return Err(ApiError::service_unavailable(
                "Feishu credentials were rejected.",
            ));
        }
        let value = non_empty(
            response.tenant_access_token.unwrap_or_default(),
            "Feishu token response is missing a token.",
        )?;
        let lifetime = response.expire.unwrap_or(7200).saturating_sub(60).max(30);
        *cached = Some(CachedToken {
            value: value.clone(),
            expires_at: std::time::Instant::now() + Duration::from_secs(lifetime),
        });
        Ok(value)
    }

    async fn request_token(
        &self,
        credentials: &FeishuCredentials,
    ) -> Result<TokenResponse, ApiError> {
        let url = format!(
            "{}/open-apis/auth/v3/tenant_access_token/internal",
            self.base_url
        );
        for attempt in 0..MAX_ATTEMPTS {
            let result = self
                .http
                .post(&url)
                .json(&serde_json::json!({
                    "app_id": credentials.app_id(),
                    "app_secret": credentials.app_secret()
                }))
                .send()
                .await;
            let response = match result {
                Ok(response) => response,
                Err(error) if attempt + 1 < MAX_ATTEMPTS => {
                    tracing::warn!(attempt, %error, "Feishu token request will be retried");
                    tokio::time::sleep(backoff(attempt)).await;
                    continue;
                }
                Err(error) => return Err(network_error(error)),
            };
            if response.status() == StatusCode::TOO_MANY_REQUESTS && attempt + 1 < MAX_ATTEMPTS {
                let delay = retry_after(&response).unwrap_or_else(|| backoff(attempt));
                tokio::time::sleep(delay.min(Duration::from_secs(30))).await;
                continue;
            }
            if response.status().is_server_error() && attempt + 1 < MAX_ATTEMPTS {
                tokio::time::sleep(backoff(attempt)).await;
                continue;
            }
            if !response.status().is_success() {
                return Err(ApiError::service_unavailable(format!(
                    "Feishu token endpoint returned HTTP {}.",
                    response.status().as_u16()
                )));
            }
            return response.json().await.map_err(network_error);
        }
        Err(ApiError::service_unavailable(
            "Feishu token retry budget was exhausted.",
        ))
    }

    async fn clear_token(&self) {
        *self.token.lock().await = None;
    }
}

#[cfg(not(test))]
fn backoff(attempt: usize) -> Duration {
    Duration::from_secs(1_u64 << attempt.min(4))
}

#[cfg(test)]
fn backoff(_attempt: usize) -> Duration {
    Duration::ZERO
}

fn retry_after(response: &reqwest::Response) -> Option<Duration> {
    response
        .headers()
        .get(header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
}

fn transient_application_code(code: i32) -> bool {
    code == 1_254_290
        || (1_255_001..=1_255_005).contains(&code)
        || matches!(code, 1_254_607 | 1_255_040)
}

fn network_error(error: reqwest::Error) -> ApiError {
    tracing::warn!(%error, "Feishu transport failed");
    ApiError::service_unavailable("Feishu is temporarily unavailable.")
}

fn feishu_http_error(status: StatusCode) -> ApiError {
    let detail = format!("Feishu returned HTTP {}.", status.as_u16());
    if matches!(
        status,
        StatusCode::UNAUTHORIZED | StatusCode::REQUEST_TIMEOUT | StatusCode::TOO_MANY_REQUESTS
    ) || status.is_server_error()
    {
        ApiError::service_unavailable(detail)
    } else {
        ApiError::conflict(detail)
    }
}

fn non_empty(value: String, detail: &str) -> Result<String, ApiError> {
    (!value.trim().is_empty())
        .then_some(value)
        .ok_or_else(|| ApiError::service_unavailable(detail))
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
