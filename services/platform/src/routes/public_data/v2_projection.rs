// Native CMS V2 public projection loaders.
//
// The published document is the source of truth. Relations and content link
// targets are resolved through `public_routes` so the website never receives a
// raw identifier it would have to guess a path for.

use crate::services::cms_content::{resolve_content_links, resolve_relation_cards};

async fn build_v2_projection(
    pool: &sqlx::PgPool,
    id: Uuid,
    revision: i64,
    updated_at: chrono::DateTime<Utc>,
    document: Value,
    locale: &str,
) -> Result<Option<PublicContentProjection>, ApiError> {
    let draft: crate::models::ContentDraftV2 =
        serde_json::from_value(document).map_err(|error| {
            tracing::error!(%error, "published CMS V2 document is invalid");
            ApiError::service_unavailable("Stored published content is invalid.")
        })?;
    if draft.schema_version != crate::models::CMS_V2_SCHEMA_VERSION {
        tracing::error!(
            schema_version = draft.schema_version,
            "published CMS document has an unsupported schema version"
        );
        return Err(ApiError::service_unavailable(
            "Stored published content has an unsupported schema version.",
        ));
    }
    if draft.locale != locale {
        return Ok(None);
    }
    let mut connection = pool.acquire().await?;
    let resolved_relations = resolve_relation_cards(&mut connection, locale, &draft).await?;
    let resolved_links = resolve_content_links(&mut connection, locale, &draft).await?;
    Ok(Some(PublicContentProjection {
        schema_version: crate::models::CMS_V2_SCHEMA_VERSION,
        id,
        kind: draft.kind,
        locale: draft.locale.clone(),
        template_key: draft.template_key,
        title: draft.title.clone(),
        slug: draft.slug.clone(),
        summary: draft.summary.clone(),
        is_placeholder: draft.is_placeholder,
        type_fields: draft.type_fields.clone(),
        body: draft.body.clone(),
        composition: draft.composition.clone(),
        seo: draft.seo.clone(),
        published_revision: revision,
        updated_at,
        resolved_relations,
        resolved_links,
    }))
}

fn public_projection_pool(state: &AppState) -> Result<&sqlx::PgPool, ApiError> {
    state.pool.as_ref().ok_or_else(|| {
        ApiError::service_unavailable("The public CMS projection store is unavailable.")
    })
}

pub(crate) async fn load_v2_content_revision_preview(
    state: &AppState,
    content_id: Uuid,
    revision: i64,
) -> Result<Option<PublicContentProjection>, ApiError> {
    let pool = public_projection_pool(state)?;
    let row = sqlx::query(
        r#"SELECT entry.id,entry.locale,revision.revision,revision.document,
                  revision.created_at
           FROM content_entries entry
           JOIN content_revisions revision ON revision.content_id=entry.id
           WHERE entry.id=$1 AND revision.revision=$2
             AND revision.document IS NOT NULL
             AND revision.document->>'schemaVersion'='2'
             AND revision.document->>'kind'=entry.kind
             AND revision.document->>'locale'=entry.locale
             AND revision.document->>'templateKey'=entry.template_key
           LIMIT 1"#,
    )
    .bind(content_id)
    .bind(revision)
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let locale: String = row.try_get("locale")?;
    build_v2_projection(
        pool,
        row.try_get("id")?,
        row.try_get("revision")?,
        row.try_get("created_at")?,
        row.try_get("document")?,
        &locale,
    )
    .await
}

pub(crate) async fn load_v2_content_by_kind_slug(
    state: &AppState,
    kind: CmsContentKind,
    slug: &str,
    locale: &str,
) -> Result<Option<PublicContentProjection>, ApiError> {
    let pool = public_projection_pool(state)?;
    let row = sqlx::query(
        r#"SELECT entry.id,entry.cms_published_revision AS revision,revision.document,
                  revision.created_at,route.canonical_path
           FROM public_routes route
           JOIN content_entries entry
             ON route.entity_type='content' AND entry.id=route.entity_id
           JOIN content_revisions revision
             ON revision.content_id=entry.id
            AND revision.revision=entry.cms_published_revision
           WHERE entry.kind=$1 AND entry.locale=$3
             AND route.locale=$3
             AND revision.document->>'slug'=$2
             AND revision.document->>'locale'=$3
             AND revision.document->>'kind'=$1
             AND revision.document->>'templateKey'=entry.template_key
             AND entry.cms_published_revision IS NOT NULL
             AND revision.document IS NOT NULL
           LIMIT 1"#,
    )
    .bind(cms_kind_label(kind))
    .bind(slug)
    .bind(locale)
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let projection = build_v2_projection(
        pool,
        row.try_get("id")?,
        row.try_get("revision")?,
        row.try_get("created_at")?,
        row.try_get("document")?,
        locale,
    )
    .await?;
    let route_path: String = row.try_get("canonical_path")?;
    Ok(projection.filter(|projection| projection_has_canonical_path(projection, &route_path)))
}

async fn load_v2_singleton(
    state: &AppState,
    kind: CmsContentKind,
    locale: &str,
) -> Result<Option<PublicContentProjection>, ApiError> {
    let pool = public_projection_pool(state)?;
    let row = sqlx::query(
        r#"SELECT entry.id,entry.cms_published_revision AS revision,revision.document,
                  revision.created_at
           FROM content_entries entry
           JOIN content_revisions revision
             ON revision.content_id=entry.id
            AND revision.revision=entry.cms_published_revision
           WHERE entry.kind=$1 AND entry.locale=$2
             AND entry.cms_published_revision IS NOT NULL
             AND revision.document IS NOT NULL
             AND revision.document->>'kind'=$1
             AND revision.document->>'locale'=$2
             AND revision.document->>'templateKey'=entry.template_key
           ORDER BY revision.created_at DESC,entry.id
           LIMIT 1"#,
    )
    .bind(cms_kind_label(kind))
    .bind(locale)
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    build_v2_projection(
        pool,
        row.try_get("id")?,
        row.try_get("revision")?,
        row.try_get("created_at")?,
        row.try_get("document")?,
        locale,
    )
    .await
}

async fn load_v2_news(
    state: &AppState,
    locale: &str,
    category: Option<&str>,
) -> Result<Vec<NewsEntry>, ApiError> {
    let pool = public_projection_pool(state)?;
    let rows = sqlx::query(
        r#"SELECT entry.id,entry.cms_published_revision AS revision,revision.document,
                  revision.created_at,route.canonical_path
           FROM content_entries entry
           JOIN public_routes route
             ON route.entity_type='content' AND route.entity_id=entry.id AND route.locale=$1
           JOIN content_revisions revision
             ON revision.content_id=entry.id
            AND revision.revision=entry.cms_published_revision
           WHERE entry.kind='news' AND entry.locale=$1
             AND entry.cms_published_revision IS NOT NULL
             AND revision.document IS NOT NULL
             AND revision.document->>'kind'='news'
             AND revision.document->>'locale'=$1
             AND revision.document->>'templateKey'=entry.template_key
             AND ($2::text IS NULL OR revision.document->'typeFields'->>'category'=$2)
           ORDER BY entry.id"#,
    )
    .bind(locale)
    .bind(category)
    .fetch_all(pool)
    .await?;
    let mut entries = Vec::new();
    for row in rows {
        let projection = build_v2_projection(
            pool,
            row.try_get("id")?,
            row.try_get("revision")?,
            row.try_get("created_at")?,
            row.try_get("document")?,
            locale,
        )
        .await?;
        if let Some(projection) = projection {
            let route_path: String = row.try_get("canonical_path")?;
            if !projection_has_canonical_path(&projection, &route_path) {
                continue;
            }
            entries.push(news_entry_from_projection(projection));
        }
    }
    Ok(entries)
}

fn news_entry_from_projection(projection: PublicContentProjection) -> NewsEntry {
    let (category, author, cover, published_at, featured) = match &projection.type_fields {
        crate::models::ContentTypeFields::News(fields) => (
            fields.category.clone(),
            fields.author_display_name.clone(),
            fields.cover.clone(),
            fields.publication_at,
            fields.featured,
        ),
        _ => (None, None, None, None, false),
    };
    NewsEntry {
        category: category
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "News".to_owned()),
        author_display_name: author,
        cover_media_id: cover.as_ref().map(|media| media.asset.asset_id),
        published_at: published_at.or(Some(projection.updated_at)),
        featured,
        data_class: if projection.is_placeholder {
            DataClass::DevelopmentFixture
        } else {
            DataClass::Editorial
        },
        content: projection,
    }
}

fn cms_kind_label(kind: CmsContentKind) -> String {
    serde_json::to_value(kind)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

pub(crate) fn projection_has_canonical_path(
    projection: &PublicContentProjection,
    path: &str,
) -> bool {
    crate::services::cms_templates::canonical_path(
        &projection.locale,
        projection.template_key,
        projection.slug.as_deref(),
    )
    .as_deref()
        == Some(path)
}

struct PublishedRouteProjection {
    page: PublicContentProjection,
    template_key: String,
    indexable: bool,
    revision: i64,
    entity_id: Uuid,
}

async fn load_v2_route(
    state: &AppState,
    path: &str,
    locale: &str,
) -> Result<Option<PublishedRouteProjection>, ApiError> {
    let pool = public_projection_pool(state)?;
    let row = sqlx::query(
        r#"SELECT route.entity_id,route.indexable,
                  entry.cms_published_revision AS revision,revision.document,
                  revision.created_at
           FROM public_routes route
           JOIN content_entries entry
             ON route.entity_type='content' AND entry.id=route.entity_id
           JOIN content_revisions revision
             ON revision.content_id=entry.id
            AND revision.revision=entry.cms_published_revision
           WHERE route.canonical_path=$1 AND route.locale=$2
             AND entry.cms_published_revision IS NOT NULL
             AND revision.document IS NOT NULL
             AND revision.document->>'locale'=$2
             AND revision.document->>'kind'=entry.kind
             AND revision.document->>'templateKey'=entry.template_key
           LIMIT 1"#,
    )
    .bind(path)
    .bind(locale)
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let projection = build_v2_projection(
        pool,
        row.try_get("entity_id")?,
        row.try_get("revision")?,
        row.try_get("created_at")?,
        row.try_get("document")?,
        locale,
    )
    .await?;
    let projection = projection.filter(|projection| projection_has_canonical_path(projection, path));
    projection
        .map(|projection| {
            let template_key = serde_json::to_value(projection.template_key)
                .ok()
                .and_then(|value| value.as_str().map(str::to_owned))
                .ok_or_else(|| ApiError::internal("CMS template serialization failed."))?;
            Ok(PublishedRouteProjection {
                page: projection,
                template_key,
                indexable: row.try_get("indexable")?,
                revision: row.try_get("revision")?,
                entity_id: row.try_get("entity_id")?,
            })
        })
        .transpose()
}
