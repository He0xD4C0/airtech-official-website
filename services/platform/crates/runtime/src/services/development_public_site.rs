//! DevTools-only, ownership-safe publication of the local public-site shell.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::error::ApiError;
use crate::services::{
    cms_templates::{canonical_path, validate_content_draft, CmsTemplateValidationPhase},
    cms_workflow,
};
use airtek_domain::models::ContentDraftV2;

const SEED_LOCK: i64 = 672_183_922;
const SEED_VERSION: i32 = 1;
const FIXTURES: &str = include_str!("../../../../fixtures/development-public-site.json");

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevelopmentPublicSiteReport {
    pub status: &'static str,
    pub published_entries: usize,
    pub editorial_replacements: usize,
}

struct Fixture {
    key: String,
    identity: String,
    checksum: String,
    path: Option<String>,
    document: ContentDraftV2,
    value: Value,
}

struct Actual {
    id: Uuid,
    identity: String,
    data_origin: String,
    is_placeholder: bool,
    document: Option<Value>,
    path: Option<String>,
    route_indexable: Option<bool>,
}

struct Ledger {
    entity_id: Uuid,
    seed_version: i32,
    checksum: String,
}

enum SeedState {
    Empty,
    Complete { editorial_replacements: usize },
    Partial(String),
}

pub async fn ensure(
    pool: &sqlx::PgPool,
    admin_email: &str,
) -> Result<DevelopmentPublicSiteReport, ApiError> {
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(pool)
        .await?;
    if !database.starts_with("airtek_test_") {
        return Err(ApiError::conflict("Public development seed is allowed only in explicitly disposable airtek_test_ databases."));
    }
    let fixtures = fixtures()?;
    let mut transaction = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(SEED_LOCK)
        .execute(&mut *transaction)
        .await?;
    match inspect_state(&mut transaction, &fixtures).await? {
        SeedState::Complete {
            editorial_replacements,
        } => {
            transaction.commit().await?;
            return Ok(report(
                "skippedComplete",
                fixtures.len(),
                editorial_replacements,
            ));
        }
        SeedState::Partial(detail) => return Err(ApiError::conflict(detail)),
        SeedState::Empty => {}
    }
    let actor_id = development_actor(&mut transaction, admin_email).await?;
    for fixture in &fixtures {
        let content_id = Uuid::new_v4();
        cms_workflow::publish_development_fixture(
            &mut transaction,
            actor_id,
            admin_email,
            content_id,
            &fixture.document,
        )
        .await?;
        sqlx::query(
            r#"INSERT INTO development_fixture_ledger
               (fixture_key,entity_type,entity_id,locale,seed_version,checksum)
               VALUES ($1,'content',$2,'en',$3,$4)"#,
        )
        .bind(&fixture.key)
        .bind(content_id)
        .bind(SEED_VERSION)
        .bind(&fixture.checksum)
        .execute(&mut *transaction)
        .await?;
    }
    transaction.commit().await?;
    Ok(report("created", fixtures.len(), 0))
}

fn fixtures() -> Result<Vec<Fixture>, ApiError> {
    let documents: Vec<ContentDraftV2> = serde_json::from_str(FIXTURES).map_err(|error| {
        ApiError::internal(format!("Development public fixture is invalid: {error}"))
    })?;
    let mut identities = BTreeSet::new();
    let mut paths = BTreeSet::new();
    let mut fixtures = Vec::with_capacity(documents.len());
    for document in documents {
        validate_fixture(&document)?;
        let identity = identity(&document);
        if !identities.insert(identity.clone()) {
            return Err(ApiError::internal(
                "Development public fixture identities must be unique.",
            ));
        }
        let path = canonical_path(
            &document.locale,
            document.template_key,
            document.slug.as_deref(),
        );
        if path
            .as_ref()
            .is_some_and(|value| !paths.insert(value.clone()))
        {
            return Err(ApiError::internal(
                "Development public fixture routes must be unique.",
            ));
        }
        let value = serde_json::to_value(&document)
            .map_err(|_| ApiError::internal("CMS serialization failed."))?;
        let checksum =
            format!(
                "{:x}",
                Sha256::digest(serde_json::to_vec(&value).map_err(|_| ApiError::internal(
                    "CMS fixture checksum serialization failed."
                ),)?)
            );
        fixtures.push(Fixture {
            key: fixture_key(&document),
            identity,
            checksum,
            path,
            document,
            value,
        });
    }
    if paths.len() != 15 || fixtures.len() != 18 {
        return Err(ApiError::internal(
            "Development public fixtures must contain 18 records and 15 routes.",
        ));
    }
    Ok(fixtures)
}

fn validate_fixture(document: &ContentDraftV2) -> Result<(), ApiError> {
    if document.locale != "en" || !document.is_placeholder || document.seo.indexable {
        return Err(ApiError::internal(
            "Development public fixtures must be English placeholders with indexing disabled.",
        ));
    }
    let issues = validate_content_draft(document, CmsTemplateValidationPhase::Publish);
    if issues.is_empty() {
        Ok(())
    } else {
        Err(ApiError::internal(format!(
            "Development fixture {:?}/{:?} failed CMS validation: {issues:?}",
            document.template_key, document.slug
        )))
    }
}

async fn inspect_state(
    transaction: &mut Transaction<'_, Postgres>,
    fixtures: &[Fixture],
) -> Result<SeedState, ApiError> {
    let actual = load_actual(transaction).await?;
    let ledgers = load_ledgers(transaction).await?;
    let draft_count = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM cms_drafts")
        .fetch_one(&mut **transaction)
        .await?;
    if actual.is_empty() && ledgers.is_empty() && draft_count == 0 {
        return Ok(SeedState::Empty);
    }

    let actual_by_identity = actual
        .iter()
        .map(|record| (record.identity.clone(), record))
        .collect::<BTreeMap<_, _>>();
    let actual_by_id = actual
        .iter()
        .map(|record| (record.id, record))
        .collect::<BTreeMap<_, _>>();
    let mut issues = Vec::new();
    let mut editorial_replacements = 0;
    let expected_keys = fixtures
        .iter()
        .map(|fixture| fixture.key.clone())
        .collect::<BTreeSet<_>>();

    for fixture in fixtures {
        let same_identity = actual_by_identity.get(&fixture.identity).copied();
        if same_identity.is_some_and(|record| valid_editorial_replacement(record, fixture)) {
            editorial_replacements += 1;
            continue;
        }
        let Some(ledger) = ledgers.get(&fixture.key) else {
            issues.push(format!(
                "{} has no owned fixture or valid editorial replacement",
                fixture.key
            ));
            continue;
        };
        let record = actual_by_id.get(&ledger.entity_id).copied();
        if !valid_owned_fixture(record, ledger, fixture) {
            issues.push(format!(
                "{} is missing, changed, or no longer ledger-owned",
                fixture.key
            ));
        }
    }
    for key in ledgers.keys().filter(|key| !expected_keys.contains(*key)) {
        issues.push(format!("unexpected ledger entry {key}"));
    }
    for record in actual
        .iter()
        .filter(|record| record.data_origin == "developmentFixture")
    {
        if !ledgers.values().any(|ledger| ledger.entity_id == record.id) {
            issues.push(format!("unowned development fixture {}", record.identity));
        }
    }
    if issues.is_empty() {
        Ok(SeedState::Complete {
            editorial_replacements,
        })
    } else {
        issues.sort();
        Ok(SeedState::Partial(format!(
            "Development public seed refused partial or unknown CMS state: {}. Resolve or reset the local development database explicitly.",
            issues.join("; ")
        )))
    }
}

async fn load_actual(transaction: &mut Transaction<'_, Postgres>) -> Result<Vec<Actual>, ApiError> {
    let rows = sqlx::query(
        r#"SELECT entry.id,entry.kind,entry.slug,entry.locale,entry.template_key,
                  entry.data_origin,entry.is_placeholder,published.document,
                  route.canonical_path,route.indexable AS route_indexable
           FROM content_entries entry
           LEFT JOIN cms_published_content published ON published.content_id=entry.id
           LEFT JOIN public_routes route
             ON route.entity_type='content' AND route.entity_id=entry.id
            AND route.locale=entry.locale"#,
    )
    .fetch_all(&mut **transaction)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(Actual {
                id: row.try_get("id")?,
                identity: format!(
                    "{}|{}|{}|{}",
                    row.get::<String, _>("kind"),
                    row.get::<String, _>("template_key"),
                    row.get::<String, _>("locale"),
                    row.get::<String, _>("slug")
                ),
                data_origin: row.try_get("data_origin")?,
                is_placeholder: row.try_get("is_placeholder")?,
                document: row.try_get("document")?,
                path: row.try_get("canonical_path")?,
                route_indexable: row.try_get("route_indexable")?,
            })
        })
        .collect()
}

async fn load_ledgers(
    transaction: &mut Transaction<'_, Postgres>,
) -> Result<BTreeMap<String, Ledger>, ApiError> {
    let rows = sqlx::query(
        r#"SELECT fixture_key,entity_id,seed_version,checksum
           FROM development_fixture_ledger
           WHERE fixture_key LIKE 'development/public-site/%'"#,
    )
    .fetch_all(&mut **transaction)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok((
                row.try_get("fixture_key")?,
                Ledger {
                    entity_id: row.try_get("entity_id")?,
                    seed_version: row.try_get("seed_version")?,
                    checksum: row.try_get("checksum")?,
                },
            ))
        })
        .collect()
}

fn valid_owned_fixture(record: Option<&Actual>, ledger: &Ledger, fixture: &Fixture) -> bool {
    ledger.seed_version == SEED_VERSION
        && ledger.checksum == fixture.checksum
        && record.is_some_and(|record| {
            record.identity == fixture.identity
                && record.data_origin == "developmentFixture"
                && record.is_placeholder
                && record.document.as_ref() == Some(&fixture.value)
                && record.path == fixture.path
                && record.route_indexable == fixture.path.as_ref().map(|_| false)
        })
}

fn valid_editorial_replacement(record: &Actual, fixture: &Fixture) -> bool {
    if record.data_origin != "editorial" || record.is_placeholder || record.path != fixture.path {
        return false;
    }
    record
        .document
        .clone()
        .and_then(|value| serde_json::from_value::<ContentDraftV2>(value).ok())
        .is_some_and(|document| {
            !document.is_placeholder
                && identity(&document) == fixture.identity
                && validate_content_draft(&document, CmsTemplateValidationPhase::Publish).is_empty()
                && record.route_indexable == fixture.path.as_ref().map(|_| document.seo.indexable)
        })
}

async fn development_actor(
    transaction: &mut Transaction<'_, Postgres>,
    admin_email: &str,
) -> Result<Uuid, ApiError> {
    sqlx::query_scalar("SELECT id FROM users WHERE lower(email)=lower($1) AND status='active'")
        .bind(admin_email.trim())
        .fetch_optional(&mut **transaction)
        .await?
        .ok_or_else(|| {
            ApiError::conflict(
                "Development public seed requires the configured active local administrator.",
            )
        })
}

fn fixture_key(document: &ContentDraftV2) -> String {
    format!(
        "development/public-site/v1/{}/{}",
        enum_label(document.template_key),
        document.slug.as_deref().unwrap_or("singleton")
    )
}

fn identity(document: &ContentDraftV2) -> String {
    format!(
        "{}|{}|{}|{}",
        enum_label(document.kind),
        enum_label(document.template_key),
        document.locale,
        document.slug.as_deref().unwrap_or("")
    )
}

fn enum_label<T: Serialize>(value: T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn report(
    status: &'static str,
    published_entries: usize,
    editorial_replacements: usize,
) -> DevelopmentPublicSiteReport {
    DevelopmentPublicSiteReport {
        status,
        published_entries,
        editorial_replacements,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_set_covers_the_agreed_routes() {
        let values = fixtures().unwrap();
        let routes = values
            .iter()
            .filter_map(|fixture| fixture.path.as_deref())
            .collect::<BTreeSet<_>>();
        for path in [
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
        ] {
            assert!(routes.contains(path), "missing {path}");
        }
    }
}
