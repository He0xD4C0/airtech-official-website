use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use thiserror::Error;
use uuid::Uuid;

const SEED_VERSION: i32 = 1;
const FIXTURE_UPDATED_AT: &str = "2026-09-02T00:00:00Z";

#[derive(Debug, Error)]
pub enum DevelopmentSeedError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error("development fixture {fixture_key} is already assigned to {actual_id}, expected {expected_id}")]
    LedgerIdentityMismatch {
        fixture_key: String,
        actual_id: Uuid,
        expected_id: Uuid,
    },
    #[error("refusing to replace non-development {entity_type} row {entity_id}")]
    OwnershipMismatch {
        entity_type: &'static str,
        entity_id: Uuid,
    },
    #[error("fixture timestamp is invalid")]
    InvalidFixtureTimestamp,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevelopmentSeedReport {
    pub status: &'static str,
    pub seed_version: i32,
    pub fixture_count: usize,
    pub news_count: usize,
    pub route_count: usize,
    pub changed_fixtures: usize,
    pub unchanged_fixtures: usize,
    pub products_created: usize,
}

#[derive(Clone)]
struct ContentFixture {
    fixture_key: &'static str,
    ledger_entity_type: &'static str,
    id: Uuid,
    kind: &'static str,
    slug: &'static str,
    title: &'static str,
    summary: &'static str,
    canonical_path: Option<&'static str>,
    page_slots: Value,
    news: Option<NewsFixture>,
}

#[derive(Clone)]
struct NewsFixture {
    category: &'static str,
    publication_at: &'static str,
}

struct ExistingContent {
    current_revision: i64,
    payload: Value,
    kind: String,
    data_origin: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ChangeState {
    Changed,
    Unchanged,
}

/// Populate only development-owned CMS fixtures. Product Master records are
/// deliberately outside this service and can only enter through validated
/// import/publishing paths.
pub async fn seed(
    pool: &PgPool,
    actor: &str,
) -> Result<DevelopmentSeedReport, DevelopmentSeedError> {
    let fixtures = content_fixtures();
    let news_count = fixtures
        .iter()
        .filter(|fixture| fixture.news.is_some())
        .count();
    let route_count = fixtures
        .iter()
        .filter(|fixture| fixture.canonical_path.is_some())
        .count();
    let mut transaction = pool.begin().await?;
    // Serialize concurrent CLI invocations so the first revision and ledger
    // ownership checks remain deterministic instead of racing on unique keys.
    sqlx::query("SELECT pg_advisory_xact_lock(731020260902::bigint)")
        .execute(&mut *transaction)
        .await?;
    let mut changed_fixtures = 0;
    let mut unchanged_fixtures = 0;

    for fixture in &fixtures {
        match seed_content(&mut transaction, fixture, actor).await? {
            ChangeState::Changed => changed_fixtures += 1,
            ChangeState::Unchanged => unchanged_fixtures += 1,
        }
    }

    match seed_general_information(&mut transaction, actor).await? {
        ChangeState::Changed => changed_fixtures += 1,
        ChangeState::Unchanged => unchanged_fixtures += 1,
    }

    sqlx::query(
        r#"INSERT INTO audit_log
           (id, actor, action, entity_type, entity_id, before_value,
            after_value, reason, request_id, occurred_at)
           VALUES ($1,$2,'devtools.cli.seed.completed','developmentFixture',
                   NULL,NULL,$3,$4,$5,now())"#,
    )
    .bind(Uuid::new_v4())
    .bind(actor)
    .bind(json!({
        "seedVersion": SEED_VERSION,
        "fixtureCount": fixtures.len() + 1,
        "newsCount": news_count,
        "routeCount": route_count,
        "changedFixtures": changed_fixtures,
        "unchangedFixtures": unchanged_fixtures,
        "productsCreated": 0
    }))
    .bind("Idempotent development CMS fixtures loaded; Product Master was not modified.")
    .bind(Uuid::new_v4())
    .execute(&mut *transaction)
    .await?;

    transaction.commit().await?;
    Ok(DevelopmentSeedReport {
        status: "completed",
        seed_version: SEED_VERSION,
        fixture_count: fixtures.len() + 1,
        news_count,
        route_count,
        changed_fixtures,
        unchanged_fixtures,
        products_created: 0,
    })
}

async fn seed_content(
    transaction: &mut Transaction<'_, Postgres>,
    fixture: &ContentFixture,
    actor: &str,
) -> Result<ChangeState, DevelopmentSeedError> {
    let checksum = checksum(&content_definition(fixture));
    validate_ledger_identity(transaction, fixture.fixture_key, fixture.id).await?;
    let ledger_checksum = ledger_checksum(transaction, fixture.fixture_key).await?;

    let existing = sqlx::query(
        "SELECT current_revision, payload, kind, data_origin FROM content_entries WHERE id=$1 FOR UPDATE",
    )
    .bind(fixture.id)
    .fetch_optional(&mut **transaction)
    .await?
    .map(|row| ExistingContent {
        current_revision: row.get("current_revision"),
        payload: row.get("payload"),
        kind: row.get("kind"),
        data_origin: row.get("data_origin"),
    });

    if let Some(entry) = existing.as_ref() {
        if entry.data_origin != "developmentFixture" {
            // Clearing `isPlaceholder` through Admin is an explicit editorial
            // takeover. The retained ledger proves this deterministic id was
            // originally created by this seed, so later seed runs must neither
            // overwrite the editorial record nor fail the whole seed.
            if ledger_checksum.is_some() && entry.data_origin == "editorial" {
                return Ok(ChangeState::Unchanged);
            }
            return Err(DevelopmentSeedError::OwnershipMismatch {
                entity_type: fixture.ledger_entity_type,
                entity_id: fixture.id,
            });
        }
        if entry.kind != fixture.kind && ledger_checksum.is_none() {
            return Err(DevelopmentSeedError::OwnershipMismatch {
                entity_type: fixture.ledger_entity_type,
                entity_id: fixture.id,
            });
        }
    }

    let current_revision = existing
        .as_ref()
        .map(|entry| entry.current_revision)
        .unwrap_or(1);
    let expected_current_payload = content_payload(fixture, current_revision)?;
    let stored_revision_payload = if existing.is_some() {
        sqlx::query_scalar::<_, Value>(
            "SELECT payload FROM content_revisions WHERE content_id=$1 AND revision=$2",
        )
        .bind(fixture.id)
        .bind(current_revision)
        .fetch_optional(&mut **transaction)
        .await?
    } else {
        None
    };
    let news_metadata_matches = current_news_metadata_matches(
        transaction,
        fixture,
        existing.as_ref().map(|_| current_revision),
    )
    .await?;
    let changed = existing.is_none()
        || ledger_checksum.as_deref() != Some(checksum.as_str())
        || stored_revision_payload.as_ref() != Some(&expected_current_payload)
        || !news_metadata_matches
        || existing
            .as_ref()
            .is_some_and(|entry| entry.payload != expected_current_payload);
    let revision = if existing.is_some() && changed {
        current_revision + 1
    } else {
        current_revision
    };
    let payload = content_payload(fixture, revision)?;
    let updated_at = fixture_timestamp()?;

    if existing.is_none() {
        sqlx::query(
            r#"INSERT INTO content_entries
               (id, kind, slug, locale, title, status, is_placeholder,
                current_revision, published_revision, scheduled_for, payload,
                updated_at, data_origin)
               VALUES ($1,$2,$3,'en',$4,'published',true,$5,$5,NULL,$6,$7,
                       'developmentFixture')"#,
        )
        .bind(fixture.id)
        .bind(fixture.kind)
        .bind(fixture.slug)
        .bind(fixture.title)
        .bind(revision)
        .bind(&payload)
        .bind(updated_at)
        .execute(&mut **transaction)
        .await?;
    }

    if changed {
        sqlx::query(
            r#"INSERT INTO content_revisions
               (content_id, revision, payload, created_by, created_at)
               VALUES ($1,$2,$3,$4,$5)"#,
        )
        .bind(fixture.id)
        .bind(revision)
        .bind(&payload)
        .bind(actor)
        .bind(updated_at)
        .execute(&mut **transaction)
        .await?;

        if existing.is_some() {
            sqlx::query(
                r#"UPDATE content_entries
                   SET kind=$2, slug=$3, locale='en', title=$4, status='published',
                       is_placeholder=true, current_revision=$5,
                       published_revision=$5, scheduled_for=NULL, payload=$6,
                       updated_at=$7
                   WHERE id=$1 AND data_origin='developmentFixture'"#,
            )
            .bind(fixture.id)
            .bind(fixture.kind)
            .bind(fixture.slug)
            .bind(fixture.title)
            .bind(revision)
            .bind(&payload)
            .bind(updated_at)
            .execute(&mut **transaction)
            .await?;
        }

        if let Some(news) = &fixture.news {
            sqlx::query(
                r#"INSERT INTO news
                   (content_id, revision, content_kind, category,
                    author_display_name, cover_media_asset_id, featured,
                    publication_at, reading_minutes, data_origin)
                   VALUES ($1,$2,'news',$3,'AIRTEKPOWER Development Fixture',NULL,
                           false,$4,2,'developmentFixture')"#,
            )
            .bind(fixture.id)
            .bind(revision)
            .bind(news.category)
            .bind(parse_timestamp(news.publication_at)?)
            .execute(&mut **transaction)
            .await?;
        }
    }

    if let Some(news) = &fixture.news {
        sqlx::query(
            r#"INSERT INTO news_working
               (content_id,content_kind,category,author_display_name,
                cover_media_asset_id,featured,publication_at,reading_minutes,data_origin,updated_at)
               VALUES ($1,'news',$2,'AIRTEKPOWER Development Fixture',NULL,false,$3,2,
                       'developmentFixture',$4)
               ON CONFLICT (content_id) DO UPDATE SET
                 category=EXCLUDED.category,
                 author_display_name=EXCLUDED.author_display_name,
                 cover_media_asset_id=EXCLUDED.cover_media_asset_id,
                 featured=EXCLUDED.featured,
                 publication_at=EXCLUDED.publication_at,
                 reading_minutes=EXCLUDED.reading_minutes,
                 updated_at=EXCLUDED.updated_at"#,
        )
        .bind(fixture.id)
        .bind(news.category)
        .bind(parse_timestamp(news.publication_at)?)
        .bind(updated_at)
        .execute(&mut **transaction)
        .await?;
    }

    if let Some(canonical_path) = fixture.canonical_path {
        sqlx::query(
            r#"INSERT INTO public_routes
               (id, entity_type, entity_id, locale, canonical_path, indexable, updated_at)
               VALUES ($1,'content',$2,'en',$3,false,$4)
               ON CONFLICT (entity_type, entity_id, locale) DO UPDATE
               SET canonical_path=EXCLUDED.canonical_path,
                   indexable=false,
                   updated_at=EXCLUDED.updated_at"#,
        )
        .bind(route_id(fixture.id))
        .bind(fixture.id)
        .bind(canonical_path)
        .bind(updated_at)
        .execute(&mut **transaction)
        .await?;
    }

    upsert_ledger(
        transaction,
        fixture.fixture_key,
        fixture.ledger_entity_type,
        fixture.id,
        &checksum,
    )
    .await?;
    Ok(if changed {
        ChangeState::Changed
    } else {
        ChangeState::Unchanged
    })
}

async fn seed_general_information(
    transaction: &mut Transaction<'_, Postgres>,
    actor: &str,
) -> Result<ChangeState, DevelopmentSeedError> {
    const FIXTURE_KEY: &str = "development/general-information/site/en";
    let id = fixture_id(0x01);
    let payload = general_information_payload();
    let checksum = checksum(&json!({
        "seedVersion": SEED_VERSION,
        "scope": "site",
        "locale": "en",
        "payload": payload,
        "isPlaceholder": true,
        "dataOrigin": "developmentFixture"
    }));
    validate_ledger_identity(transaction, FIXTURE_KEY, id).await?;
    let ledger_checksum = ledger_checksum(transaction, FIXTURE_KEY).await?;
    let existing = sqlx::query(
        "SELECT current_revision, payload, data_origin FROM general_information WHERE id=$1 FOR UPDATE",
    )
    .bind(id)
    .fetch_optional(&mut **transaction)
    .await?;
    if let Some(row) = existing.as_ref() {
        let origin = row.get::<String, _>("data_origin");
        if origin != "developmentFixture" {
            if ledger_checksum.is_some() && origin == "editorial" {
                return Ok(ChangeState::Unchanged);
            }
            return Err(DevelopmentSeedError::OwnershipMismatch {
                entity_type: "generalInformation",
                entity_id: id,
            });
        }
    }
    let current_revision = existing
        .as_ref()
        .map(|row| row.get::<i64, _>("current_revision"))
        .unwrap_or(1);
    let stored_revision_payload = if existing.is_some() {
        sqlx::query_scalar::<_, Value>(
            "SELECT payload FROM general_information_revisions WHERE general_information_id=$1 AND revision=$2",
        )
        .bind(id)
        .bind(current_revision)
        .fetch_optional(&mut **transaction)
        .await?
    } else {
        None
    };
    let changed = existing.is_none()
        || ledger_checksum.as_deref() != Some(checksum.as_str())
        || stored_revision_payload.as_ref() != Some(&payload)
        || existing
            .as_ref()
            .is_some_and(|row| row.get::<Value, _>("payload") != payload);
    let revision = if existing.is_some() && changed {
        current_revision + 1
    } else {
        current_revision
    };
    let updated_at = fixture_timestamp()?;

    if existing.is_none() {
        sqlx::query(
            r#"INSERT INTO general_information
               (id, scope, locale, status, is_placeholder, data_origin,
                current_revision, published_revision, scheduled_for, payload,
                updated_by, updated_at)
               VALUES ($1,'site','en','published',true,'developmentFixture',
                       $2,$2,NULL,$3,$4,$5)"#,
        )
        .bind(id)
        .bind(revision)
        .bind(&payload)
        .bind(actor)
        .bind(updated_at)
        .execute(&mut **transaction)
        .await?;
    }

    if changed {
        sqlx::query(
            r#"INSERT INTO general_information_revisions
               (general_information_id,revision,payload,locale,is_placeholder,data_origin,
                created_by,created_at)
               VALUES ($1,$2,$3,'en',true,'developmentFixture',$4,$5)"#,
        )
        .bind(id)
        .bind(revision)
        .bind(&payload)
        .bind(actor)
        .bind(updated_at)
        .execute(&mut **transaction)
        .await?;
        if existing.is_some() {
            sqlx::query(
                r#"UPDATE general_information
                   SET scope='site', locale='en', status='published',
                       is_placeholder=true, current_revision=$2,
                       published_revision=$2, scheduled_for=NULL, payload=$3,
                       updated_by=$4, updated_at=$5
                   WHERE id=$1 AND data_origin='developmentFixture'"#,
            )
            .bind(id)
            .bind(revision)
            .bind(&payload)
            .bind(actor)
            .bind(updated_at)
            .execute(&mut **transaction)
            .await?;
        }
    }

    upsert_ledger(
        transaction,
        FIXTURE_KEY,
        "generalInformation",
        id,
        &checksum,
    )
    .await?;
    Ok(if changed {
        ChangeState::Changed
    } else {
        ChangeState::Unchanged
    })
}

async fn current_news_metadata_matches(
    transaction: &mut Transaction<'_, Postgres>,
    fixture: &ContentFixture,
    revision: Option<i64>,
) -> Result<bool, DevelopmentSeedError> {
    let Some(expected) = &fixture.news else {
        return Ok(true);
    };
    let Some(revision) = revision else {
        return Ok(false);
    };
    let publication_at = parse_timestamp(expected.publication_at)?;
    let row = sqlx::query(
        r#"SELECT category, author_display_name, featured, publication_at,
                  reading_minutes
           FROM news WHERE content_id=$1 AND revision=$2"#,
    )
    .bind(fixture.id)
    .bind(revision)
    .fetch_optional(&mut **transaction)
    .await?;
    Ok(row.is_some_and(|row| {
        row.get::<String, _>("category") == expected.category
            && row
                .get::<Option<String>, _>("author_display_name")
                .as_deref()
                == Some("AIRTEKPOWER Development Fixture")
            && !row.get::<bool, _>("featured")
            && row.get::<Option<DateTime<Utc>>, _>("publication_at") == Some(publication_at)
            && row.get::<Option<i32>, _>("reading_minutes") == Some(2)
    }))
}

async fn validate_ledger_identity(
    transaction: &mut Transaction<'_, Postgres>,
    fixture_key: &str,
    expected_id: Uuid,
) -> Result<(), DevelopmentSeedError> {
    let existing = sqlx::query_scalar::<_, Uuid>(
        "SELECT entity_id FROM development_fixture_ledger WHERE fixture_key=$1 FOR UPDATE",
    )
    .bind(fixture_key)
    .fetch_optional(&mut **transaction)
    .await?;
    if let Some(actual_id) = existing.filter(|actual_id| *actual_id != expected_id) {
        return Err(DevelopmentSeedError::LedgerIdentityMismatch {
            fixture_key: fixture_key.to_owned(),
            actual_id,
            expected_id,
        });
    }
    Ok(())
}

async fn ledger_checksum(
    transaction: &mut Transaction<'_, Postgres>,
    fixture_key: &str,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT checksum FROM development_fixture_ledger WHERE fixture_key=$1")
        .bind(fixture_key)
        .fetch_optional(&mut **transaction)
        .await
}

async fn upsert_ledger(
    transaction: &mut Transaction<'_, Postgres>,
    fixture_key: &str,
    entity_type: &str,
    entity_id: Uuid,
    checksum: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO development_fixture_ledger
           (fixture_key, entity_type, entity_id, locale, seed_version,
            checksum, loaded_at, updated_at)
           VALUES ($1,$2,$3,'en',$4,$5,now(),now())
           ON CONFLICT (fixture_key) DO UPDATE
           SET entity_type=EXCLUDED.entity_type,
               entity_id=EXCLUDED.entity_id,
               locale=EXCLUDED.locale,
               seed_version=EXCLUDED.seed_version,
               checksum=EXCLUDED.checksum,
               updated_at=now()"#,
    )
    .bind(fixture_key)
    .bind(entity_type)
    .bind(entity_id)
    .bind(SEED_VERSION)
    .bind(checksum)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

fn content_definition(fixture: &ContentFixture) -> Value {
    json!({
        "seedVersion": SEED_VERSION,
        "kind": fixture.kind,
        "slug": fixture.slug,
        "locale": "en",
        "title": fixture.title,
        "summary": fixture.summary,
        "canonicalPath": fixture.canonical_path,
        "pageSlots": fixture.page_slots,
        "news": fixture.news.as_ref().map(|news| json!({
            "category": news.category,
            "publicationAt": news.publication_at,
            "authorDisplayName": "AIRTEKPOWER Development Fixture",
            "featured": false,
            "readingMinutes": 2
        })),
        "isPlaceholder": true,
        "indexable": false,
        "dataOrigin": "developmentFixture"
    })
}

fn content_payload(fixture: &ContentFixture, revision: i64) -> Result<Value, DevelopmentSeedError> {
    let description = format!("{} Development-only placeholder content.", fixture.summary);
    Ok(json!({
        "id": fixture.id,
        "kind": fixture.kind,
        "slug": fixture.slug,
        "locale": "en",
        "title": fixture.title,
        "summary": fixture.summary,
        "body": {
            "schemaVersion": 1,
            "doc": {
                "type": "doc",
                "attrs": { "pageSlots": fixture.page_slots },
                "content": [{
                    "type": "paragraph",
                    "content": [{
                        "type": "text",
                        "text": "This development fixture verifies database-backed rendering. Replace it with reviewed editorial content before launch."
                    }]
                }]
            }
        },
        "seo": {
            "title": format!("{} | Development Preview", fixture.title),
            "description": description,
            "canonicalPath": fixture.canonical_path,
            "indexable": false
        },
        "status": "published",
        "isPlaceholder": true,
        "currentRevision": revision,
        "publishedRevision": revision,
        "scheduledFor": null,
        "updatedAt": FIXTURE_UPDATED_AT
    }))
}

fn general_information_payload() -> Value {
    json!({
        "brandName": "AIRTEKPOWER",
        "brandLine": "Redefining Airflow with Smart, Green Technology",
        "homePath": "/en",
        "footerStatement": "Development fixture content. Replace all placeholders with reviewed, source-backed publication data before launch.",
        "copyrightText": "© {year} AIRTEKPOWER. Development fixture.",
        "defaultSeo": {
            "title": "AIRTEKPOWER | Development Preview",
            "description": "A database-backed development preview for the AIRTEKPOWER public website."
        },
        "organization": {
            "name": "AIRTEKPOWER"
        },
        "navigationCta": {
            "label": "Request a Quote",
            "href": "/en/request-a-quote"
        },
        "productCategories": product_category_presentations()
    })
}

fn product_category_presentations() -> Value {
    json!([
        {
            "code": "centrifugal",
            "slug": "centrifugal",
            "name": "Centrifugal",
            "description": "Development category presentation; models appear only after validated Product Master publication.",
            "sortOrder": 10
        },
        {
            "code": "axial",
            "slug": "axial",
            "name": "Axial",
            "description": "Development category presentation; models appear only after validated Product Master publication.",
            "sortOrder": 20
        },
        {
            "code": "crossFlow",
            "slug": "cross-flow",
            "name": "Cross-flow",
            "description": "Development category presentation; models appear only after validated Product Master publication.",
            "sortOrder": 30
        },
        {
            "code": "inlineDuct",
            "slug": "inline-duct",
            "name": "Inline Duct",
            "description": "Development category presentation; models appear only after validated Product Master publication.",
            "sortOrder": 40
        },
        {
            "code": "motors",
            "slug": "motors",
            "name": "Motors",
            "description": "Development category presentation; models appear only after validated Product Master publication.",
            "sortOrder": 50
        }
    ])
}

fn content_fixtures() -> Vec<ContentFixture> {
    let mut fixtures = vec![
        ContentFixture {
            fixture_key: "development/content/home/en",
            ledger_entity_type: "content",
            id: fixture_id(0x10),
            kind: "home",
            slug: "index",
            title: "AIRTEKPOWER Development Preview",
            summary: "Database-backed public website development preview.",
            canonical_path: Some("/en"),
            page_slots: json!({
                "templateKey": "home",
                "eyebrow": "Development fixture",
                "hero": {
                    "eyebrow": "Development fixture",
                    "title": "AIRTEKPOWER Development Preview",
                    "description": "This placeholder verifies the public SSR content pipeline without publishing unreviewed product or business claims."
                },
                "primaryCta": {
                    "eyebrow": "Development fixture",
                    "title": "Prepare a structured inquiry",
                    "description": "Use the RFQ flow to test the development experience.",
                    "label": "Request a Quote",
                    "href": "/en/request-a-quote"
                },
                "sections": [{
                    "id": "database-backed",
                    "eyebrow": "Development fixture",
                    "title": "Database-backed content",
                    "description": "This section is loaded from PostgreSQL and is intentionally non-indexable."
                }]
            }),
            news: None,
        },
        ContentFixture {
            fixture_key: "development/content/news-index/en",
            ledger_entity_type: "content",
            id: fixture_id(0x11),
            // The News landing page is ordinary page content. Only individual
            // News records use the revision-metadata extension and its dedicated
            // Admin API.
            kind: "home",
            slug: "news-index",
            title: "News Development Preview",
            summary: "Development fixtures for the database-backed News experience.",
            canonical_path: Some("/en/resources/news"),
            page_slots: json!({
                "templateKey": "newsIndex",
                "eyebrow": "Development fixture",
                "hero": {
                    "eyebrow": "Development fixture",
                    "title": "News Development Preview",
                    "description": "Six non-indexable placeholder records exercise the News list and detail views."
                },
                "breadcrumbs": [
                    { "label": "Home", "href": "/en" },
                    { "label": "News" }
                ]
            }),
            news: None,
        },
        shell_fixture(
            0x12,
            "development/content/navigation/en",
            "navigation",
            "primary",
            "Primary navigation",
            json!({
                "items": [
                    { "label": "Home", "href": "/en" },
                    { "label": "Products", "href": "/en/products" },
                    { "label": "Solutions", "href": "/en/solutions" },
                    { "label": "Technology", "href": "/en/technology" },
                    { "label": "Resources", "href": "/en/resources/articles" },
                    { "label": "Company", "href": "/en/company/about" },
                    { "label": "News", "href": "/en/resources/news" }
                ]
            }),
        ),
        shell_fixture(
            0x13,
            "development/content/footer/en",
            "footer",
            "primary",
            "Primary footer",
            json!({
                "columns": [
                    {
                        "title": "Explore",
                        "links": [
                            { "label": "Products", "href": "/en/products" },
                            { "label": "Solutions", "href": "/en/solutions" },
                            { "label": "Technology", "href": "/en/technology" }
                        ]
                    },
                    {
                        "title": "Resources",
                        "links": [
                            { "label": "Articles", "href": "/en/resources/articles" },
                            { "label": "News", "href": "/en/resources/news" },
                            { "label": "Downloads", "href": "/en/resources/downloads" }
                        ]
                    },
                    {
                        "title": "Company",
                        "links": [
                            { "label": "About", "href": "/en/company/about" },
                            { "label": "Contact", "href": "/en/company/contact" }
                        ]
                    }
                ],
                "legalLinks": [
                    { "label": "Privacy", "href": "/en/privacy" },
                    { "label": "Terms", "href": "/en/terms" },
                    { "label": "Cookie Settings", "href": "/en/cookie-settings" }
                ]
            }),
        ),
    ];
    fixtures.extend(public_page_fixtures());

    let news = [
        (
            0x20,
            "selection-brief-development-preview",
            "Development Preview: Preparing a Selection Brief",
            "A placeholder record for testing structured engineering-content layouts.",
            "Selection workflow",
            "2026-08-27T08:00:00Z",
        ),
        (
            0x21,
            "operating-point-development-preview",
            "Development Preview: Describing an Operating Point",
            "A placeholder record for testing technical terminology and content relationships.",
            "Engineering basics",
            "2026-08-26T08:00:00Z",
        ),
        (
            0x22,
            "comparison-development-preview",
            "Development Preview: Building a Comparison Checklist",
            "A placeholder record for testing comparison-oriented content cards.",
            "Product discovery",
            "2026-08-25T08:00:00Z",
        ),
        (
            0x23,
            "product-data-development-preview",
            "Development Preview: Reviewing Product Data",
            "A placeholder record for testing product-data governance messaging.",
            "Data quality",
            "2026-08-24T08:00:00Z",
        ),
        (
            0x24,
            "units-development-preview",
            "Development Preview: Recording Units and Conditions",
            "A placeholder record for testing evidence-led editorial structures.",
            "Engineering basics",
            "2026-08-23T08:00:00Z",
        ),
        (
            0x25,
            "rfq-development-preview",
            "Development Preview: From Discovery to RFQ",
            "A placeholder record for testing the content-to-inquiry journey.",
            "RFQ workflow",
            "2026-08-22T08:00:00Z",
        ),
    ];
    fixtures.extend(news.into_iter().map(
        |(suffix, slug, title, summary, category, publication_at)| ContentFixture {
            fixture_key: match suffix {
                0x20 => "development/news/selection-brief/en",
                0x21 => "development/news/operating-point/en",
                0x22 => "development/news/comparison/en",
                0x23 => "development/news/product-data/en",
                0x24 => "development/news/units/en",
                _ => "development/news/rfq/en",
            },
            ledger_entity_type: "news",
            id: fixture_id(suffix),
            kind: "news",
            slug,
            title,
            summary,
            canonical_path: Some(match suffix {
                0x20 => "/en/resources/news/selection-brief-development-preview",
                0x21 => "/en/resources/news/operating-point-development-preview",
                0x22 => "/en/resources/news/comparison-development-preview",
                0x23 => "/en/resources/news/product-data-development-preview",
                0x24 => "/en/resources/news/units-development-preview",
                _ => "/en/resources/news/rfq-development-preview",
            }),
            page_slots: json!({
                "templateKey": "newsDetail",
                "eyebrow": "Development fixture",
                "hero": {
                    "eyebrow": category,
                    "title": title,
                    "description": summary
                },
                "breadcrumbs": [
                    { "label": "Home", "href": "/en" },
                    { "label": "News", "href": "/en/resources/news" },
                    { "label": title }
                ],
                "sections": [{
                    "id": "fixture-notice",
                    "eyebrow": "Development fixture",
                    "title": "Placeholder content",
                    "description": "This record contains no approved product specification or publication claim."
                }],
                "primaryCta": {
                    "eyebrow": "Next step",
                    "title": "Continue with a structured inquiry",
                    "description": "Use the development RFQ flow without treating this fixture as engineering advice.",
                    "label": "Request a Quote",
                    "href": "/en/request-a-quote"
                }
            }),
            news: Some(NewsFixture {
                category,
                publication_at,
            }),
        },
    ));
    fixtures
}

#[allow(clippy::too_many_arguments)]
fn public_page_fixture(
    suffix: u128,
    fixture_key: &'static str,
    kind: &'static str,
    slug: &'static str,
    title: &'static str,
    canonical_path: &'static str,
    template_key: &'static str,
    eyebrow: &'static str,
    description: &'static str,
) -> ContentFixture {
    ContentFixture {
        fixture_key,
        ledger_entity_type: "content",
        id: fixture_id(suffix),
        kind,
        slug,
        title,
        summary: description,
        canonical_path: Some(canonical_path),
        page_slots: json!({
            "templateKey": template_key,
            "eyebrow": eyebrow,
            "hero": {
                "eyebrow": eyebrow,
                "title": title,
                "description": description
            },
            "breadcrumbs": [
                { "label": "Home", "href": "/en" },
                { "label": title }
            ],
            "sections": [{
                "id": "database-placeholder",
                "eyebrow": "Development fixture",
                "title": "Database-backed placeholder",
                "description": "This page block verifies the SSR and admin editing pipeline. It is non-indexable and must be replaced before launch."
            }],
            "primaryCta": {
                "eyebrow": "Development fixture",
                "title": "Continue to a structured inquiry",
                "description": "Use the development RFQ workflow without treating placeholder text as product guidance.",
                "label": "Request a Quote",
                "href": "/en/request-a-quote"
            }
        }),
        news: None,
    }
}

fn public_page_fixtures() -> Vec<ContentFixture> {
    let mut pages = vec![
        public_page_fixture(
            0x100,
            "development/content/products-index/en",
            "home",
            "products-index",
            "Products Development Preview",
            "/en/products",
            "productIndex",
            "Products",
            "Browse normalized product categories; models appear only after validated Product Master publication.",
        ),
        public_page_fixture(
            0x101,
            "development/content/products-centrifugal/en",
            "home",
            "products-centrifugal",
            "Centrifugal Development Preview",
            "/en/products/centrifugal",
            "productFamily",
            "Product category",
            "Development category presentation with no synthetic models or specifications.",
        ),
        public_page_fixture(
            0x102,
            "development/content/products-axial/en",
            "home",
            "products-axial",
            "Axial Development Preview",
            "/en/products/axial",
            "productFamily",
            "Product category",
            "Development category presentation with no synthetic models or specifications.",
        ),
        public_page_fixture(
            0x103,
            "development/content/products-cross-flow/en",
            "home",
            "products-cross-flow",
            "Cross-flow Development Preview",
            "/en/products/cross-flow",
            "productFamily",
            "Product category",
            "Development category presentation with no synthetic models or specifications.",
        ),
        public_page_fixture(
            0x104,
            "development/content/products-inline-duct/en",
            "home",
            "products-inline-duct",
            "Inline Duct Development Preview",
            "/en/products/inline-duct",
            "productFamily",
            "Product category",
            "Development category presentation with no synthetic models or specifications.",
        ),
        public_page_fixture(
            0x105,
            "development/content/products-motors/en",
            "home",
            "products-motors",
            "Motors Development Preview",
            "/en/products/motors",
            "productFamily",
            "Product category",
            "Development category presentation with no synthetic models or specifications.",
        ),
        public_page_fixture(
            0x106,
            "development/content/fan-selector/en",
            "home",
            "fan-selector",
            "Fan Selector Development Preview",
            "/en/products/selector",
            "selector",
            "Product discovery",
            "The client-side selector starts with no validated candidates until published product data is available.",
        ),
        public_page_fixture(
            0x107,
            "development/content/product-compare/en",
            "home",
            "product-compare",
            "Product Compare Development Preview",
            "/en/products/compare",
            "compare",
            "Product discovery",
            "The comparison workspace contains no fixed rows, scores, or synthetic product facts.",
        ),
        public_page_fixture(
            0x110,
            "development/content/solutions-index/en",
            "solution",
            "index",
            "Solutions Development Preview",
            "/en/solutions",
            "solutionIndex",
            "Solutions",
            "Application routes are placeholders and do not establish model suitability.",
        ),
        public_page_fixture(
            0x111,
            "development/content/solution-hvac/en",
            "solution",
            "hvac",
            "HVAC Development Preview",
            "/en/solutions/hvac",
            "solutionDetail",
            "Solution route",
            "This placeholder does not assert suitability for any product or operating condition.",
        ),
        public_page_fixture(
            0x112,
            "development/content/solution-refrigeration/en",
            "solution",
            "refrigeration",
            "Refrigeration Development Preview",
            "/en/solutions/refrigeration",
            "solutionDetail",
            "Solution route",
            "This placeholder does not assert suitability for any product or operating condition.",
        ),
        public_page_fixture(
            0x113,
            "development/content/solution-data-centers/en",
            "solution",
            "data-centers",
            "Data Centers Development Preview",
            "/en/solutions/data-centers",
            "solutionDetail",
            "Solution route",
            "This placeholder does not assert suitability for any product or operating condition.",
        ),
        public_page_fixture(
            0x114,
            "development/content/solution-energy-storage/en",
            "solution",
            "energy-storage",
            "Energy Storage Development Preview",
            "/en/solutions/energy-storage",
            "solutionDetail",
            "Solution route",
            "This placeholder does not assert suitability for any product or operating condition.",
        ),
        public_page_fixture(
            0x115,
            "development/content/solution-air-purification/en",
            "solution",
            "air-purification",
            "Air Purification Development Preview",
            "/en/solutions/air-purification",
            "solutionDetail",
            "Solution route",
            "This placeholder does not assert suitability for any product or operating condition.",
        ),
        public_page_fixture(
            0x116,
            "development/content/solution-cleanroom/en",
            "solution",
            "cleanroom",
            "Cleanroom Development Preview",
            "/en/solutions/cleanroom",
            "solutionDetail",
            "Solution route",
            "This placeholder does not assert suitability for any product or operating condition.",
        ),
        public_page_fixture(
            0x117,
            "development/content/solution-industrial-ventilation/en",
            "solution",
            "industrial-ventilation",
            "Industrial Ventilation Development Preview",
            "/en/solutions/industrial-ventilation",
            "solutionDetail",
            "Solution route",
            "This placeholder does not assert suitability for any product or operating condition.",
        ),
        public_page_fixture(
            0x118,
            "development/content/solution-commercial-buildings/en",
            "solution",
            "commercial-buildings",
            "Commercial Buildings Development Preview",
            "/en/solutions/commercial-buildings",
            "solutionDetail",
            "Solution route",
            "This placeholder does not assert suitability for any product or operating condition.",
        ),
        public_page_fixture(
            0x120,
            "development/content/technology-index/en",
            "technology",
            "index",
            "Technology Development Preview",
            "/en/technology",
            "technologyIndex",
            "Technology",
            "Technology routes are non-indexable development placeholders pending reviewed evidence.",
        ),
        public_page_fixture(
            0x121,
            "development/content/technology-ec-motor/en",
            "technology",
            "ec-motor",
            "EC Motor Development Preview",
            "/en/technology/ec-motor",
            "technologyDetail",
            "Technology route",
            "This placeholder contains no model-specific motor, control, or performance claim.",
        ),
        public_page_fixture(
            0x122,
            "development/content/technology-aerodynamics/en",
            "technology",
            "aerodynamics",
            "Aerodynamics Development Preview",
            "/en/technology/aerodynamics",
            "technologyDetail",
            "Technology route",
            "This placeholder contains no model-specific aerodynamic or performance claim.",
        ),
        public_page_fixture(
            0x123,
            "development/content/technology-airflow-pressure/en",
            "technology",
            "airflow-and-pressure",
            "Airflow and Pressure Development Preview",
            "/en/technology/airflow-and-pressure",
            "technologyDetail",
            "Technology route",
            "This placeholder contains no operating point, curve, unit conversion, or test claim.",
        ),
        public_page_fixture(
            0x124,
            "development/content/technology-control/en",
            "technology",
            "control",
            "Control Development Preview",
            "/en/technology/control",
            "technologyDetail",
            "Technology route",
            "This placeholder contains no wiring, interface, compatibility, or safety instruction.",
        ),
        public_page_fixture(
            0x125,
            "development/content/technology-efficiency/en",
            "technology",
            "efficiency",
            "Efficiency Development Preview",
            "/en/technology/efficiency",
            "technologyDetail",
            "Technology route",
            "This placeholder contains no efficiency value, saving estimate, or performance claim.",
        ),
        public_page_fixture(
            0x126,
            "development/content/technology-noise-vibration/en",
            "technology",
            "noise-and-vibration",
            "Noise and Vibration Development Preview",
            "/en/technology/noise-and-vibration",
            "technologyDetail",
            "Technology route",
            "This placeholder contains no acoustic value, measurement setup, or product comparison.",
        ),
        public_page_fixture(
            0x130,
            "development/content/articles-index/en",
            "article",
            "index",
            "Articles Development Preview",
            "/en/resources/articles",
            "articleIndex",
            "Resources",
            "The article collection is database-backed and intentionally empty until editorial records are reviewed.",
        ),
        public_page_fixture(
            0x131,
            "development/content/faqs-index/en",
            "faq",
            "index",
            "FAQ Development Preview",
            "/en/resources/faqs",
            "faqIndex",
            "Resources",
            "The FAQ collection is database-backed and contains no invented questions or answers.",
        ),
        public_page_fixture(
            0x132,
            "development/content/case-studies-index/en",
            "caseStudy",
            "index",
            "Case Studies Development Preview",
            "/en/resources/case-studies",
            "caseStudyIndex",
            "Resources",
            "The case-study collection remains empty until evidence and publication permissions are approved.",
        ),
        public_page_fixture(
            0x133,
            "development/content/downloads-index/en",
            "download",
            "index",
            "Downloads Development Preview",
            "/en/resources/downloads",
            "downloadIndex",
            "Resources",
            "The download collection contains no fabricated files, versions, certificates, or model links.",
        ),
        public_page_fixture(
            0x140,
            "development/content/company-about/en",
            "company",
            "about",
            "About Development Preview",
            "/en/company/about",
            "about",
            "Company",
            "This non-indexable placeholder avoids unreviewed company metrics, certifications, and legal claims.",
        ),
        public_page_fixture(
            0x141,
            "development/content/company-contact/en",
            "company",
            "contact",
            "Contact Development Preview",
            "/en/company/contact",
            "contact",
            "Company",
            "This placeholder exercises the Contact form without publishing unconfirmed personal contact details.",
        ),
        public_page_fixture(
            0x150,
            "development/content/rfq-index/en",
            "home",
            "rfq-index",
            "Request a Quote Development Preview",
            "/en/request-a-quote",
            "rfqRouter",
            "RFQ",
            "Choose a structured inquiry path; development fixtures do not imply response times or product suitability.",
        ),
        public_page_fixture(
            0x151,
            "development/content/rfq-product/en",
            "home",
            "rfq-product",
            "Product RFQ Development Preview",
            "/en/request-a-quote/product",
            "rfqForm",
            "RFQ",
            "The form requires a published product context and accepts no public attachment upload.",
        ),
        public_page_fixture(
            0x152,
            "development/content/rfq-selection/en",
            "home",
            "rfq-selection",
            "Selection RFQ Development Preview",
            "/en/request-a-quote/selection",
            "rfqForm",
            "RFQ",
            "The form collects structured selection context without inventing a recommended model.",
        ),
        public_page_fixture(
            0x153,
            "development/content/rfq-project/en",
            "home",
            "rfq-project",
            "Project RFQ Development Preview",
            "/en/request-a-quote/project",
            "rfqForm",
            "RFQ",
            "The form exercises project inquiry fields without making delivery or engineering commitments.",
        ),
        public_page_fixture(
            0x154,
            "development/content/rfq-replacement/en",
            "home",
            "rfq-replacement",
            "Replacement RFQ Development Preview",
            "/en/request-a-quote/replacement",
            "rfqForm",
            "RFQ",
            "The form exercises replacement inquiry fields without asserting compatibility.",
        ),
        public_page_fixture(
            0x160,
            "development/content/search/en",
            "home",
            "search",
            "Search Development Preview",
            "/en/search",
            "search",
            "Search",
            "Search returns only published projections and this route remains non-indexable.",
        ),
        public_page_fixture(
            0x170,
            "development/content/privacy/en",
            "legal",
            "privacy",
            "Privacy Development Preview",
            "/en/privacy",
            "legal",
            "Legal",
            "Development placeholder only; it is not an approved privacy notice.",
        ),
        public_page_fixture(
            0x171,
            "development/content/terms/en",
            "legal",
            "terms",
            "Terms Development Preview",
            "/en/terms",
            "legal",
            "Legal",
            "Development placeholder only; it is not an approved terms document.",
        ),
        public_page_fixture(
            0x172,
            "development/content/cookie-settings/en",
            "legal",
            "cookie-settings",
            "Cookie Settings Development Preview",
            "/en/cookie-settings",
            "legal",
            "Legal",
            "Development placeholder for consent controls; it is not a legal policy.",
        ),
    ];

    if let Some(products) = pages
        .iter_mut()
        .find(|page| page.canonical_path == Some("/en/products"))
    {
        products.page_slots["categories"] = product_category_presentations();
    }
    let categories = [
        ("/en/products/centrifugal", "centrifugal"),
        ("/en/products/axial", "axial"),
        ("/en/products/cross-flow", "crossFlow"),
        ("/en/products/inline-duct", "inlineDuct"),
        ("/en/products/motors", "motors"),
    ];
    for (path, code) in categories {
        if let Some(page) = pages
            .iter_mut()
            .find(|page| page.canonical_path == Some(path))
        {
            page.page_slots["category"] = product_category_presentations()
                .as_array()
                .and_then(|categories| {
                    categories
                        .iter()
                        .find(|category| category["code"].as_str() == Some(code))
                })
                .cloned()
                .unwrap_or(Value::Null);
        }
    }
    pages
}

fn shell_fixture(
    suffix: u128,
    fixture_key: &'static str,
    kind: &'static str,
    slug: &'static str,
    title: &'static str,
    attrs: Value,
) -> ContentFixture {
    ContentFixture {
        fixture_key,
        ledger_entity_type: "content",
        id: fixture_id(suffix),
        kind,
        slug,
        title,
        summary: "Development-only site shell configuration.",
        canonical_path: None,
        page_slots: attrs,
        news: None,
    }
}

fn fixture_id(suffix: u128) -> Uuid {
    Uuid::from_u128(0xd300_0000_0000_4000_8000_0000_0000_0000 | suffix)
}

fn route_id(entity_id: Uuid) -> Uuid {
    Uuid::from_u128(entity_id.as_u128() ^ 0x0000_0000_0000_0000_1000_0000_0000_0000)
}

fn checksum(value: &Value) -> String {
    let digest = Sha256::digest(serde_json::to_vec(value).expect("fixture JSON is serializable"));
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn fixture_timestamp() -> Result<DateTime<Utc>, DevelopmentSeedError> {
    parse_timestamp(FIXTURE_UPDATED_AT)
}

fn parse_timestamp(value: &str) -> Result<DateTime<Utc>, DevelopmentSeedError> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| DevelopmentSeedError::InvalidFixtureTimestamp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_contract_contains_six_safe_news_fixtures_and_no_products() {
        let fixtures = content_fixtures();
        let news = fixtures
            .iter()
            .filter(|fixture| fixture.news.is_some())
            .collect::<Vec<_>>();
        assert_eq!(news.len(), 6);
        assert!(fixtures
            .iter()
            .all(|fixture| fixture.ledger_entity_type != "product"));
        for fixture in news {
            let payload = content_payload(fixture, 1).unwrap();
            assert_eq!(payload["isPlaceholder"], true);
            assert_eq!(payload["seo"]["indexable"], false);
            assert_eq!(payload["status"], "published");
        }
    }

    #[test]
    fn site_shell_payload_matches_the_public_contract() {
        let information = general_information_payload();
        assert_eq!(information["brandName"], "AIRTEKPOWER");
        assert_eq!(information["homePath"], "/en");
        assert!(information["defaultSeo"].is_object());
        assert!(information["organization"].is_object());
        assert!(information["navigationCta"].is_object());
        assert_eq!(
            information["productCategories"].as_array().map(Vec::len),
            Some(5)
        );

        let fixtures = content_fixtures();
        let navigation = fixtures
            .iter()
            .find(|fixture| fixture.kind == "navigation")
            .unwrap();
        let footer = fixtures
            .iter()
            .find(|fixture| fixture.kind == "footer")
            .unwrap();
        assert!(navigation.page_slots["items"].is_array());
        assert!(footer.page_slots["columns"].is_array());
        assert!(footer.page_slots["legalLinks"].is_array());
    }

    #[test]
    fn every_planned_static_public_route_has_a_page_block() {
        let fixtures = content_fixtures();
        let paths = fixtures
            .iter()
            .filter_map(|fixture| fixture.canonical_path)
            .collect::<std::collections::BTreeSet<_>>();
        for required in [
            "/en",
            "/en/products",
            "/en/products/centrifugal",
            "/en/products/axial",
            "/en/products/cross-flow",
            "/en/products/inline-duct",
            "/en/products/motors",
            "/en/products/selector",
            "/en/products/compare",
            "/en/solutions",
            "/en/solutions/hvac",
            "/en/solutions/refrigeration",
            "/en/solutions/data-centers",
            "/en/solutions/energy-storage",
            "/en/solutions/air-purification",
            "/en/solutions/cleanroom",
            "/en/solutions/industrial-ventilation",
            "/en/solutions/commercial-buildings",
            "/en/technology",
            "/en/technology/ec-motor",
            "/en/technology/aerodynamics",
            "/en/technology/airflow-and-pressure",
            "/en/technology/control",
            "/en/technology/efficiency",
            "/en/technology/noise-and-vibration",
            "/en/resources/articles",
            "/en/resources/news",
            "/en/resources/faqs",
            "/en/resources/case-studies",
            "/en/resources/downloads",
            "/en/company/about",
            "/en/company/contact",
            "/en/request-a-quote",
            "/en/request-a-quote/product",
            "/en/request-a-quote/selection",
            "/en/request-a-quote/project",
            "/en/request-a-quote/replacement",
            "/en/search",
            "/en/privacy",
            "/en/terms",
            "/en/cookie-settings",
        ] {
            assert!(paths.contains(required), "missing public route {required}");
        }
        for fixture in fixtures
            .iter()
            .filter(|fixture| fixture.canonical_path.is_some())
        {
            let payload = content_payload(fixture, 1).unwrap();
            assert!(
                payload["body"]["doc"]["attrs"]["pageSlots"]["templateKey"].is_string(),
                "missing templateKey for {}",
                fixture.fixture_key
            );
            assert_eq!(payload["isPlaceholder"], true);
            assert_eq!(payload["seo"]["indexable"], false);
        }
    }

    #[test]
    fn fixture_definitions_and_ids_are_stable() {
        let fixtures = content_fixtures();
        let keys = fixtures
            .iter()
            .map(|fixture| fixture.fixture_key)
            .collect::<std::collections::BTreeSet<_>>();
        let ids = fixtures
            .iter()
            .map(|fixture| fixture.id)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(keys.len(), fixtures.len());
        assert_eq!(ids.len(), fixtures.len());
        assert_eq!(
            checksum(&content_definition(&fixtures[0])),
            checksum(&content_definition(&fixtures[0]))
        );
    }
}
