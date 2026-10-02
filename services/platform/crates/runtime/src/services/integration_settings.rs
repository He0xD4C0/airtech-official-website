//! Administrator-managed outbound integration settings.
//!
//! Secrets live in PostgreSQL as plaintext by explicit owner decision, are
//! never returned by an API and are never copied into audit records or logs.

use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::error::ApiError;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MailSettings {
    pub configured: bool,
    pub host: String,
    pub port: i32,
    pub protocol: String,
    pub username: String,
    pub from_address: String,
    pub from_name: String,
    pub revision: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateMailSettings {
    pub host: String,
    pub port: i32,
    pub protocol: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub from_address: String,
    pub from_name: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SmsSettings {
    pub configured: bool,
    pub provider: String,
    pub access_key_id: String,
    pub sign_name: String,
    pub template_code: String,
    pub region: String,
    pub revision: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateSmsSettings {
    pub provider: String,
    pub access_key_id: Option<String>,
    pub access_key_secret: Option<String>,
    pub sign_name: String,
    pub template_code: String,
    pub region: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptchaSettings {
    pub configured: bool,
    pub provider: String,
    pub site_key: String,
    pub revision: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateCaptchaSettings {
    pub provider: String,
    pub site_key: String,
    pub secret_key: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct MailTransport {
    pub host: String,
    pub port: i32,
    pub protocol: String,
    pub username: String,
    pub password: String,
    pub from_address: String,
    pub from_name: String,
}

#[derive(Debug, Clone)]
pub struct SmsTransport {
    pub access_key_id: String,
    pub access_key_secret: String,
    pub sign_name: String,
    pub template_code: String,
    pub region: String,
}

#[derive(Debug, Clone)]
pub struct CaptchaTransport {
    pub provider: String,
    pub site_key: String,
    pub secret_key: String,
}

fn invalid(message: &str) -> ApiError {
    ApiError::bad_request(message.to_owned())
}

pub async fn get_mail(state: &AppState) -> Result<MailSettings, ApiError> {
    let row = sqlx::query(
        "SELECT host, port, protocol, username, from_address, from_name, revision
           FROM mail_settings WHERE singleton=true",
    )
    .fetch_optional(&state.pool)
    .await?;
    Ok(match row {
        Some(row) => MailSettings {
            configured: true,
            host: row.try_get("host")?,
            port: row.try_get("port")?,
            protocol: row.try_get("protocol")?,
            username: row.try_get("username")?,
            from_address: row.try_get("from_address")?,
            from_name: row.try_get("from_name")?,
            revision: row.try_get("revision")?,
        },
        None => MailSettings {
            configured: false,
            host: String::new(),
            port: 587,
            protocol: "starttls".into(),
            username: String::new(),
            from_address: String::new(),
            from_name: String::new(),
            revision: 0,
        },
    })
}

pub async fn update_mail(
    state: &AppState,
    actor: &str,
    expected: i64,
    input: &UpdateMailSettings,
) -> Result<MailSettings, ApiError> {
    let host = input.host.trim().to_owned();
    if host.is_empty() || host.len() > 255 || input.port < 1 || input.port > 65535 {
        return Err(invalid("host and port are invalid."));
    }
    if !matches!(input.protocol.as_str(), "starttls" | "tls") {
        return Err(invalid(
            "protocol must be starttls or tls; unencrypted SMTP authentication is refused.",
        ));
    }
    let username = input.username.clone().unwrap_or_default();
    let from_address = input.from_address.trim().to_owned();
    if from_address.len() < 3
        || from_address.len() > 320
        || !from_address.contains('@')
        || from_address.chars().any(char::is_whitespace)
    {
        return Err(invalid("fromAddress must be a valid email address."));
    }
    let mut transaction = state.pool.begin().await?;
    let current: Option<i64> =
        sqlx::query_scalar("SELECT revision FROM mail_settings WHERE singleton=true FOR UPDATE")
            .fetch_optional(&mut *transaction)
            .await?;
    if current.unwrap_or(0) != expected {
        return Err(ApiError::conflict(
            "The email settings changed; reload before saving.",
        ));
    }
    let password = match current {
        Some(_) => match &input.password {
            Some(password) if !password.is_empty() => password.clone(),
            _ => {
                sqlx::query_scalar::<_, String>(
                    "SELECT password FROM mail_settings WHERE singleton=true",
                )
                .fetch_one(&mut *transaction)
                .await?
            }
        },
        None => input.password.clone().unwrap_or_default(),
    };
    if username.is_empty() != password.is_empty() {
        return Err(invalid(
            "username and password must be supplied or cleared together.",
        ));
    }
    sqlx::query(
        r#"INSERT INTO mail_settings
             (singleton, host, port, protocol, username, password, from_address, from_name, updated_by)
           VALUES (true,$1,$2,$3,$4,$5,$6,$7,$8)
           ON CONFLICT (singleton) DO UPDATE SET
             host=EXCLUDED.host, port=EXCLUDED.port, protocol=EXCLUDED.protocol,
             username=EXCLUDED.username, password=EXCLUDED.password,
             from_address=EXCLUDED.from_address, from_name=EXCLUDED.from_name,
             revision=mail_settings.revision+1, updated_at=now(), updated_by=EXCLUDED.updated_by"#,
    )
    .bind(&host)
    .bind(input.port)
    .bind(&input.protocol)
    .bind(&username)
    .bind(&password)
    .bind(&from_address)
    .bind(input.from_name.trim())
    .bind(actor)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    get_mail(state).await
}

pub async fn load_mail(state: &AppState) -> Result<Option<MailTransport>, ApiError> {
    let row = sqlx::query(
        "SELECT host, port, protocol, username, password, from_address, from_name
           FROM mail_settings WHERE singleton=true",
    )
    .fetch_optional(&state.pool)
    .await?;
    Ok(match row {
        Some(row) => Some(MailTransport {
            host: row.try_get("host")?,
            port: row.try_get("port")?,
            protocol: row.try_get("protocol")?,
            username: row.try_get("username")?,
            password: row.try_get("password")?,
            from_address: row.try_get("from_address")?,
            from_name: row.try_get("from_name")?,
        }),
        None => None,
    })
}

pub async fn get_sms(state: &AppState) -> Result<SmsSettings, ApiError> {
    let row = sqlx::query(
        "SELECT provider, access_key_id, sign_name, template_code, region, revision
           FROM sms_settings WHERE singleton=true",
    )
    .fetch_optional(&state.pool)
    .await?;
    Ok(match row {
        Some(row) => SmsSettings {
            configured: true,
            provider: row.try_get("provider")?,
            access_key_id: row.try_get("access_key_id")?,
            sign_name: row.try_get("sign_name")?,
            template_code: row.try_get("template_code")?,
            region: row.try_get("region")?,
            revision: row.try_get("revision")?,
        },
        None => SmsSettings {
            configured: false,
            provider: "aliyun".into(),
            access_key_id: String::new(),
            sign_name: String::new(),
            template_code: String::new(),
            region: "cn-hangzhou".into(),
            revision: 0,
        },
    })
}

pub async fn update_sms(
    state: &AppState,
    actor: &str,
    expected: i64,
    input: &UpdateSmsSettings,
) -> Result<SmsSettings, ApiError> {
    if input.provider != "aliyun" {
        return Err(invalid("provider must be aliyun."));
    }
    let sign_name = input.sign_name.trim();
    let template_code = input.template_code.trim();
    let region = input.region.trim();
    if sign_name.is_empty() || template_code.is_empty() || region.is_empty() {
        return Err(invalid(
            "signName, templateCode and region must not be empty.",
        ));
    }
    let mut transaction = state.pool.begin().await?;
    let existing = sqlx::query(
        "SELECT revision, access_key_id, access_key_secret FROM sms_settings
           WHERE singleton=true FOR UPDATE",
    )
    .fetch_optional(&mut *transaction)
    .await?;
    let current = existing
        .as_ref()
        .map(|row| row.try_get::<i64, _>("revision"))
        .transpose()?
        .unwrap_or(0);
    if current != expected {
        return Err(ApiError::conflict(
            "The SMS settings changed; reload before saving.",
        ));
    }
    let access_key_id = input
        .access_key_id
        .clone()
        .filter(|value| !value.is_empty())
        .or_else(|| {
            existing
                .as_ref()
                .and_then(|row| row.try_get::<String, _>("access_key_id").ok())
        })
        .unwrap_or_default();
    let access_key_secret = input
        .access_key_secret
        .clone()
        .filter(|value| !value.is_empty())
        .or_else(|| {
            existing
                .as_ref()
                .and_then(|row| row.try_get::<String, _>("access_key_secret").ok())
        })
        .unwrap_or_default();
    if access_key_id.is_empty() != access_key_secret.is_empty() {
        return Err(invalid(
            "accessKeyId and accessKeySecret must be supplied or cleared together.",
        ));
    }
    sqlx::query(
        r#"INSERT INTO sms_settings
             (singleton, provider, access_key_id, access_key_secret, sign_name, template_code, region, updated_by)
           VALUES (true,$1,$2,$3,$4,$5,$6,$7)
           ON CONFLICT (singleton) DO UPDATE SET
             provider=EXCLUDED.provider, access_key_id=EXCLUDED.access_key_id,
             access_key_secret=EXCLUDED.access_key_secret, sign_name=EXCLUDED.sign_name,
             template_code=EXCLUDED.template_code, region=EXCLUDED.region,
             revision=sms_settings.revision+1, updated_at=now(), updated_by=EXCLUDED.updated_by"#,
    )
    .bind(&input.provider)
    .bind(&access_key_id)
    .bind(&access_key_secret)
    .bind(sign_name)
    .bind(template_code)
    .bind(region)
    .bind(actor)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    get_sms(state).await
}

pub async fn load_sms(state: &AppState) -> Result<Option<SmsTransport>, ApiError> {
    let row = sqlx::query(
        "SELECT access_key_id, access_key_secret, sign_name, template_code, region
           FROM sms_settings WHERE singleton=true",
    )
    .fetch_optional(&state.pool)
    .await?;
    Ok(match row {
        Some(row) => {
            let transport = SmsTransport {
                access_key_id: row.try_get("access_key_id")?,
                access_key_secret: row.try_get("access_key_secret")?,
                sign_name: row.try_get("sign_name")?,
                template_code: row.try_get("template_code")?,
                region: row.try_get("region")?,
            };
            if transport.access_key_id.is_empty() {
                None
            } else {
                Some(transport)
            }
        }
        None => None,
    })
}

pub async fn get_captcha(state: &AppState) -> Result<CaptchaSettings, ApiError> {
    let row = sqlx::query(
        "SELECT provider, site_key, revision FROM captcha_settings WHERE singleton=true",
    )
    .fetch_optional(&state.pool)
    .await?;
    Ok(match row {
        Some(row) => CaptchaSettings {
            configured: true,
            provider: row.try_get("provider")?,
            site_key: row.try_get("site_key")?,
            revision: row.try_get("revision")?,
        },
        None => CaptchaSettings {
            configured: false,
            provider: "turnstile".into(),
            site_key: String::new(),
            revision: 0,
        },
    })
}

pub async fn update_captcha(
    state: &AppState,
    actor: &str,
    expected: i64,
    input: &UpdateCaptchaSettings,
) -> Result<CaptchaSettings, ApiError> {
    if !matches!(
        input.provider.as_str(),
        "turnstile" | "recaptcha" | "hcaptcha"
    ) {
        return Err(invalid(
            "provider must be turnstile, recaptcha or hcaptcha.",
        ));
    }
    let site_key = input.site_key.trim();
    if site_key.is_empty() || site_key.len() > 512 {
        return Err(invalid("siteKey must contain 1 to 512 characters."));
    }
    let mut transaction = state.pool.begin().await?;
    let existing = sqlx::query(
        "SELECT revision, secret_key FROM captcha_settings WHERE singleton=true FOR UPDATE",
    )
    .fetch_optional(&mut *transaction)
    .await?;
    let current = existing
        .as_ref()
        .map(|row| row.try_get::<i64, _>("revision"))
        .transpose()?
        .unwrap_or(0);
    if current != expected {
        return Err(ApiError::conflict(
            "The CAPTCHA settings changed; reload before saving.",
        ));
    }
    let secret_key = input
        .secret_key
        .clone()
        .filter(|value| !value.is_empty())
        .or_else(|| {
            existing
                .as_ref()
                .and_then(|row| row.try_get::<String, _>("secret_key").ok())
        })
        .unwrap_or_default();
    if secret_key.is_empty() {
        return Err(invalid("secretKey is required."));
    }
    sqlx::query(
        r#"INSERT INTO captcha_settings (singleton, provider, site_key, secret_key, updated_by)
           VALUES (true,$1,$2,$3,$4)
           ON CONFLICT (singleton) DO UPDATE SET
             provider=EXCLUDED.provider, site_key=EXCLUDED.site_key,
             secret_key=EXCLUDED.secret_key, revision=captcha_settings.revision+1,
             updated_at=now(), updated_by=EXCLUDED.updated_by"#,
    )
    .bind(&input.provider)
    .bind(site_key)
    .bind(&secret_key)
    .bind(actor)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    get_captcha(state).await
}

pub async fn load_captcha(state: &AppState) -> Result<Option<CaptchaTransport>, ApiError> {
    let row = sqlx::query(
        "SELECT provider, site_key, secret_key FROM captcha_settings WHERE singleton=true",
    )
    .fetch_optional(&state.pool)
    .await?;
    Ok(match row {
        Some(row) => {
            let transport = CaptchaTransport {
                provider: row.try_get("provider")?,
                site_key: row.try_get("site_key")?,
                secret_key: row.try_get("secret_key")?,
            };
            if transport.secret_key.is_empty() {
                None
            } else {
                Some(transport)
            }
        }
        None => None,
    })
}
