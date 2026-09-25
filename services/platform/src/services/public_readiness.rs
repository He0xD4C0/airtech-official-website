use std::{collections::BTreeSet, time::Duration};

use reqwest::{
    header::{LOCATION, ORIGIN},
    redirect::Policy,
    Client, Method, StatusCode,
};
use serde::Serialize;
use serde_json::Value;
use sqlx::PgPool;
use thiserror::Error;

mod assets;
mod config;
mod http;
mod parsing;

use config::{validate_origin_contract, ReadinessConfig, Surface};
use http::content_type;

const CORE_ROUTES: [&str; 15] = [
    "/en",
    "/en/products",
    "/en/products/selector",
    "/en/products/compare",
    "/en/search",
    "/en/company/about",
    "/en/company/contact",
    "/en/request-a-quote",
    "/en/request-a-quote/product",
    "/en/request-a-quote/selection",
    "/en/request-a-quote/project",
    "/en/request-a-quote/replacement",
    "/en/privacy",
    "/en/terms",
    "/en/cookie-settings",
];
const SITEMAPS: [&str; 4] = [
    "sitemap-pages.xml",
    "sitemap-products.xml",
    "sitemap-solutions.xml",
    "sitemap-resources.xml",
];

#[derive(Debug, Error)]
#[error("public readiness check failed: {0}")]
pub struct PublicReadinessError(String);

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicReadinessReport {
    status: &'static str,
    checks: Vec<&'static str>,
    warnings: Vec<String>,
}

struct Checker {
    config: ReadinessConfig,
    client: Client,
    checks: Vec<&'static str>,
    warnings: Vec<String>,
}

pub async fn check(pool: &PgPool) -> Result<PublicReadinessReport, PublicReadinessError> {
    let config = ReadinessConfig::from_env()?;
    validate_origin_contract(&config)?;
    let client = Client::builder()
        .redirect(Policy::none())
        .timeout(Duration::from_secs(10))
        .user_agent("airtek-public-readiness/1")
        .build()
        .map_err(failure)?;
    let mut checker = Checker {
        config,
        client,
        checks: vec!["originContract"],
        warnings: Vec::new(),
    };
    checker.check_database(pool).await?;
    let linked_routes = checker.check_shell().await?;
    checker.check_routes(linked_routes).await?;
    checker.check_cors().await?;
    checker.check_discovery().await?;
    checker.check_site_assets().await?;
    checker.check_bundled_origins().await?;
    Ok(PublicReadinessReport {
        status: "ready",
        checks: checker.checks,
        warnings: checker.warnings,
    })
}

impl Checker {
    async fn check_database(&mut self, pool: &PgPool) -> Result<(), PublicReadinessError> {
        let mut transaction = pool.begin().await.map_err(failure)?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *transaction)
            .await
            .map_err(failure)?;
        sqlx::query("SET LOCAL statement_timeout = '10s'")
            .execute(&mut *transaction)
            .await
            .map_err(failure)?;
        let content = sqlx::query_scalar::<_, i64>(
            r#"SELECT count(*)
               FROM cms_published_content published
               JOIN content_entries entry ON entry.id=published.content_id
               WHERE entry.data_origin='developmentFixture'
                  OR entry.is_placeholder=true
                  OR published.document->>'isPlaceholder'='true'"#,
        )
        .fetch_one(&mut *transaction)
        .await
        .map_err(failure)?;
        let products = sqlx::query_scalar::<_, i64>(
            r#"SELECT count(*)
               FROM products product
               LEFT JOIN product_localizations localization
                 ON localization.product_id=product.id
                AND localization.product_revision=product.published_revision
               WHERE product.published_revision IS NOT NULL
                 AND (product.data_origin='developmentFixture'
                      OR localization.data_origin='developmentFixture'
                      OR localization.is_placeholder=true)"#,
        )
        .fetch_one(&mut *transaction)
        .await
        .map_err(failure)?;
        require(
            content == 0 && products == 0,
            format!(
                "published projection contains {content} placeholder/development content records and {products} placeholder/development product records"
            ),
        )?;
        transaction.commit().await.map_err(failure)?;
        self.checks.push("publishedDataClasses");
        Ok(())
    }

    async fn check_shell(&mut self) -> Result<BTreeSet<String>, PublicReadinessError> {
        let bootstrap = self
            .json(Surface::Api, "/api/public/v1/site-bootstrap?locale=en")
            .await?;
        for name in ["generalInformation", "navigation", "footer"] {
            let projection = bootstrap
                .get(name)
                .filter(|value| value.is_object())
                .ok_or_else(|| PublicReadinessError(format!("site bootstrap is missing {name}")))?;
            require(
                projection.get("isPlaceholder").and_then(Value::as_bool) == Some(false),
                format!("site bootstrap {name} is still a placeholder"),
            )?;
        }
        let mut routes = BTreeSet::new();
        for name in ["generalInformation", "navigation", "footer"] {
            routes.extend(parsing::local_route_targets(&bootstrap[name]));
            if let Some(links) = bootstrap[name]
                .get("resolvedLinks")
                .and_then(Value::as_array)
            {
                routes.extend(links.iter().filter_map(|link| {
                    link.get("href").and_then(Value::as_str).map(str::to_owned)
                }));
            }
        }
        self.checks.push("siteShell");
        Ok(routes)
    }

    async fn check_routes(
        &mut self,
        mut linked_routes: BTreeSet<String>,
    ) -> Result<(), PublicReadinessError> {
        linked_routes.extend(CORE_ROUTES.into_iter().map(str::to_owned));
        let root = self.response(Surface::Public, "/", Method::GET).await?;
        require(
            root.status() == StatusCode::PERMANENT_REDIRECT,
            "root must return 308",
        )?;
        require(
            root.headers()
                .get(LOCATION)
                .and_then(|value| value.to_str().ok())
                == Some("/en"),
            "root must redirect to /en",
        )?;
        for path in linked_routes {
            require(
                path == "/en" || path.starts_with("/en/"),
                format!("site shell contains a non-English or unsafe route target: {path}"),
            )?;
            let mut resolve_url = self.config.gateway_url("/api/public/v1/routes/resolve")?;
            resolve_url
                .query_pairs_mut()
                .append_pair("locale", "en")
                .append_pair("path", &path);
            let resolution = self.json_url(Surface::Api, resolve_url).await?;
            require(
                resolution.get("dataClass").and_then(Value::as_str) != Some("developmentFixture"),
                format!("{path} resolves to developmentFixture"),
            )?;
            require(
                resolution
                    .pointer("/page/isPlaceholder")
                    .and_then(Value::as_bool)
                    != Some(true),
                format!("{path} resolves to placeholder content"),
            )?;
            let html = self.text(Surface::Public, &path).await?;
            let canonical = parsing::canonical_href(&html)
                .ok_or_else(|| PublicReadinessError(format!("{path} has no canonical link")))?;
            let expected = format!(
                "{}{}",
                self.config.public_origin.as_str().trim_end_matches('/'),
                path
            );
            require(
                canonical == expected,
                format!("{path} canonical is {canonical}, expected {expected}"),
            )?;
        }
        self.checks.push("coreAndShellRoutes");
        Ok(())
    }

    async fn check_cors(&mut self) -> Result<(), PublicReadinessError> {
        for (path, origin, headers) in [
            (
                "/api/public/v1/rfqs",
                &self.config.public_origin,
                "content-type,idempotency-key",
            ),
            (
                "/api/public/v1/contact",
                &self.config.public_origin,
                "content-type,idempotency-key",
            ),
            (
                "/api/public/v1/selector",
                &self.config.public_origin,
                "content-type",
            ),
            (
                "/api/admin/v1/auth/logout",
                &self.config.admin_origin,
                "content-type,x-csrf-token",
            ),
        ] {
            let response = self
                .request(
                    Surface::Api,
                    self.config.gateway_url(path)?,
                    Method::OPTIONS,
                )
                .header(ORIGIN, origin.origin().ascii_serialization())
                .header("Access-Control-Request-Method", "POST")
                .header("Access-Control-Request-Headers", headers)
                .send()
                .await
                .map_err(failure)?;
            require(
                response.status().is_success(),
                format!("{path} CORS preflight failed"),
            )?;
            require(
                response
                    .headers()
                    .get("Access-Control-Allow-Origin")
                    .and_then(|v| v.to_str().ok())
                    == Some(origin.as_str().trim_end_matches('/')),
                format!("{path} CORS origin mismatch"),
            )?;
            let allowed = response
                .headers()
                .get("Access-Control-Allow-Headers")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_ascii_lowercase();
            require(
                headers
                    .split(',')
                    .all(|header| allowed.split(',').any(|v| v.trim() == header)),
                format!("{path} CORS request headers missing"),
            )?;
            let methods = response
                .headers()
                .get("Access-Control-Allow-Methods")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            require(
                methods.split(',').any(|v| v.trim() == "POST"),
                format!("{path} CORS POST not allowed"),
            )?;
        }
        self.checks.push("cors");
        Ok(())
    }

    async fn check_discovery(&mut self) -> Result<(), PublicReadinessError> {
        let public = self.config.public_origin.as_str().trim_end_matches('/');
        let robots = self.text(Surface::Public, "/robots.txt").await?;
        require(
            !robots.lines().any(|line| line.trim() == "Disallow: /"),
            "robots.txt disallows the complete site",
        )?;
        require(
            robots
                .lines()
                .any(|line| line.trim() == format!("Sitemap: {public}/sitemap.xml")),
            "robots.txt does not advertise the canonical sitemap",
        )?;
        let index = self
            .xml(Surface::Public, "/sitemap.xml", "sitemapindex")
            .await?;
        let expected = SITEMAPS
            .iter()
            .map(|name| format!("{public}/{name}"))
            .collect::<BTreeSet<_>>();
        require(
            parsing::xml_locations(&index)
                .map_err(failure)?
                .into_iter()
                .collect::<BTreeSet<_>>()
                == expected,
            "sitemap index does not contain the exact canonical child set",
        )?;
        for name in SITEMAPS {
            let xml = self
                .xml(Surface::Public, &format!("/{name}"), "urlset")
                .await?;
            for location in parsing::xml_locations(&xml).map_err(failure)? {
                require(
                    location.starts_with(&format!("{public}/")) || location == public,
                    format!("{name} contains a non-canonical URL: {location}"),
                )?;
            }
        }
        self.checks.push("robotsAndSitemaps");
        Ok(())
    }

    async fn check_bundled_origins(&mut self) -> Result<(), PublicReadinessError> {
        let public_html = self.text(Surface::Public, "/en").await?;
        let admin_html = self.text(Surface::Admin, "/login").await?;
        let public_bundle = self.bundle_text(Surface::Public, &public_html).await?;
        let admin_bundle = self.bundle_text(Surface::Admin, &admin_html).await?;
        let public_api = format!(
            "{}/api/public/v1",
            self.config.api_origin.as_str().trim_end_matches('/')
        );
        let admin_api = format!(
            "{}/api/admin/v1",
            self.config.api_origin.as_str().trim_end_matches('/')
        );
        require(
            public_bundle.contains(&public_api),
            "Public bundle API origin does not match runtime",
        )?;
        require(
            admin_bundle.contains(&admin_api),
            "Admin bundle API origin does not match runtime",
        )?;
        for (name, bundle) in [("Public", &public_bundle), ("Admin", &admin_bundle)] {
            require(
                ![
                    "http://localhost:8080",
                    "http://localhost:3000",
                    "http://localhost:3100",
                ]
                .iter()
                .any(|origin| bundle.contains(origin)),
                format!("{name} bundle contains a native-development origin"),
            )?;
        }
        self.checks.push("browserBundleOrigins");
        Ok(())
    }
}

fn require(condition: bool, detail: impl Into<String>) -> Result<(), PublicReadinessError> {
    if condition {
        Ok(())
    } else {
        Err(PublicReadinessError(detail.into()))
    }
}

fn failure(error: impl std::fmt::Display) -> PublicReadinessError {
    PublicReadinessError(error.to_string())
}
