//! Development-only, transactional publication of the local public-site shell.

use std::collections::{BTreeMap, BTreeSet};

use chrono::Utc;
use serde::Serialize;
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    error::ApiError,
    models::{AuditEvent, ContentDraftV2},
    services::{
        audit_log,
        cms_content::publish_current_route,
        cms_publication_dependencies as dependencies,
        cms_templates::{canonical_path, validate_content_draft, CmsTemplateValidationPhase},
    },
};

const SEED_LOCK: i64 = 672_183_922;
const FIXTURES: &str = include_str!("../../fixtures/development-public-site.json");
const ACTOR: &str = "development-public-seed@airtek.invalid";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevelopmentPublicSiteReport {
    pub status: &'static str,
    pub published_entries: usize,
}

pub async fn ensure(
    pool: &sqlx::PgPool,
    admin_email: &str,
) -> Result<DevelopmentPublicSiteReport, ApiError> {
    let documents = fixture_documents()?;
    validate_fixture_set(&documents)?;
    let mut transaction = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(SEED_LOCK)
        .execute(&mut *transaction)
        .await?;

    match inspect_state(&mut transaction, &documents).await? {
        SeedState::Complete => {
            transaction.commit().await?;
            return Ok(report("skippedComplete", documents.len()));
        }
        SeedState::Partial(detail) => return Err(ApiError::conflict(detail)),
        SeedState::Empty => {}
    }

    let actor_user_id = development_actor(&mut transaction, admin_email).await?;
    for document in &documents {
        publish_fixture(&mut transaction, actor_user_id, document).await?;
    }
    transaction.commit().await?;
    Ok(report("created", documents.len()))
}

fn fixture_documents() -> Result<Vec<ContentDraftV2>, ApiError> {
    serde_json::from_str(FIXTURES).map_err(|error| {
        ApiError::internal(format!("Development public fixture is invalid: {error}"))
    })
}

fn validate_fixture_set(documents: &[ContentDraftV2]) -> Result<(), ApiError> {
    if documents.is_empty() {
        return Err(ApiError::internal(
            "Development public fixture set is empty.",
        ));
    }
    let mut identities = BTreeSet::new();
    for document in documents {
        if document.locale != "en" || !document.is_placeholder || document.seo.indexable {
            return Err(ApiError::internal(
                "Development public fixtures must be English placeholders with indexing disabled.",
            ));
        }
        let issues = validate_content_draft(document, CmsTemplateValidationPhase::Publish);
        if !issues.is_empty() {
            return Err(ApiError::internal(format!(
                "Development fixture {:?}/{:?} failed CMS validation: {:?}",
                document.template_key, document.slug, issues
            )));
        }
        if !identities.insert(identity(document)) {
            return Err(ApiError::internal(
                "Development public fixture identities must be unique.",
            ));
        }
    }
    Ok(())
}

enum SeedState {
    Empty,
    Complete,
    Partial(String),
}

async fn inspect_state(
    transaction: &mut Transaction<'_, Postgres>,
    documents: &[ContentDraftV2],
) -> Result<SeedState, ApiError> {
    let entries = sqlx::query(
        r#"SELECT entry.kind,entry.slug,entry.locale,entry.template_key,
                  entry.data_origin,entry.is_placeholder,
                  published.document
           FROM content_entries entry
           LEFT JOIN cms_published_content published ON published.content_id=entry.id"#,
    )
    .fetch_all(&mut **transaction)
    .await?;
    let draft_count = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM cms_drafts")
        .fetch_one(&mut **transaction)
        .await?;
    let routes = sqlx::query_scalar::<_, String>(
        "SELECT canonical_path FROM public_routes WHERE entity_type='content' ORDER BY canonical_path",
    )
    .fetch_all(&mut **transaction)
    .await?;
    if entries.is_empty() && draft_count == 0 && routes.is_empty() {
        return Ok(SeedState::Empty);
    }

    let expected_documents = documents
        .iter()
        .map(|document| {
            serde_json::to_value(document)
                .map(|value| (identity(document), value))
                .map_err(|_| ApiError::internal("CMS serialization failed."))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let expected_identities = expected_documents.keys().cloned().collect::<BTreeSet<_>>();
    let actual_identities = entries
        .iter()
        .map(|row| {
            format!(
                "{}|{}|{}|{}",
                row.get::<String, _>("kind"),
                row.get::<String, _>("template_key"),
                row.get::<String, _>("locale"),
                row.get::<String, _>("slug"),
            )
        })
        .collect::<BTreeSet<_>>();
    let expected_routes = route_paths(documents)?;
    let actual_routes = routes.into_iter().collect::<BTreeSet<_>>();
    let safe_rows = entries.iter().all(|row| {
        let row_identity = format!(
            "{}|{}|{}|{}",
            row.get::<String, _>("kind"),
            row.get::<String, _>("template_key"),
            row.get::<String, _>("locale"),
            row.get::<String, _>("slug"),
        );
        let published_document = row.get::<Option<serde_json::Value>, _>("document");
        row.get::<String, _>("data_origin") == "developmentFixture"
            && row.get::<bool, _>("is_placeholder")
            && published_document.as_ref() == expected_documents.get(&row_identity)
    });
    if draft_count == 0
        && safe_rows
        && actual_identities == expected_identities
        && actual_routes == expected_routes
    {
        return Ok(SeedState::Complete);
    }
    let missing_entries = expected_identities
        .difference(&actual_identities)
        .cloned()
        .collect::<Vec<_>>();
    let missing_routes = expected_routes
        .difference(&actual_routes)
        .cloned()
        .collect::<Vec<_>>();
    let unexpected_entries = actual_identities
        .difference(&expected_identities)
        .cloned()
        .collect::<Vec<_>>();
    let unexpected_routes = actual_routes
        .difference(&expected_routes)
        .cloned()
        .collect::<Vec<_>>();
    Ok(SeedState::Partial(format!(
        "Development public seed refused partial CMS state (missing entries: {missing_entries:?}; missing routes: {missing_routes:?}; unexpected entries: {unexpected_entries:?}; unexpected routes: {unexpected_routes:?}; drafts: {draft_count}; exact published fixtures: {safe_rows}). Resolve or reset the local development database explicitly."
    )))
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

async fn publish_fixture(
    transaction: &mut Transaction<'_, Postgres>,
    actor_user_id: Uuid,
    document: &ContentDraftV2,
) -> Result<(), ApiError> {
    let content_id = Uuid::new_v4();
    let now = Utc::now();
    sqlx::query(
        r#"INSERT INTO content_entries
           (id,kind,slug,locale,is_placeholder,data_origin,template_key,created_at)
           VALUES ($1,$2,$3,$4,true,'developmentFixture',$5,$6)"#,
    )
    .bind(content_id)
    .bind(enum_label(document.kind))
    .bind(document.slug.as_deref().unwrap_or(""))
    .bind(&document.locale)
    .bind(enum_label(document.template_key))
    .bind(now)
    .execute(&mut **transaction)
    .await?;
    sqlx::query(
        r#"INSERT INTO cms_published_content
           (content_id,document,publication_version,published_by,published_at,updated_at)
           VALUES ($1,$2,1,$3,$4,$4)"#,
    )
    .bind(content_id)
    .bind(
        serde_json::to_value(document)
            .map_err(|_| ApiError::internal("CMS serialization failed."))?,
    )
    .bind(actor_user_id)
    .bind(now)
    .execute(&mut **transaction)
    .await?;

    let extracted = dependencies::extract_draft(document);
    dependencies::lock_targets(transaction, &extracted.lock_targets).await?;
    let plan = dependencies::validate_extracted(transaction, extracted).await?;
    if !plan.is_complete() {
        return Err(ApiError::internal(format!(
            "Development fixture dependency validation failed: {:?}",
            plan.blocking_issues
        )));
    }
    dependencies::replace_current(transaction, content_id, 1, ACTOR, &plan).await?;
    publish_current_route(transaction, content_id, document).await?;
    sqlx::query(
        r#"INSERT INTO outbox_events(id,topic,aggregate_type,aggregate_id,payload)
           VALUES ($1,'public.content.published','content',$2,$3)"#,
    )
    .bind(Uuid::new_v4())
    .bind(content_id)
    .bind(serde_json::json!({"entityId": content_id, "revision": 1, "locale": "en"}))
    .execute(&mut **transaction)
    .await?;
    audit_log::insert_in_transaction(
        transaction,
        &AuditEvent {
            id: Uuid::new_v4(),
            actor: ACTOR.into(),
            action: "content.development_fixture_seeded".into(),
            entity_type: "content".into(),
            entity_id: Some(content_id),
            before: None,
            after: Some(serde_json::json!({"templateKey": enum_label(document.template_key), "slug": document.slug})),
            reason: Some("Created by the explicitly enabled local development public-site seed.".into()),
            current_version: Some(1),
            request_id: Uuid::new_v4(),
            occurred_at: now,
        },
    )
    .await
}

fn route_paths(documents: &[ContentDraftV2]) -> Result<BTreeSet<String>, ApiError> {
    documents
        .iter()
        .filter_map(|document| {
            canonical_path(
                &document.locale,
                document.template_key,
                document.slug.as_deref(),
            )
        })
        .map(Ok)
        .collect()
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

fn report(status: &'static str, published_entries: usize) -> DevelopmentPublicSiteReport {
    DevelopmentPublicSiteReport {
        status,
        published_entries,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        models::{AssetVersionReference, ContentTypeFields},
        services::cms_publication_dependencies::{
            ExtractedDependencyTarget, PublicationDependencyKind,
        },
    };

    #[test]
    fn fixture_set_is_publishable_and_covers_the_core_routes() {
        let documents = fixture_documents().unwrap();
        validate_fixture_set(&documents).unwrap();
        let routes = route_paths(&documents).unwrap();
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
        assert_eq!(routes.len(), 15);
        assert_eq!(documents.len(), 18);
    }

    #[test]
    fn site_icon_is_an_exact_media_publication_dependency() {
        let mut document = fixture_documents()
            .unwrap()
            .into_iter()
            .find(|document| {
                matches!(
                    &document.type_fields,
                    ContentTypeFields::GeneralInformation(_)
                )
            })
            .unwrap();
        let asset_id = Uuid::new_v4();
        let ContentTypeFields::GeneralInformation(fields) = &mut document.type_fields else {
            unreachable!();
        };
        fields.site_icon = Some(AssetVersionReference { asset_id });
        let extracted = dependencies::extract_draft(&document);
        assert!(extracted.references.iter().any(|reference| {
            reference.reference_path == "/typeFields/siteIcon/assetId"
                && reference.kind == PublicationDependencyKind::MediaInline
                && reference.target == ExtractedDependencyTarget::Media(asset_id)
        }));
    }
}
