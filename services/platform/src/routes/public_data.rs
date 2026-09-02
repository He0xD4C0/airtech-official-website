use std::{
    collections::{BTreeMap, BTreeSet},
    net::IpAddr,
};

use axum::{
    extract::{Query, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::{Duration, Utc};
use serde::Deserialize;
#[cfg(test)]
use serde_json::json;
use serde_json::Value;
use sqlx::Row;
use uuid::Uuid;

use crate::{
    config::Config,
    error::ApiError,
    models::{
        ContentEntry, ContentKind, CreateGuestVisit, CursorPage, DataClass, GeneralInformation,
        GuestVisit, NewsEntry, ProductFamily, ProductFamilyPresentation, PublicationStatus,
        RouteResolution, SiteBootstrap,
    },
    routes::etag,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/site-bootstrap", get(site_bootstrap))
        .route("/routes/resolve", get(resolve_route))
        .route("/news", get(list_news))
        .route("/news/{slug}", get(get_news))
        .route("/guest-visits", post(create_guest_visit))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocaleQuery {
    #[serde(default = "default_locale")]
    locale: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResolveRouteQuery {
    path: String,
    #[serde(default = "default_locale")]
    locale: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NewsQuery {
    #[serde(default = "default_locale")]
    locale: String,
    category: Option<String>,
    cursor: Option<String>,
    limit: Option<usize>,
}

async fn site_bootstrap(
    State(state): State<AppState>,
    Query(query): Query<LocaleQuery>,
) -> Result<Json<SiteBootstrap>, ApiError> {
    validate_locale(&query.locale)?;
    let general_information = published_general_information(&state, &query.locale).await?;
    let navigation =
        published_shell_content(&state, ContentKind::Navigation, &query.locale).await?;
    let footer = published_shell_content(&state, ContentKind::Footer, &query.locale).await?;
    let product_families = product_family_presentations(general_information.as_ref());
    let motor_technologies = published_motor_technologies(&state, &query.locale).await?;
    Ok(Json(SiteBootstrap {
        general_information,
        navigation,
        footer,
        product_families,
        motor_technologies,
        generated_at: Utc::now(),
    }))
}

pub(super) async fn published_site_shell_has_placeholder(
    state: &AppState,
    locale: &str,
) -> Result<bool, ApiError> {
    let information = published_general_information(state, locale)
        .await?
        .ok_or_else(incomplete_site_shell)?;
    let navigation = published_shell_content(state, ContentKind::Navigation, locale)
        .await?
        .ok_or_else(incomplete_site_shell)?;
    let footer = published_shell_content(state, ContentKind::Footer, locale)
        .await?
        .ok_or_else(incomplete_site_shell)?;

    // Placeholders deliberately fail closed without requiring their temporary
    // payloads to satisfy the public rendering contract.
    if information.is_placeholder || navigation.is_placeholder || footer.is_placeholder {
        return Ok(true);
    }

    if !valid_published_general_information(&information, locale)
        || !valid_published_shell_content(&navigation, ContentKind::Navigation, locale)
        || !valid_published_shell_content(&footer, ContentKind::Footer, locale)
    {
        return Err(incomplete_site_shell());
    }

    Ok(false)
}

fn incomplete_site_shell() -> ApiError {
    ApiError::service_unavailable("The public site shell projection is incomplete.")
}

fn valid_published_general_information(information: &GeneralInformation, locale: &str) -> bool {
    let Some(payload) = information.payload.as_object() else {
        return false;
    };
    let brand_name = payload.get("brandName").and_then(Value::as_str);
    let home_path = payload.get("homePath").and_then(Value::as_str);
    let organization_name = payload
        .get("organization")
        .and_then(Value::as_object)
        .and_then(|organization| organization.get("name"))
        .and_then(Value::as_str);

    information.locale == locale
        && information.status == PublicationStatus::Published
        && information
            .published_revision
            .is_some_and(|revision| revision > 0)
        && valid_required_text(brand_name, 160)
        && home_path.is_some_and(|path| valid_site_home_path(path, locale))
        && valid_required_text(organization_name, 200)
}

fn valid_published_shell_content(content: &ContentEntry, kind: ContentKind, locale: &str) -> bool {
    content.kind == kind
        && content.locale == locale
        && content.status == PublicationStatus::Published
        && content
            .published_revision
            .is_some_and(|revision| revision > 0)
        && content.body.schema_version == 1
        && content
            .body
            .doc
            .as_object()
            .and_then(|document| document.get("type"))
            .and_then(Value::as_str)
            == Some("doc")
}

fn valid_required_text(value: Option<&str>, maximum_length: usize) -> bool {
    value.is_some_and(|value| {
        let value = value.trim();
        !value.is_empty() && value.len() <= maximum_length && !value.chars().any(char::is_control)
    })
}

fn valid_site_home_path(path: &str, locale: &str) -> bool {
    let path = path.trim();
    let locale_root = format!("/{locale}");
    (path == locale_root || path.starts_with(&format!("{locale_root}/")))
        && path.len() <= 2_048
        && !path.contains(['?', '#', '\\'])
        && !path.contains("//")
        && !path.chars().any(char::is_control)
}

async fn published_motor_technologies(
    state: &AppState,
    locale: &str,
) -> Result<Vec<String>, ApiError> {
    if let Some(pool) = &state.pool {
        return Ok(sqlx::query_scalar::<_, String>(
            r#"SELECT DISTINCT revision.payload->>'motorTechnology'
               FROM products product
               JOIN product_revisions revision ON revision.product_id=product.id
                 AND revision.revision=product.published_revision
               JOIN product_localizations localization ON localization.product_id=product.id
                 AND localization.product_revision=product.published_revision
                 AND localization.locale=$1 AND localization.translation_state='verified'
               JOIN public_routes route ON route.entity_type='product'
                 AND route.entity_id=product.id
                 AND route.locale=localization.locale
                 AND route.canonical_path=localization.seo_metadata->>'canonicalPath'
               WHERE product.published_revision IS NOT NULL
                 AND revision.payload->>'locale'=$1
                 AND nullif(trim(revision.payload->>'motorTechnology'),'') IS NOT NULL
               ORDER BY 1"#,
        )
        .bind(locale)
        .fetch_all(pool)
        .await?);
    }
    let mut values = state
        .data
        .read()
        .await
        .published_products
        .values()
        .filter(|product| product.locale == locale)
        .filter_map(|product| product.motor_technology.clone())
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    Ok(values)
}

async fn resolve_route(
    State(state): State<AppState>,
    Query(query): Query<ResolveRouteQuery>,
) -> Result<Json<RouteResolution>, ApiError> {
    validate_locale(&query.locale)?;
    validate_public_path(&query.path)?;

    let page = published_content_by_path(&state, &query.path, &query.locale).await?;
    if let Some(page) = page {
        let data_class = if page.is_placeholder {
            DataClass::DevelopmentFixture
        } else {
            DataClass::Editorial
        };
        let template_key =
            page_template_key(&page).unwrap_or_else(|| template_key(page.kind).to_owned());
        let indexable = page.seo.indexable && !page.is_placeholder;
        return Ok(Json(RouteResolution {
            path: query.path,
            template_key,
            entity_type: "content".into(),
            entity_id: Some(page.id),
            locale: query.locale,
            published_revision: page.published_revision,
            indexable,
            data_class,
            page: Some(page),
        }));
    }

    if let Some(pool) = &state.pool {
        let route = sqlx::query(
            r#"SELECT route.entity_type,route.entity_id,route.indexable,
                      product.published_revision,product.data_origin
               FROM public_routes route
               JOIN products product
                 ON route.entity_type='product' AND product.id=route.entity_id
                AND product.published_revision IS NOT NULL
               JOIN product_localizations localization
                 ON localization.product_id=product.id
                AND localization.product_revision=product.published_revision
                AND localization.locale=route.locale
                AND localization.translation_state='verified'
               WHERE route.canonical_path=$1 AND route.locale=$2"#,
        )
        .bind(&query.path)
        .bind(&query.locale)
        .fetch_optional(pool)
        .await?;
        if let Some(row) = route {
            let entity_type: String = row.try_get("entity_type")?;
            return Ok(Json(RouteResolution {
                path: query.path,
                template_key: if entity_type == "product" {
                    "productDetail".into()
                } else {
                    entity_type.clone()
                },
                entity_type,
                entity_id: Some(row.try_get("entity_id")?),
                locale: query.locale,
                published_revision: row.try_get("published_revision")?,
                indexable: row.try_get("indexable")?,
                data_class: decode_data_class(row.try_get("data_origin")?),
                page: None,
            }));
        }
    }
    Err(ApiError::not_found("Published route was not found."))
}

async fn list_news(
    State(state): State<AppState>,
    Query(query): Query<NewsQuery>,
) -> Result<Json<CursorPage<NewsEntry>>, ApiError> {
    validate_locale(&query.locale)?;
    let limit = query.limit.unwrap_or(20);
    if !(1..=100).contains(&limit) {
        return Err(ApiError::bad_request("limit must be between 1 and 100."));
    }
    let after = query
        .cursor
        .as_deref()
        .map(|cursor| {
            Uuid::parse_str(cursor).map_err(|_| ApiError::bad_request("cursor is invalid."))
        })
        .transpose()?;

    let mut values = load_published_news(&state, &query.locale, query.category.as_deref()).await?;
    values.sort_by(|left, right| {
        right
            .published_at
            .unwrap_or(right.content.updated_at)
            .cmp(&left.published_at.unwrap_or(left.content.updated_at))
            .then_with(|| left.content.id.cmp(&right.content.id))
    });
    if let Some(after) = after {
        let position = values
            .iter()
            .position(|entry| entry.content.id == after)
            .ok_or_else(|| ApiError::bad_request("cursor is stale or belongs to other filters."))?;
        values.drain(..=position);
    }
    let has_more = values.len() > limit;
    values.truncate(limit);
    let next_cursor = has_more
        .then(|| values.last().map(|entry| entry.content.id.to_string()))
        .flatten();
    Ok(Json(CursorPage {
        items: values,
        next_cursor,
    }))
}

async fn get_news(
    State(state): State<AppState>,
    axum::extract::Path(slug): axum::extract::Path<String>,
    Query(query): Query<LocaleQuery>,
) -> Result<Response, ApiError> {
    validate_locale(&query.locale)?;
    let news = load_published_news(&state, &query.locale, None)
        .await?
        .into_iter()
        .find(|entry| entry.content.slug == slug)
        .ok_or_else(|| ApiError::not_found("Published news was not found."))?;
    let revision = news
        .content
        .published_revision
        .unwrap_or(news.content.current_revision);
    let mut response = Json(news).into_response();
    response.headers_mut().insert(
        header::ETAG,
        HeaderValue::from_str(&etag(revision)).expect("revision ETag is valid"),
    );
    Ok(response)
}

async fn create_guest_visit(
    State(state): State<AppState>,
    Json(request): Json<CreateGuestVisit>,
) -> Result<Response, ApiError> {
    validate_guest_visit(&state.config, &request)?;
    let request = normalize_guest_visit(request);
    let storage_session_id = state.analytics_storage_session_id(request.anonymous_session_id)?;
    let receipt = state
        .current_analytics_consent(request.consent_receipt)
        .await?
        .filter(|receipt| {
            receipt.analytics_allowed
                && receipt.expires_at > Utc::now()
                && receipt.anonymous_session_id == storage_session_id
                && receipt.policy_version == request.policy_version
                && request.policy_version == super::public::ANALYTICS_POLICY_VERSION
        })
        .ok_or_else(|| {
            ApiError::validation(BTreeMap::from([(
                "consentReceipt".into(),
                vec!["An active affirmative analytics consent receipt is required.".into()],
            )]))
        })?;
    let now = Utc::now();
    let source = classify_source(&request);
    let locale = request
        .landing_path
        .trim_start_matches('/')
        .split('/')
        .next()
        .filter(|value| valid_locale_tag(value))
        .unwrap_or("en")
        .to_owned();

    let (visit, created) = if let Some(pool) = &state.pool {
        let existing_id = sqlx::query_scalar::<_, Uuid>(
            r#"SELECT id FROM guest_visits
               WHERE anonymous_session_id=$1 AND consent_record_id=$2 AND retention_until > now()
               ORDER BY first_seen_at DESC LIMIT 1"#,
        )
        .bind(storage_session_id)
        .bind(receipt.consent_receipt)
        .fetch_optional(pool)
        .await?;
        let id = existing_id.unwrap_or_else(Uuid::new_v4);
        let retention_until = now + Duration::days(state.config.guest_raw_retention_days);
        let row = sqlx::query(
            r#"INSERT INTO guest_visits
               (id, anonymous_session_id, consent_record_id, consent_analytics_allowed,
                locale, landing_path, source_type, referrer_host, utm_source, utm_medium,
                utm_campaign, first_seen_at, last_seen_at, retention_until, created_at)
               VALUES ($1,$2,$3,true,$4,$5,$6,$7,$8,$9,$10,$11,$11,$12,$11)
               ON CONFLICT (id) DO UPDATE SET last_seen_at=EXCLUDED.last_seen_at
               RETURNING id, anonymous_session_id, landing_path, referrer_host, source_type,
                         utm_medium, utm_campaign, first_seen_at, last_seen_at, retention_until"#,
        )
        .bind(id)
        .bind(storage_session_id)
        .bind(receipt.consent_receipt)
        .bind(&locale)
        .bind(&request.landing_path)
        .bind(&source)
        .bind(request.referrer_domain.as_deref())
        .bind(request.source.as_deref())
        .bind(request.medium.as_deref())
        .bind(request.campaign.as_deref())
        .bind(now)
        .bind(retention_until)
        .fetch_one(pool)
        .await?;
        let mut visit = decode_guest_visit(&row)?;
        // The keyed database identifier is never returned to the browser.
        visit.anonymous_session_id = request.anonymous_session_id;
        (visit, existing_id.is_none())
    } else {
        let existing = state
            .data
            .read()
            .await
            .guest_visits
            .values()
            .find(|visit| {
                visit.anonymous_session_id == request.anonymous_session_id
                    && visit.retention_until > now
            })
            .cloned();
        let created = existing.is_none();
        let visit = match existing {
            Some(mut visit) => {
                visit.last_seen_at = now;
                visit
            }
            None => GuestVisit {
                id: Uuid::new_v4(),
                anonymous_session_id: request.anonymous_session_id,
                landing_path: request.landing_path,
                referrer_domain: request.referrer_domain,
                source,
                medium: request.medium,
                campaign: request.campaign,
                first_seen_at: now,
                last_seen_at: now,
                retention_until: now + Duration::days(state.config.guest_raw_retention_days),
            },
        };
        state
            .data
            .write()
            .await
            .guest_visits
            .insert(visit.id, visit.clone());
        (visit, created)
    };
    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    let mut response = (status, Json(visit)).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store, max-age=0"),
    );
    Ok(response)
}

async fn published_general_information(
    state: &AppState,
    locale: &str,
) -> Result<Option<GeneralInformation>, ApiError> {
    if let Some(pool) = &state.pool {
        let row = sqlx::query(
            r#"SELECT id, locale, payload, published_revision, is_placeholder,
                      revision_created_at
               FROM published_general_information
               WHERE scope='site' AND locale=$1"#,
        )
        .bind(locale)
        .fetch_optional(pool)
        .await?;
        return row
            .map(|row| {
                let revision: i64 = row.try_get("published_revision")?;
                Ok(GeneralInformation {
                    id: row.try_get("id")?,
                    locale: row.try_get("locale")?,
                    payload: row.try_get("payload")?,
                    status: PublicationStatus::Published,
                    current_revision: revision,
                    published_revision: Some(revision),
                    is_placeholder: row.try_get("is_placeholder")?,
                    updated_at: row.try_get("revision_created_at")?,
                })
            })
            .transpose();
    }
    Ok(state
        .data
        .read()
        .await
        .published_general_information
        .values()
        .find(|entry| entry.locale == locale)
        .cloned())
}

async fn published_shell_content(
    state: &AppState,
    kind: ContentKind,
    locale: &str,
) -> Result<Option<ContentEntry>, ApiError> {
    if let Some(pool) = &state.pool {
        let row = sqlx::query(
            r#"SELECT revision.payload,entry.published_revision FROM content_entries entry
               JOIN content_revisions revision ON revision.content_id=entry.id
                 AND revision.revision=entry.published_revision
               WHERE revision.payload->>'kind'=$1 AND revision.payload->>'locale'=$2
               ORDER BY revision.created_at DESC LIMIT 1"#,
        )
        .bind(content_kind_label(kind))
        .bind(locale)
        .fetch_optional(pool)
        .await?;
        return row
            .map(|row| {
                let mut content =
                    decode_content(row.try_get("payload")?, "published shell content")?;
                let revision: i64 = row.try_get("published_revision")?;
                content.status = PublicationStatus::Published;
                content.current_revision = revision;
                content.published_revision = Some(revision);
                if content.is_placeholder {
                    content.seo.indexable = false;
                }
                Ok(content)
            })
            .transpose();
    }
    Ok(state
        .data
        .read()
        .await
        .published_content
        .values()
        .find(|entry| entry.kind == kind && entry.locale == locale)
        .cloned())
}

async fn published_content_by_path(
    state: &AppState,
    path: &str,
    locale: &str,
) -> Result<Option<ContentEntry>, ApiError> {
    if let Some(pool) = &state.pool {
        let row = sqlx::query(
            r#"SELECT revision.payload,entry.published_revision FROM public_routes route
               JOIN content_entries entry
                 ON route.entity_type='content' AND entry.id=route.entity_id
               JOIN content_revisions revision ON revision.content_id=entry.id
                 AND revision.revision=entry.published_revision
               WHERE route.canonical_path=$1 AND route.locale=$2
                 AND revision.payload->>'locale'=$2 AND entry.published_revision IS NOT NULL
               LIMIT 1"#,
        )
        .bind(path)
        .bind(locale)
        .fetch_optional(pool)
        .await?;
        return row
            .map(|row| {
                let mut content =
                    decode_content(row.try_get("payload")?, "published route content")?;
                let revision: i64 = row.try_get("published_revision")?;
                content.status = PublicationStatus::Published;
                content.current_revision = revision;
                content.published_revision = Some(revision);
                if content.is_placeholder {
                    content.seo.indexable = false;
                }
                Ok(content)
            })
            .transpose();
    }
    Ok(state
        .data
        .read()
        .await
        .published_content
        .values()
        .find(|entry| entry.locale == locale && entry.seo.canonical_path.as_deref() == Some(path))
        .cloned())
}

async fn load_published_news(
    state: &AppState,
    locale: &str,
    category: Option<&str>,
) -> Result<Vec<NewsEntry>, ApiError> {
    if let Some(pool) = &state.pool {
        let rows = sqlx::query(
            r#"SELECT id, payload, published_revision, category, author_display_name,
                      cover_media_asset_id,featured, publication_at, data_origin
               FROM published_news
               WHERE locale=$1 AND ($2::text IS NULL OR category=$2)
               ORDER BY publication_at DESC NULLS LAST, id"#,
        )
        .bind(locale)
        .bind(category)
        .fetch_all(pool)
        .await?;
        return rows
            .into_iter()
            .map(|row| {
                let revision: i64 = row.try_get("published_revision")?;
                let mut content = decode_content(row.try_get("payload")?, "published news")?;
                content.status = PublicationStatus::Published;
                content.current_revision = revision;
                content.published_revision = Some(revision);
                if content.is_placeholder {
                    content.seo.indexable = false;
                }
                Ok(NewsEntry {
                    content,
                    category: row.try_get("category")?,
                    author_display_name: row.try_get("author_display_name")?,
                    cover_media_id: row.try_get("cover_media_asset_id")?,
                    published_at: row.try_get("publication_at")?,
                    featured: row.try_get("featured")?,
                    data_class: decode_data_class(row.try_get("data_origin")?),
                })
            })
            .collect();
    }
    Ok(state
        .data
        .read()
        .await
        .published_news
        .values()
        .filter(|entry| {
            entry.content.locale == locale
                && category
                    .map(|category| category == entry.category)
                    .unwrap_or(true)
        })
        .cloned()
        .collect())
}

fn product_family_presentations(
    general_information: Option<&GeneralInformation>,
) -> Vec<ProductFamilyPresentation> {
    let categories = general_information
        .and_then(|information| information.payload.get("productCategories"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut presentations = categories
        .into_iter()
        .filter_map(|category| {
            let code = category.get("code")?.as_str()?;
            let code_enum = parse_product_family_code(code)?;
            let slug = category.get("slug")?.as_str()?;
            let name = category.get("name")?.as_str()?;
            if !valid_slug(slug) || name.trim().is_empty() || name.len() > 120 {
                return None;
            }
            Some(ProductFamilyPresentation {
                code: code_enum,
                slug: slug.to_owned(),
                name: name.to_owned(),
                description: category
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                sort_order: category
                    .get("sortOrder")
                    .and_then(Value::as_i64)
                    .and_then(|value| i32::try_from(value).ok())
                    .unwrap_or(i32::MAX),
            })
        })
        .collect::<Vec<_>>();
    presentations.sort_by_key(|value| value.sort_order);
    presentations.dedup_by_key(|value| value.code);
    presentations
}

fn decode_guest_visit(row: &sqlx::postgres::PgRow) -> Result<GuestVisit, ApiError> {
    Ok(GuestVisit {
        id: row.try_get("id")?,
        anonymous_session_id: row.try_get("anonymous_session_id")?,
        landing_path: row.try_get("landing_path")?,
        referrer_domain: row.try_get("referrer_host")?,
        source: row.try_get("source_type")?,
        medium: row.try_get("utm_medium")?,
        campaign: row.try_get("utm_campaign")?,
        first_seen_at: row.try_get("first_seen_at")?,
        last_seen_at: row.try_get("last_seen_at")?,
        retention_until: row.try_get("retention_until")?,
    })
}

fn validate_guest_visit(config: &Config, request: &CreateGuestVisit) -> Result<(), ApiError> {
    let mut errors = BTreeMap::new();
    if !valid_guest_landing_path(&request.landing_path) {
        errors.insert(
            "landingPath".into(),
            vec![
                "Must be a clean public path without query, fragment, encoded sensitive markers, or PII-like segments."
                    .into(),
            ],
        );
    }
    if request
        .referrer_domain
        .as_deref()
        .is_some_and(|value| !valid_referrer_hostname(value))
    {
        errors.insert(
            "referrerDomain".into(),
            vec![
                "Must be a hostname without a URL, IP address, control character, or PII-like value."
                    .into(),
            ],
        );
    }
    for (name, value, maximum, allowlist) in [
        (
            "source",
            request.source.as_deref(),
            128_usize,
            &config.analytics_utm_source_allowlist,
        ),
        (
            "medium",
            request.medium.as_deref(),
            128,
            &config.analytics_utm_medium_allowlist,
        ),
        (
            "campaign",
            request.campaign.as_deref(),
            200,
            &config.analytics_utm_campaign_allowlist,
        ),
    ] {
        if value.is_some_and(|value| !valid_guest_attribution(value, maximum, allowlist)) {
            errors.insert(
                name.into(),
                vec![format!(
                    "Must be an approved non-PII analytics identifier no longer than {maximum} characters."
                )],
            );
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ApiError::validation(errors))
    }
}

fn normalize_guest_visit(mut request: CreateGuestVisit) -> CreateGuestVisit {
    request.referrer_domain = request
        .referrer_domain
        .map(|value| value.trim().to_ascii_lowercase());
    request.source = normalize_optional_attribution(request.source);
    request.medium = normalize_optional_attribution(request.medium);
    request.campaign = normalize_optional_attribution(request.campaign);
    request
}

fn normalize_optional_attribution(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_ascii_lowercase();
        (!value.is_empty()).then_some(value)
    })
}

fn valid_referrer_hostname(value: &str) -> bool {
    if value.chars().any(char::is_control) {
        return false;
    }
    let hostname = value.trim();
    if hostname.is_empty()
        || hostname.len() > 253
        || !hostname.is_ascii()
        || hostname.parse::<IpAddr>().is_ok()
        || contains_pii_like_value(hostname)
        || hostname.starts_with('.')
        || hostname.ends_with('.')
    {
        return false;
    }
    hostname.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !looks_like_phone(label)
            && label
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphanumeric)
            && label
                .as_bytes()
                .last()
                .is_some_and(u8::is_ascii_alphanumeric)
            && label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    })
}

fn valid_guest_attribution(value: &str, maximum: usize, allowlist: &BTreeSet<String>) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return true;
    }
    let normalized = trimmed.to_ascii_lowercase();
    normalized.len() <= maximum
        && normalized.is_ascii()
        && normalized
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        && normalized
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric)
        && normalized
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
        && !contains_pii_like_value(&normalized)
        && allowlist.contains(&normalized)
}

pub(super) fn valid_guest_landing_path(path: &str) -> bool {
    if validate_public_path(path).is_err()
        || path.chars().any(char::is_control)
        || path.contains('\\')
    {
        return false;
    }
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .all(|segment| {
            let Some(decoded) = decode_safe_path_segment(segment) else {
                return false;
            };
            !contains_pii_like_value(segment) && !contains_pii_like_value(&decoded)
        })
}

/// Decode one path segment while rejecting encoded delimiters and identifier
/// markers. Encoded `%` is rejected as well so double encoding cannot bypass
/// the PII checks on a second decode.
fn decode_safe_path_segment(segment: &str) -> Option<String> {
    let bytes = segment.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            decoded.push(bytes[index]);
            index += 1;
            continue;
        }
        let high = *bytes.get(index + 1)?;
        let low = *bytes.get(index + 2)?;
        let byte = hex_value(high)? * 16 + hex_value(low)?;
        if byte.is_ascii_control()
            || matches!(
                byte,
                b'%' | b'@' | b'+' | b':' | b'?' | b'#' | b'/' | b'\\' | b'=' | b'&'
            )
        {
            return None;
        }
        decoded.push(byte);
        index += 3;
    }
    String::from_utf8(decoded).ok()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn contains_pii_like_value(value: &str) -> bool {
    looks_like_email(value)
        || contains_phone_like(value)
        || contains_ip_address(value)
        || contains_secret_like_value(value)
        || value
            .split(|character: char| !character.is_ascii_alphanumeric() && character != '-')
            .any(|token| Uuid::parse_str(token).is_ok())
}

fn looks_like_email(value: &str) -> bool {
    value.split_whitespace().any(|token| {
        let token = token.trim_matches(|character: char| {
            matches!(
                character,
                '<' | '>' | '(' | ')' | '[' | ']' | ',' | ';' | '"' | '\''
            )
        });
        token.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty() && domain.contains('.') && !domain.ends_with('.')
        })
    })
}

fn contains_phone_like(value: &str) -> bool {
    if looks_like_phone(value.trim()) {
        return true;
    }
    let mut digit_count = 0_usize;
    for byte in value.bytes().chain(std::iter::once(b'!')) {
        if byte.is_ascii_digit() {
            digit_count += 1;
        } else if !matches!(byte, b' ' | b'+' | b'-' | b'(' | b')' | b'.') {
            if (10..=15).contains(&digit_count) {
                return true;
            }
            digit_count = 0;
        }
    }
    false
}

fn looks_like_phone(value: &str) -> bool {
    let digit_count = value.bytes().filter(u8::is_ascii_digit).count();
    (8..=15).contains(&digit_count)
        && value.bytes().all(|byte| {
            byte.is_ascii_digit() || matches!(byte, b' ' | b'+' | b'-' | b'(' | b')' | b'.')
        })
}

fn contains_ip_address(value: &str) -> bool {
    value.parse::<IpAddr>().is_ok()
        || value
            .split(|character: char| {
                !(character.is_ascii_hexdigit() || matches!(character, '.' | ':'))
            })
            .filter(|candidate| !candidate.is_empty())
            .any(|candidate| candidate.parse::<IpAddr>().is_ok())
}

fn contains_secret_like_value(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.starts_with("bearer ")
        || lower.starts_with("basic ")
        || lower.starts_with("sk-")
        || lower.contains("api_key=")
        || lower.contains("apikey=")
        || lower.contains("token=")
        || lower.contains("mailto:")
        || lower.contains("tel:")
}

fn classify_source(request: &CreateGuestVisit) -> String {
    let source = request.source.as_deref().unwrap_or_default().to_lowercase();
    let medium = request.medium.as_deref().unwrap_or_default().to_lowercase();
    if source.is_empty() && request.referrer_domain.is_none() {
        "direct"
    } else if medium.contains("cpc") || medium.contains("paid") || medium.contains("ppc") {
        "paidSearch"
    } else if medium.contains("organic") {
        "organicSearch"
    } else if medium.contains("social") {
        "social"
    } else if medium.contains("email") {
        "email"
    } else if request.referrer_domain.is_some() {
        "referral"
    } else if !source.is_empty() {
        "other"
    } else {
        "unknown"
    }
    .into()
}

fn validate_locale(value: &str) -> Result<(), ApiError> {
    if valid_locale_tag(value) {
        Ok(())
    } else {
        Err(ApiError::bad_request("locale is invalid."))
    }
}

fn valid_locale_tag(value: &str) -> bool {
    (2..=35).contains(&value.len())
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn validate_public_path(path: &str) -> Result<(), ApiError> {
    if path.starts_with('/')
        && path.len() <= 2_048
        && !path.contains('?')
        && !path.contains('#')
        && !path.contains("//")
        && !path.starts_with("/admin")
    {
        Ok(())
    } else {
        Err(ApiError::bad_request("path is not a clean public path."))
    }
}

fn decode_content(payload: Value, entity: &str) -> Result<ContentEntry, ApiError> {
    serde_json::from_value(payload).map_err(|error| {
        tracing::error!(%error, entity, "stored JSON payload is invalid");
        ApiError::service_unavailable(format!("Stored {entity} data is invalid."))
    })
}

fn decode_data_class(value: String) -> DataClass {
    match value.as_str() {
        "developmentFixture" => DataClass::DevelopmentFixture,
        "feishu" => DataClass::Feishu,
        "verifiedCsv" => DataClass::VerifiedCsv,
        _ => DataClass::Editorial,
    }
}

fn content_kind_label(kind: ContentKind) -> &'static str {
    match kind {
        ContentKind::Home => "home",
        ContentKind::Solution => "solution",
        ContentKind::Technology => "technology",
        ContentKind::Article => "article",
        ContentKind::News => "news",
        ContentKind::Faq => "faq",
        ContentKind::CaseStudy => "caseStudy",
        ContentKind::Download => "download",
        ContentKind::Company => "company",
        ContentKind::Legal => "legal",
        ContentKind::Navigation => "navigation",
        ContentKind::Footer => "footer",
    }
}

fn template_key(kind: ContentKind) -> &'static str {
    match kind {
        ContentKind::Home => "home",
        ContentKind::Solution => "solution",
        ContentKind::Technology => "technology",
        ContentKind::Article => "article",
        ContentKind::News => "news",
        ContentKind::Faq => "faq",
        ContentKind::CaseStudy => "caseStudy",
        ContentKind::Download => "download",
        ContentKind::Company => "company",
        ContentKind::Legal => "legal",
        ContentKind::Navigation => "navigation",
        ContentKind::Footer => "footer",
    }
}

fn parse_product_family_code(value: &str) -> Option<ProductFamily> {
    match value {
        "centrifugal" => Some(ProductFamily::Centrifugal),
        "axial" => Some(ProductFamily::Axial),
        "crossFlow" => Some(ProductFamily::CrossFlow),
        "inlineDuct" => Some(ProductFamily::InlineDuct),
        "motors" => Some(ProductFamily::Motors),
        _ => None,
    }
}

fn page_template_key(page: &ContentEntry) -> Option<String> {
    let value = page
        .body
        .doc
        .pointer("/attrs/pageSlots/templateKey")?
        .as_str()?;
    matches!(
        value,
        "home"
            | "products"
            | "productFamily"
            | "productDetail"
            | "selector"
            | "compare"
            | "solutions"
            | "solution"
            | "technology"
            | "articleIndex"
            | "article"
            | "newsIndex"
            | "news"
            | "faq"
            | "caseStudy"
            | "downloads"
            | "about"
            | "contact"
            | "rfq"
            | "search"
            | "legal"
    )
    .then(|| value.to_owned())
}

fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn default_locale() -> String {
    "en".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site_information(payload: Value, is_placeholder: bool) -> GeneralInformation {
        GeneralInformation {
            id: Uuid::new_v4(),
            locale: "en".into(),
            payload,
            status: PublicationStatus::Published,
            current_revision: 1,
            published_revision: Some(1),
            is_placeholder,
            updated_at: Utc::now(),
        }
    }

    fn shell_content(kind: ContentKind, is_placeholder: bool) -> ContentEntry {
        ContentEntry {
            id: Uuid::new_v4(),
            kind,
            slug: match kind {
                ContentKind::Navigation => "primary-navigation",
                ContentKind::Footer => "primary-footer",
                _ => unreachable!("site shell fixture only supports navigation and footer"),
            }
            .into(),
            locale: "en".into(),
            title: "Published site shell".into(),
            summary: None,
            body: crate::models::RichTextDocument {
                schema_version: 1,
                doc: json!({"type": "doc", "content": []}),
            },
            seo: crate::models::SeoMetadata::default(),
            status: PublicationStatus::Published,
            is_placeholder,
            current_revision: 1,
            published_revision: Some(1),
            scheduled_for: None,
            updated_at: Utc::now(),
        }
    }

    async fn install_site_shell(
        state: &AppState,
        information: GeneralInformation,
        navigation_placeholder: bool,
        footer_placeholder: bool,
    ) {
        let navigation = shell_content(ContentKind::Navigation, navigation_placeholder);
        let footer = shell_content(ContentKind::Footer, footer_placeholder);
        let mut data = state.data.write().await;
        data.published_general_information
            .insert(information.id, information);
        data.published_content.insert(navigation.id, navigation);
        data.published_content.insert(footer.id, footer);
    }

    fn complete_site_information(is_placeholder: bool) -> GeneralInformation {
        site_information(
            json!({
                "brandName": "AIRTEKPOWER",
                "homePath": "/en",
                "organization": {"name": "AIRTEKPOWER"}
            }),
            is_placeholder,
        )
    }

    fn guest_visit_fixture() -> CreateGuestVisit {
        CreateGuestVisit {
            anonymous_session_id: Uuid::new_v4(),
            consent_receipt: Uuid::new_v4(),
            policy_version: super::super::public::ANALYTICS_POLICY_VERSION.into(),
            landing_path: "/en/products/b23e280h128-102-b0".into(),
            referrer_domain: Some("search.example.com".into()),
            source: Some("google".into()),
            medium: Some("cpc".into()),
            campaign: Some("autumn-launch-2026".into()),
        }
    }

    #[test]
    fn guest_visit_rejects_full_referrer_and_tracking_query() {
        let mut request = guest_visit_fixture();
        request.landing_path = "/en/products?email=private@example.com".into();
        request.referrer_domain = Some("https://example.com/a?q=private".into());
        let problem = validate_guest_visit(&Config::for_test(), &request)
            .expect_err("unsafe attribution is rejected");
        assert_eq!(problem.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn guest_visit_normalizes_safe_attribution_before_storage() {
        let mut request = guest_visit_fixture();
        request.referrer_domain = Some("  Search.Example.COM  ".into());
        request.source = Some("  Google-Ads  ".into());
        request.medium = Some(" cpc ".into());
        request.campaign = Some("   ".into());

        validate_guest_visit(&Config::for_test(), &request).expect("safe attribution is accepted");
        let normalized = normalize_guest_visit(request);

        assert_eq!(
            normalized.referrer_domain.as_deref(),
            Some("search.example.com")
        );
        assert_eq!(normalized.source.as_deref(), Some("google-ads"));
        assert_eq!(normalized.medium.as_deref(), Some("cpc"));
        assert_eq!(normalized.campaign, None);
    }

    #[test]
    fn guest_visit_rejects_non_hostname_or_identifier_referrers() {
        for referrer in [
            "192.0.2.10",
            "2001:db8::1",
            "person@example.com",
            "13800138000.example",
            "bad_host.example",
            "-bad.example",
            "bad-.example",
            "bad..example",
            "example.com.",
            "example.com\n",
        ] {
            let mut request = guest_visit_fixture();
            request.referrer_domain = Some(referrer.into());
            assert!(
                validate_guest_visit(&Config::for_test(), &request).is_err(),
                "referrer {referrer:?} must be rejected"
            );
        }
    }

    #[test]
    fn guest_visit_rejects_pii_like_attribution_values() {
        for value in [
            "person@example.com",
            "+86 13800138000",
            "call +86 13800138000",
            "192.0.2.10",
            "source 2001:db8::1",
            "token=secret-value",
            "550e8400-e29b-41d4-a716-446655440000",
            "line\nbreak",
        ] {
            for field in ["source", "medium", "campaign"] {
                let mut request = guest_visit_fixture();
                match field {
                    "source" => request.source = Some(value.into()),
                    "medium" => request.medium = Some(value.into()),
                    "campaign" => request.campaign = Some(value.into()),
                    _ => unreachable!(),
                }
                assert!(
                    validate_guest_visit(&Config::for_test(), &request).is_err(),
                    "{field} value {value:?} must be rejected"
                );
            }
        }
    }

    #[test]
    fn guest_visit_rejects_direct_and_encoded_identifiers_in_landing_path() {
        for landing_path in [
            "/en/ref/person@example.com",
            "/en/ref/person%40example.com",
            "/en/ref/%31%33%38%30%30%31%33%38%30%30%30",
            "/en/ref/192.0.2.10",
            "/en/ref/550e8400-e29b-41d4-a716-446655440000",
            "/en/ref/%2540",
            "/en/ref/%0aheader",
            "/en/ref\\private",
            "/en/ref/line\nbreak",
        ] {
            let mut request = guest_visit_fixture();
            request.landing_path = landing_path.into();
            assert!(
                validate_guest_visit(&Config::for_test(), &request).is_err(),
                "landing path {landing_path:?} must be rejected"
            );
        }
    }

    #[test]
    fn guest_visit_accepts_canonical_product_path_and_standard_utm_values() {
        validate_guest_visit(&Config::for_test(), &guest_visit_fixture())
            .expect("canonical paths and non-PII UTM dimensions are accepted");
    }

    #[test]
    fn guest_visit_rejects_names_and_unregistered_free_text_dimensions() {
        for value in ["Jane Doe", "unregistered-campaign", "form-body-value"] {
            for field in ["source", "medium", "campaign"] {
                let mut request = guest_visit_fixture();
                match field {
                    "source" => request.source = Some(value.into()),
                    "medium" => request.medium = Some(value.into()),
                    "campaign" => request.campaign = Some(value.into()),
                    _ => unreachable!(),
                }
                assert!(
                    validate_guest_visit(&Config::for_test(), &request).is_err(),
                    "unregistered {field} value {value:?} must be rejected"
                );
            }
        }
    }

    #[test]
    fn source_classification_is_data_minimized() {
        let request = guest_visit_fixture();
        assert_eq!(classify_source(&request), "paidSearch");
    }

    #[test]
    fn configured_product_family_remains_visible_without_any_products() {
        let information = GeneralInformation {
            id: Uuid::new_v4(),
            locale: "en".into(),
            payload: json!({
                "productCategories": [{
                    "code": "axial",
                    "slug": "axial",
                    "name": "Axial fans",
                    "description": "Configured category with an intentionally empty catalog.",
                    "sortOrder": 2
                }]
            }),
            status: PublicationStatus::Published,
            current_revision: 1,
            published_revision: Some(1),
            is_placeholder: false,
            updated_at: Utc::now(),
        };

        let presentations = product_family_presentations(Some(&information));
        assert_eq!(presentations.len(), 1);
        assert_eq!(presentations[0].code, ProductFamily::Axial);
        assert_eq!(presentations[0].slug, "axial");
    }

    #[tokio::test]
    async fn public_discovery_rejects_a_missing_site_shell() {
        let error = published_site_shell_has_placeholder(&AppState::for_test(), "en")
            .await
            .expect_err("missing site shell must fail closed");
        assert_eq!(error.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn public_discovery_rejects_an_incomplete_site_shell() {
        let state = AppState::for_test();
        install_site_shell(&state, site_information(json!({}), false), false, false).await;

        let error = published_site_shell_has_placeholder(&state, "en")
            .await
            .expect_err("incomplete General Information must fail closed");
        assert_eq!(error.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn public_discovery_returns_no_entries_for_a_placeholder_site_shell() {
        let state = AppState::for_test();
        install_site_shell(&state, complete_site_information(true), false, false).await;

        assert!(published_site_shell_has_placeholder(&state, "en")
            .await
            .expect("placeholder lookup succeeds"));
    }

    #[tokio::test]
    async fn public_discovery_accepts_a_complete_non_placeholder_site_shell() {
        let state = AppState::for_test();
        install_site_shell(&state, complete_site_information(false), false, false).await;

        assert!(!published_site_shell_has_placeholder(&state, "en")
            .await
            .expect("complete site shell lookup succeeds"));
    }
}
