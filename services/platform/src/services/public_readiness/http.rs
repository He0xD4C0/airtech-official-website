use reqwest::{
    header::{HeaderMap, ACCEPT, CONTENT_TYPE, HOST},
    Method, Url,
};
use serde_json::Value;

use super::{
    config::{authority, Surface},
    failure, parsing, require, Checker, PublicReadinessError,
};

impl Checker {
    pub(super) async fn bundle_text(
        &self,
        surface: Surface,
        html: &str,
    ) -> Result<String, PublicReadinessError> {
        let assets = parsing::javascript_assets(html);
        require(
            !assets.is_empty(),
            "HTML contains no JavaScript bundle references",
        )?;
        let mut output = String::new();
        for asset in assets {
            let absolute = self.config.origin(surface).join(&asset).map_err(failure)?;
            require(
                absolute.origin() == self.config.origin(surface).origin(),
                format!("bundle URL crosses an unexpected origin: {absolute}"),
            )?;
            output.push_str(&self.text(surface, absolute.path()).await?);
        }
        Ok(output)
    }

    pub(super) async fn xml(
        &self,
        surface: Surface,
        path: &str,
        root: &str,
    ) -> Result<String, PublicReadinessError> {
        let response = self.response(surface, path, Method::GET).await?;
        require(
            response.status().is_success(),
            format!("{path} is unavailable"),
        )?;
        require(
            content_type(response.headers()).contains("xml"),
            format!("{path} is not XML"),
        )?;
        let body = response.text().await.map_err(failure)?;
        let document = roxmltree::Document::parse(&body).map_err(failure)?;
        require(
            document.root_element().tag_name().name() == root
                && document.root_element().tag_name().namespace()
                    == Some("http://www.sitemaps.org/schemas/sitemap/0.9"),
            format!("{path} is not a complete {root} document"),
        )?;
        Ok(body)
    }

    pub(super) async fn json(
        &self,
        surface: Surface,
        path: &str,
    ) -> Result<Value, PublicReadinessError> {
        let url = self.config.gateway_url(path)?;
        self.json_url(surface, url).await
    }

    pub(super) async fn json_url(
        &self,
        surface: Surface,
        url: Url,
    ) -> Result<Value, PublicReadinessError> {
        let response = self
            .request(surface, url, Method::GET)
            .send()
            .await
            .map_err(failure)?;
        require(
            response.status().is_success(),
            format!("JSON endpoint returned {}", response.status()),
        )?;
        response.json().await.map_err(failure)
    }

    pub(super) async fn text(
        &self,
        surface: Surface,
        path: &str,
    ) -> Result<String, PublicReadinessError> {
        let response = self.response(surface, path, Method::GET).await?;
        require(
            response.status().is_success(),
            format!("{path} returned {}", response.status()),
        )?;
        response.text().await.map_err(failure)
    }

    pub(super) async fn response(
        &self,
        surface: Surface,
        path: &str,
        method: Method,
    ) -> Result<reqwest::Response, PublicReadinessError> {
        let url = self.config.gateway_url(path)?;
        self.request(surface, url, method)
            .send()
            .await
            .map_err(failure)
    }

    pub(super) fn request(
        &self,
        surface: Surface,
        url: Url,
        method: Method,
    ) -> reqwest::RequestBuilder {
        let url = if self.config.external {
            let mut external = self.config.origin(surface).clone();
            external.set_path(url.path());
            external.set_query(url.query());
            external
        } else {
            url
        };
        self.client
            .request(method, url)
            .header(HOST, authority(self.config.origin(surface)))
            .header(ACCEPT, "*/*")
    }
}

pub(super) fn content_type(headers: &HeaderMap) -> &str {
    headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
}
