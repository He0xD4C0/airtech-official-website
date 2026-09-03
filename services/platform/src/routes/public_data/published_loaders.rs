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
