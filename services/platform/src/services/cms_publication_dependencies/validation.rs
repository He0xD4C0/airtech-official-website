use serde_json::Value;
use sqlx::{postgres::PgConnection, Row};
use uuid::Uuid;

use crate::models::CMS_V2_SCHEMA_VERSION;

use super::{
    sort_issues, DependencyBlockingIssue, DependencyGate, DependencyIssueCode, ExtractedDependency,
    ExtractedDependencyTarget, ExtractedPublicationDependencies, PublicationDependencyRow,
    PublicationDependencyTarget, ValidatedPublicationDependencies,
};

pub(super) async fn validate(
    connection: &mut PgConnection,
    extracted: ExtractedPublicationDependencies,
) -> Result<ValidatedPublicationDependencies, sqlx::Error> {
    let mut dependencies = Vec::new();
    let mut issues = extracted.blocking_issues;
    for dependency in extracted.references {
        let validated = match dependency.target.clone() {
            ExtractedDependencyTarget::Content(content_id) => {
                validate_content(
                    connection,
                    &extracted.source_locale,
                    dependency,
                    content_id,
                    &mut issues,
                )
                .await?
            }
            ExtractedDependencyTarget::Product(product_id) => {
                validate_product(connection, dependency, product_id, &mut issues).await?
            }
            ExtractedDependencyTarget::Media(asset_id) => {
                validate_media(connection, dependency, asset_id, &mut issues).await?
            }
        };
        if let Some(dependency) = validated {
            dependencies.push(dependency);
        }
    }
    dependencies.sort_by(|left, right| {
        (&left.reference_path, left.kind).cmp(&(&right.reference_path, right.kind))
    });
    sort_issues(&mut issues);
    Ok(ValidatedPublicationDependencies {
        dependencies,
        blocking_issues: issues,
        lock_targets: extracted.lock_targets,
    })
}

async fn validate_content(
    connection: &mut PgConnection,
    source_locale: &Option<String>,
    dependency: ExtractedDependency,
    content_id: Uuid,
    issues: &mut Vec<DependencyBlockingIssue>,
) -> Result<Option<PublicationDependencyRow>, sqlx::Error> {
    let route_locale = source_locale.as_deref().unwrap_or("");
    let row = sqlx::query(
        r#"SELECT entry.locale,published.publication_version,published.document,
                  EXISTS (
                    SELECT 1 FROM public_routes route
                    WHERE route.entity_type='content' AND route.entity_id=entry.id
                      AND route.locale=$2 AND length(trim(route.canonical_path)) > 0
                  ) AS has_public_route
           FROM content_entries entry
           LEFT JOIN cms_published_content published ON published.content_id=entry.id
           WHERE entry.id=$1"#,
    )
    .bind(content_id)
    .bind(route_locale)
    .fetch_optional(&mut *connection)
    .await?;
    let Some(row) = row else {
        push_issue(
            issues,
            DependencyIssueCode::TargetMissing,
            &dependency.reference_path,
            DependencyGate::TargetIdentity,
            content_id,
            "Referenced content does not exist.",
        );
        return Ok(None);
    };
    let published_revision: Option<i64> = row.try_get("publication_version")?;
    let Some(published_revision) = published_revision else {
        push_issue(
            issues,
            DependencyIssueCode::ContentNotPublished,
            &dependency.reference_path,
            DependencyGate::ContentPublished,
            content_id,
            "Referenced content has no CMS V2 published revision.",
        );
        return Ok(None);
    };
    let document: Option<Value> = row.try_get("document")?;
    if document
        .as_ref()
        .and_then(|document| document.get("schemaVersion"))
        .and_then(Value::as_u64)
        != Some(u64::from(CMS_V2_SCHEMA_VERSION))
    {
        push_issue(
            issues,
            DependencyIssueCode::ContentSchemaMismatch,
            &dependency.reference_path,
            DependencyGate::ContentSchemaVersion,
            content_id,
            "Referenced publication is not a CMS schemaVersion 2 document.",
        );
    }
    if let Some(source_locale) = source_locale {
        let entry_locale: String = row.try_get("locale")?;
        let document_locale = document
            .as_ref()
            .and_then(|document| document.get("locale"))
            .and_then(Value::as_str);
        if entry_locale != *source_locale || document_locale != Some(source_locale.as_str()) {
            push_issue(
                issues,
                DependencyIssueCode::ContentLocaleMismatch,
                &dependency.reference_path,
                DependencyGate::ContentLocale,
                content_id,
                "Referenced content must use the source document locale.",
            );
        }
        if !row.try_get::<bool, _>("has_public_route")? {
            push_issue(
                issues,
                DependencyIssueCode::ContentRouteMissing,
                &dependency.reference_path,
                DependencyGate::ContentPublicRoute,
                content_id,
                "Referenced content has no public route for the source locale.",
            );
        }
    }
    let _ = published_revision;
    Ok(Some(PublicationDependencyRow {
        reference_path: dependency.reference_path,
        kind: dependency.kind,
        target: PublicationDependencyTarget::Content(content_id),
    }))
}

async fn validate_product(
    connection: &mut PgConnection,
    dependency: ExtractedDependency,
    product_id: Uuid,
    issues: &mut Vec<DependencyBlockingIssue>,
) -> Result<Option<PublicationDependencyRow>, sqlx::Error> {
    let row = sqlx::query(
        r#"SELECT product.status,product.published_revision,
                  EXISTS (
                    SELECT 1 FROM product_revisions revision
                    WHERE revision.product_id=product.id
                      AND revision.revision=product.published_revision
                  ) AS revision_exists
           FROM products product WHERE product.id=$1"#,
    )
    .bind(product_id)
    .fetch_optional(&mut *connection)
    .await?;
    let Some(row) = row else {
        push_issue(
            issues,
            DependencyIssueCode::TargetMissing,
            &dependency.reference_path,
            DependencyGate::TargetIdentity,
            product_id,
            "Referenced Product Master entity does not exist.",
        );
        return Ok(None);
    };
    let revision: Option<i64> = row.try_get("published_revision")?;
    let Some(revision) = revision else {
        push_issue(
            issues,
            DependencyIssueCode::ProductNotPublished,
            &dependency.reference_path,
            DependencyGate::ProductPublished,
            product_id,
            "Referenced Product Master entity has no published revision.",
        );
        return Ok(None);
    };
    if row.try_get::<String, _>("status")? != "published" {
        push_issue(
            issues,
            DependencyIssueCode::ProductNotPublished,
            &dependency.reference_path,
            DependencyGate::ProductPublished,
            product_id,
            "Referenced Product Master entity is not published.",
        );
    }
    if !row.try_get::<bool, _>("revision_exists")? {
        push_issue(
            issues,
            DependencyIssueCode::ProductRevisionMissing,
            &dependency.reference_path,
            DependencyGate::ProductRevision,
            product_id,
            "The exact Product Master published revision does not exist.",
        );
        return Ok(None);
    }
    Ok(Some(PublicationDependencyRow {
        reference_path: dependency.reference_path,
        kind: dependency.kind,
        target: PublicationDependencyTarget::Product {
            product_id,
            product_revision: revision,
        },
    }))
}

async fn validate_media(
    connection: &mut PgConnection,
    dependency: ExtractedDependency,
    asset_id: Uuid,
    issues: &mut Vec<DependencyBlockingIssue>,
) -> Result<Option<PublicationDependencyRow>, sqlx::Error> {
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM media_assets WHERE id=$1 AND deleted_at IS NULL)",
    )
    .bind(asset_id)
    .fetch_one(&mut *connection)
    .await?;
    if !exists {
        push_issue(
            issues,
            DependencyIssueCode::MediaMissing,
            &dependency.reference_path,
            DependencyGate::MediaExists,
            asset_id,
            "Referenced media asset does not exist or was deleted.",
        );
        return Ok(None);
    }
    Ok(Some(PublicationDependencyRow {
        reference_path: dependency.reference_path,
        kind: dependency.kind,
        target: PublicationDependencyTarget::Media(asset_id),
    }))
}

fn push_issue(
    issues: &mut Vec<DependencyBlockingIssue>,
    code: DependencyIssueCode,
    path: &str,
    gate: DependencyGate,
    target_id: Uuid,
    detail: &str,
) {
    issues.push(DependencyBlockingIssue::new(
        code,
        path,
        gate,
        Some(target_id),
        detail,
    ));
}
