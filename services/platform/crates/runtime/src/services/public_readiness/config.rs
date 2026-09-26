use std::{collections::BTreeSet, env};

use reqwest::Url;

use super::{failure, require, PublicReadinessError};

#[derive(Clone, Copy)]
pub(super) enum Surface {
    Public,
    Admin,
    Api,
}

pub(super) struct ReadinessConfig {
    pub(super) public_origin: Url,
    pub(super) admin_origin: Url,
    pub(super) api_origin: Url,
    pub(super) gateway_origin: Url,
    public_host: String,
    admin_host: String,
    api_host: String,
    pub(super) allow_http: bool,
    pub(super) external: bool,
}

impl ReadinessConfig {
    pub(super) fn from_env() -> Result<Self, PublicReadinessError> {
        Ok(Self {
            public_origin: required_origin("AIRTEK_PUBLIC_ORIGIN")?,
            admin_origin: required_origin("AIRTEK_ADMIN_ORIGIN")?,
            api_origin: required_origin("AIRTEK_API_ORIGIN")?,
            gateway_origin: optional_origin(
                "AIRTEK_READINESS_GATEWAY_ORIGIN",
                "http://gateway:8088",
            )?,
            public_host: required("PUBLIC_HOST")?,
            admin_host: required("ADMIN_HOST")?,
            api_host: required("API_HOST")?,
            allow_http: strict_boolean("AIRTEK_READINESS_ALLOW_HTTP")?,
            external: strict_boolean("AIRTEK_READINESS_EXTERNAL")?,
        })
    }

    pub(super) fn origin(&self, surface: Surface) -> &Url {
        match surface {
            Surface::Public => &self.public_origin,
            Surface::Admin => &self.admin_origin,
            Surface::Api => &self.api_origin,
        }
    }

    pub(super) fn gateway_url(&self, path: &str) -> Result<Url, PublicReadinessError> {
        self.gateway_origin
            .join(path.trim_start_matches('/'))
            .map_err(failure)
    }
}

pub(super) fn validate_origin_contract(
    config: &ReadinessConfig,
) -> Result<(), PublicReadinessError> {
    let origins = [
        &config.public_origin,
        &config.admin_origin,
        &config.api_origin,
    ];
    for origin in origins {
        require(
            (origin.scheme() == "https" || config.allow_http)
                && origin.path() == "/"
                && origin.query().is_none()
                && origin.fragment().is_none()
                && origin.username().is_empty()
                && origin.password().is_none(),
            format!("invalid production origin: {origin}"),
        )?;
    }
    require(
        origins
            .iter()
            .map(|url| url.origin().ascii_serialization())
            .collect::<BTreeSet<_>>()
            .len()
            == 3,
        "public, admin and API origins must be distinct",
    )?;
    for (url, expected) in [
        (&config.public_origin, &config.public_host),
        (&config.admin_origin, &config.admin_host),
        (&config.api_origin, &config.api_host),
    ] {
        require(
            url.host_str() == Some(expected),
            format!("{url} does not match host {expected}"),
        )?;
    }
    Ok(())
}

pub(super) fn authority(url: &Url) -> String {
    let host = url.host_str().unwrap_or_default();
    let host = if host.contains(':') {
        format!("[{host}]")
    } else {
        host.to_owned()
    };
    url.port()
        .map_or(host.clone(), |port| format!("{host}:{port}"))
}

fn required(name: &str) -> Result<String, PublicReadinessError> {
    env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| PublicReadinessError(format!("{name} is required")))
}

fn required_origin(name: &str) -> Result<Url, PublicReadinessError> {
    Url::parse(&required(name)?).map_err(failure)
}

fn optional_origin(name: &str, fallback: &str) -> Result<Url, PublicReadinessError> {
    Url::parse(&env::var(name).unwrap_or_else(|_| fallback.to_owned())).map_err(failure)
}

fn strict_boolean(name: &str) -> Result<bool, PublicReadinessError> {
    match env::var(name).unwrap_or_else(|_| "false".into()).as_str() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(PublicReadinessError(format!(
            "{name} must be true or false"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(public: &str, admin: &str, api: &str, allow_http: bool) -> ReadinessConfig {
        ReadinessConfig {
            public_origin: Url::parse(public).unwrap(),
            admin_origin: Url::parse(admin).unwrap(),
            api_origin: Url::parse(api).unwrap(),
            gateway_origin: Url::parse("http://gateway:8088").unwrap(),
            public_host: "www.example.test".into(),
            admin_host: "admin.example.test".into(),
            api_host: "api.example.test".into(),
            allow_http,
            external: false,
        }
    }

    #[test]
    fn production_origin_contract_requires_https_distinct_matching_hosts() {
        assert!(validate_origin_contract(&config(
            "https://www.example.test",
            "https://admin.example.test",
            "https://api.example.test",
            false,
        ))
        .is_ok());
        assert!(validate_origin_contract(&config(
            "http://www.example.test",
            "https://admin.example.test",
            "https://api.example.test",
            false,
        ))
        .is_err());
        assert!(validate_origin_contract(&config(
            "https://www.example.test/path",
            "https://admin.example.test",
            "https://api.example.test",
            false,
        ))
        .is_err());
    }
}
