//! Canonical public projection written when unified CMS content is published.
//!
//! The template registry owns every canonical path, the publish transaction owns
//! the `public_routes` row, and the public read path never guesses a path from
//! the draft shape.

use std::collections::BTreeSet;

use axum::http::StatusCode;
use sqlx::{postgres::PgConnection, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    error::ApiError,
    models::{
        ContentBlock, ContentDraftV2, ContentTypeFields, LinkTargetReference, NavigationItem,
        RelationTargetReference, ResolvedLinkTarget, ResolvedRelationCard,
        ResolvedRelationEntityType,
    },
    services::cms_templates::{canonical_path, template_definition},
};

type Result<T> = std::result::Result<T, ApiError>;

#[path = "publication/media.rs"]
mod media;
use media::validate_published_media;

fn publication_blocked(detail: String) -> ApiError {
    ApiError::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        "Publication blocked",
        detail,
    )
}

pub async fn publish_current_route(
    transaction: &mut Transaction<'_, Postgres>,
    content_id: Uuid,
    draft: &ContentDraftV2,
) -> Result<()> {
    validate_published_references(&mut *transaction, draft).await?;
    let definition = template_definition(draft.template_key)
        .ok_or_else(|| ApiError::internal("The CMS content template is not registered."))?;
    if !definition.routable {
        sqlx::query("DELETE FROM public_routes WHERE entity_type='content' AND entity_id=$1")
            .bind(content_id)
            .execute(&mut **transaction)
            .await?;
        return Ok(());
    }
    let path = canonical_path(&draft.locale, draft.template_key, draft.slug.as_deref())
        .ok_or_else(|| {
            publication_blocked(format!(
                "No public path could be derived for template {:?} and slug {:?}.",
                draft.template_key, draft.slug
            ))
        })?;
    if let Some((entity_type, entity_id)) =
        conflicting_route_owner(&mut *transaction, &path, content_id, &draft.locale).await?
    {
        return Err(route_conflict(&path, &entity_type, entity_id));
    }
    let indexable = draft.seo.indexable && !draft.is_placeholder;
    sqlx::query("SAVEPOINT cms_public_route_upsert")
        .execute(&mut **transaction)
        .await?;
    let write = sqlx::query(
        r#"INSERT INTO public_routes
           (id,entity_type,entity_id,locale,canonical_path,indexable,updated_at)
           VALUES ($1,'content',$2,$3,$4,$5,now())
           ON CONFLICT (entity_type,entity_id,locale) DO UPDATE SET
             canonical_path=EXCLUDED.canonical_path,
             indexable=EXCLUDED.indexable,
             updated_at=EXCLUDED.updated_at"#,
    )
    .bind(Uuid::new_v4())
    .bind(content_id)
    .bind(&draft.locale)
    .bind(&path)
    .bind(indexable)
    .execute(&mut **transaction)
    .await;
    match write {
        Ok(_) => {
            sqlx::query("RELEASE SAVEPOINT cms_public_route_upsert")
                .execute(&mut **transaction)
                .await?;
            Ok(())
        }
        Err(error) => {
            sqlx::query("ROLLBACK TO SAVEPOINT cms_public_route_upsert")
                .execute(&mut **transaction)
                .await?;
            sqlx::query("RELEASE SAVEPOINT cms_public_route_upsert")
                .execute(&mut **transaction)
                .await?;
            if !is_canonical_path_conflict(&error) {
                return Err(error.into());
            }
            let owner =
                conflicting_route_owner(&mut *transaction, &path, content_id, &draft.locale)
                    .await?;
            match owner {
                Some((entity_type, entity_id)) => {
                    Err(route_conflict(&path, &entity_type, entity_id))
                }
                None => Err(ApiError::conflict(format!(
                    "The public path {path} was claimed concurrently by an existing entity."
                ))),
            }
        }
    }
}

async fn conflicting_route_owner(
    connection: &mut PgConnection,
    path: &str,
    content_id: Uuid,
    locale: &str,
) -> Result<Option<(String, Uuid)>> {
    Ok(sqlx::query_as(
        r#"SELECT entity_type,entity_id FROM public_routes
           WHERE canonical_path=$1
             AND NOT (entity_type='content' AND entity_id=$2 AND locale=$3)
           LIMIT 1"#,
    )
    .bind(path)
    .bind(content_id)
    .bind(locale)
    .fetch_optional(connection)
    .await?)
}

fn is_canonical_path_conflict(error: &sqlx::Error) -> bool {
    error.as_database_error().is_some_and(|database| {
        database.code().as_deref() == Some("23505")
            && database.constraint() == Some("public_routes_canonical_path_key")
    })
}

fn route_conflict(path: &str, entity_type: &str, entity_id: Uuid) -> ApiError {
    ApiError::conflict(format!(
        "The public path {path} is already published by {entity_type} {entity_id}."
    ))
}

/// Resolves the draft relations into public cards for the website.
pub async fn resolve_relation_cards(
    connection: &mut PgConnection,
    locale: &str,
    draft: &ContentDraftV2,
) -> Result<Vec<ResolvedRelationCard>> {
    let mut cards = Vec::new();
    for relation in &draft.relations {
        match &relation.target {
            RelationTargetReference::Content { content_id } => {
                if let Some(card) =
                    content_relation_card(connection, locale, relation.id, *content_id).await?
                {
                    cards.push(card);
                }
            }
            RelationTargetReference::Product { product_id } => {
                if let Some(card) =
                    product_relation_card(connection, locale, relation.id, *product_id).await?
                {
                    cards.push(card);
                }
            }
        }
    }
    Ok(cards)
}

async fn content_relation_card(
    connection: &mut PgConnection,
    locale: &str,
    relation_id: Uuid,
    content_id: Uuid,
) -> Result<Option<ResolvedRelationCard>> {
    let row = sqlx::query(
        r#"SELECT route.canonical_path,
                  published.document->>'title' AS title,
                  published.document->>'summary' AS summary,
                  published.document->'typeFields'->>'type' AS fields_type,
                  published.document->'typeFields'->>'category' AS category
           FROM public_routes route
           JOIN content_entries entry ON entry.id=route.entity_id
           JOIN cms_published_content published ON published.content_id=entry.id
           WHERE route.entity_type='content' AND route.entity_id=$1 AND route.locale=$2
             AND published.document->>'locale'=$2
             AND published.document->>'kind'=entry.kind
             AND published.document->>'templateKey'=entry.template_key
           LIMIT 1"#,
    )
    .bind(content_id)
    .bind(locale)
    .fetch_optional(connection)
    .await?;
    let Some(row) = row else { return Ok(None) };
    let title: Option<String> = row.try_get("title")?;
    let Some(title) = title.filter(|value| !value.trim().is_empty()) else {
        return Ok(None);
    };
    let category: Option<String> = row.try_get("category")?;
    Ok(Some(ResolvedRelationCard {
        relation_id,
        entity_type: ResolvedRelationEntityType::Content,
        title,
        summary: non_empty(row.try_get("summary")?),
        href: row.try_get("canonical_path")?,
        eyebrow: non_empty(row.try_get("fields_type")?),
        tags: category
            .filter(|value| !value.trim().is_empty())
            .into_iter()
            .collect(),
    }))
}

async fn product_relation_card(
    connection: &mut PgConnection,
    locale: &str,
    relation_id: Uuid,
    product_id: Uuid,
) -> Result<Option<ResolvedRelationCard>> {
    let row = sqlx::query(
        r#"SELECT route.canonical_path,localization.title,localization.summary,product.family
           FROM public_routes route
           JOIN products product
             ON product.id=route.entity_id AND product.published_revision IS NOT NULL
           JOIN product_localizations localization
             ON localization.product_id=product.id
            AND localization.product_revision=product.published_revision
            AND localization.locale=route.locale
            AND localization.translation_state='verified'
           WHERE route.entity_type='product' AND route.entity_id=$1 AND route.locale=$2
           LIMIT 1"#,
    )
    .bind(product_id)
    .bind(locale)
    .fetch_optional(connection)
    .await?;
    let Some(row) = row else { return Ok(None) };
    Ok(Some(ResolvedRelationCard {
        relation_id,
        entity_type: ResolvedRelationEntityType::Product,
        title: row.try_get("title")?,
        summary: non_empty(row.try_get("summary")?),
        href: row.try_get("canonical_path")?,
        eyebrow: non_empty(Some(row.try_get::<String, _>("family")?)),
        tags: Vec::new(),
    }))
}

/// Resolves every content link target used by the draft into an `href`.
pub async fn resolve_content_links(
    connection: &mut PgConnection,
    locale: &str,
    draft: &ContentDraftV2,
) -> Result<Vec<ResolvedLinkTarget>> {
    let ids = collect_content_link_ids(draft);
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query(
        r#"SELECT route.entity_id,route.canonical_path
           FROM public_routes route
           JOIN content_entries entry
             ON route.entity_type='content' AND entry.id=route.entity_id
           JOIN cms_published_content published ON published.content_id=entry.id
           WHERE route.locale=$1 AND route.entity_id = ANY($2)
             AND published.document->>'locale'=$1
             AND published.document->>'kind'=entry.kind
             AND published.document->>'templateKey'=entry.template_key"#,
    )
    .bind(locale)
    .bind(ids.into_iter().collect::<Vec<_>>())
    .fetch_all(connection)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(ResolvedLinkTarget {
                content_id: row.try_get("entity_id")?,
                href: row.try_get("canonical_path")?,
            })
        })
        .collect()
}

fn collect_content_link_ids(draft: &ContentDraftV2) -> BTreeSet<Uuid> {
    let mut ids = BTreeSet::new();
    let mut push = |target: &LinkTargetReference| {
        if let LinkTargetReference::Content { content_id } = target {
            ids.insert(*content_id);
        }
    };
    for block in &draft.composition.blocks {
        match block {
            ContentBlock::Hero(hero) => hero.actions.iter().for_each(|action| push(&action.target)),
            ContentBlock::Cta(cta) => push(&cta.action.target),
            ContentBlock::ContactBlock(contact) => {
                if let Some(action) = &contact.action {
                    push(&action.target);
                }
            }
            _ => {}
        }
    }
    match &draft.type_fields {
        ContentTypeFields::Navigation(fields) => walk_navigation_items(&fields.items, &mut push),
        ContentTypeFields::Footer(fields) => {
            for column in &fields.columns {
                walk_navigation_items(&column.links, &mut push);
            }
            walk_navigation_items(&fields.legal_links, &mut push);
        }
        ContentTypeFields::GeneralInformation(fields) => {
            if let Some(action) = &fields.navigation_cta {
                push(&action.target);
            }
        }
        _ => {}
    }
    ids
}

fn walk_navigation_items(items: &[NavigationItem], push: &mut impl FnMut(&LinkTargetReference)) {
    for item in items {
        if let Some(target) = &item.target {
            push(target);
        }
        walk_navigation_items(&item.children, push);
    }
}

async fn validate_published_references(
    connection: &mut PgConnection,
    draft: &ContentDraftV2,
) -> Result<()> {
    validate_published_relations(connection, draft).await?;
    validate_published_media(connection, draft).await
}

async fn validate_published_relations(
    connection: &mut PgConnection,
    draft: &ContentDraftV2,
) -> Result<()> {
    let resolved = resolve_relation_cards(connection, &draft.locale, draft).await?;
    let published = resolved
        .iter()
        .map(|card| card.relation_id)
        .collect::<BTreeSet<_>>();
    for block in &draft.composition.blocks {
        let ContentBlock::RelationCollection(collection) = block else {
            continue;
        };
        for relation_id in &collection.relation_ids {
            if published.contains(relation_id) {
                continue;
            }
            let target = draft
                .relations
                .iter()
                .find(|relation| relation.id == *relation_id);
            let detail = match target {
                Some(relation) => format!(
                    "Relation block {} references {} {}, which is not published yet.",
                    collection.id,
                    target_label(&relation.target),
                    target_id(&relation.target)
                ),
                None => format!(
                    "Relation block {} references unknown relation {relation_id}.",
                    collection.id
                ),
            };
            return Err(publication_blocked(detail));
        }
    }
    Ok(())
}

fn target_label(target: &RelationTargetReference) -> &'static str {
    match target {
        RelationTargetReference::Content { .. } => "content",
        RelationTargetReference::Product { .. } => "product",
    }
}

fn target_id(target: &RelationTargetReference) -> Uuid {
    match target {
        RelationTargetReference::Content { content_id } => *content_id,
        RelationTargetReference::Product { product_id } => *product_id,
    }
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.trim().is_empty())
}
