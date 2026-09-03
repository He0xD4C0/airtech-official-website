async fn load_news(state: &AppState, id: Uuid) -> Result<NewsEntry, ApiError> {
    if let Some(pool) = &state.pool {
        let row = sqlx::query(
            r#"SELECT entry.payload,news.category,news.author_display_name,
                      news.cover_media_asset_id,news.publication_at,news.featured,
                      news.data_origin
               FROM content_entries entry JOIN news_working news
                 ON news.content_id=entry.id
               WHERE entry.id=$1 AND entry.kind='news'"#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await?;
        return row
            .map(decode_news_row)
            .transpose()?
            .ok_or_else(|| ApiError::not_found("News entry was not found."));
    }
    state
        .data
        .read()
        .await
        .news
        .get(&id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("News entry was not found."))
}

async fn load_news_revision(
    state: &AppState,
    id: Uuid,
    revision: i64,
) -> Result<NewsEntry, ApiError> {
    if let Some(pool) = &state.pool {
        let row = sqlx::query(
            r#"SELECT content_revision.payload,news.category,news.author_display_name,
                      news.cover_media_asset_id,news.publication_at,news.featured,
                      news.data_origin
               FROM content_revisions content_revision
               JOIN content_entries entry ON entry.id=content_revision.content_id
               JOIN news ON news.content_id=content_revision.content_id
                 AND news.revision=content_revision.revision
               WHERE content_revision.content_id=$1 AND content_revision.revision=$2"#,
        )
        .bind(id)
        .bind(revision)
        .fetch_optional(pool)
        .await?;
        return row
            .map(decode_news_row)
            .transpose()?
            .ok_or_else(|| ApiError::not_found("News revision was not found."));
    }
    let data = state.data.read().await;
    let content = data
        .content_revisions
        .get(&id)
        .and_then(|values| values.get(&revision))
        .cloned()
        .ok_or_else(|| ApiError::not_found("News revision was not found."))?;
    let metadata = data
        .news
        .get(&id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("News metadata was not found."))?;
    Ok(NewsEntry {
        content,
        ..metadata
    })
}

fn decode_news_row(row: sqlx::postgres::PgRow) -> Result<NewsEntry, ApiError> {
    Ok(NewsEntry {
        content: decode_json(row.try_get("payload")?, "news content")?,
        category: row.try_get("category")?,
        author_display_name: row.try_get("author_display_name")?,
        cover_media_id: row.try_get("cover_media_asset_id")?,
        published_at: row.try_get("publication_at")?,
        featured: row.try_get("featured")?,
        data_class: match row.try_get::<String, _>("data_origin")?.as_str() {
            "developmentFixture" => DataClass::DevelopmentFixture,
            _ => DataClass::Editorial,
        },
    })
}

async fn persist_news_revision(
    state: &AppState,
    entry: &NewsEntry,
    expected_revision: Option<i64>,
    publish: bool,
    actor: &str,
    audit: &AuditEvent,
) -> Result<(), ApiError> {
    let Some(pool) = &state.pool else {
        return Ok(());
    };
    let payload = serde_json::to_value(&entry.content)
        .map_err(|_| ApiError::internal("News serialization failed."))?;
    let origin = if entry.data_class == DataClass::DevelopmentFixture {
        "developmentFixture"
    } else {
        "editorial"
    };
    let mut transaction = pool.begin().await?;
    match expected_revision {
        None => {
            sqlx::query(
                r#"INSERT INTO content_entries
                   (id,kind,slug,locale,title,status,is_placeholder,current_revision,
                    published_revision,scheduled_for,payload,updated_at,data_origin)
                   VALUES ($1,'news',$2,$3,$4,'draft',$5,1,NULL,NULL,$6,$7,$8)"#,
            )
            .bind(entry.content.id)
            .bind(&entry.content.slug)
            .bind(&entry.content.locale)
            .bind(&entry.content.title)
            .bind(entry.content.is_placeholder)
            .bind(&payload)
            .bind(entry.content.updated_at)
            .bind(origin)
            .execute(&mut *transaction)
            .await?;
        }
        Some(expected) => {
            let status = if publish { "published" } else { "draft" };
            let published_revision = publish.then_some(entry.content.current_revision);
            let result = sqlx::query(
                r#"UPDATE content_entries SET slug=$2,locale=$3,title=$4,status=$5,
                          is_placeholder=$6,current_revision=$7,published_revision=COALESCE($8,published_revision),
                          scheduled_for=NULL,payload=$9,updated_at=$10,data_origin=$11
                   WHERE id=$1 AND current_revision=$12 AND kind='news'"#,
            )
            .bind(entry.content.id)
            .bind(&entry.content.slug)
            .bind(&entry.content.locale)
            .bind(&entry.content.title)
            .bind(status)
            .bind(entry.content.is_placeholder)
            .bind(entry.content.current_revision)
            .bind(published_revision)
            .bind(&payload)
            .bind(entry.content.updated_at)
            .bind(origin)
            .bind(expected)
            .execute(&mut *transaction)
            .await?;
            if result.rows_affected() != 1 {
                transaction.rollback().await?;
                return Err(ApiError::conflict(
                    "The news entry changed; reload before saving.",
                ));
            }
        }
    }
    sqlx::query(
        r#"INSERT INTO news_working
           (content_id,content_kind,category,author_display_name,
            cover_media_asset_id,featured,publication_at,reading_minutes,data_origin,updated_at)
           VALUES ($1,'news',$2,$3,$4,$5,$6,NULL,$7,$8)
           ON CONFLICT (content_id) DO UPDATE SET
             category=EXCLUDED.category,
             author_display_name=EXCLUDED.author_display_name,
             cover_media_asset_id=EXCLUDED.cover_media_asset_id,
             featured=EXCLUDED.featured,
             publication_at=EXCLUDED.publication_at,
             reading_minutes=EXCLUDED.reading_minutes,
             data_origin=EXCLUDED.data_origin,
             updated_at=EXCLUDED.updated_at"#,
    )
    .bind(entry.content.id)
    .bind(&entry.category)
    .bind(entry.author_display_name.as_deref())
    .bind(entry.cover_media_id)
    .bind(entry.featured)
    .bind(entry.published_at)
    .bind(origin)
    .bind(entry.content.updated_at)
    .execute(&mut *transaction)
    .await?;
    let publication_event = if expected_revision == Some(entry.content.current_revision) {
        "publish"
    } else {
        "rollback"
    };
    if publish {
        sqlx::query(
            r#"INSERT INTO content_revisions(content_id,revision,payload,created_by,created_at)
               VALUES ($1,$2,$3,$4,$5)
               ON CONFLICT (content_id,revision) DO NOTHING"#,
        )
        .bind(entry.content.id)
        .bind(entry.content.current_revision)
        .bind(&payload)
        .bind(actor)
        .bind(entry.content.updated_at)
        .execute(&mut *transaction)
        .await?;
    }
    if publish {
        sqlx::query(
            r#"INSERT INTO news
               (content_id,revision,content_kind,category,author_display_name,
                cover_media_asset_id,featured,publication_at,reading_minutes,data_origin)
               VALUES ($1,$2,'news',$3,$4,$5,$6,$7,NULL,$8)
               ON CONFLICT (content_id,revision) DO NOTHING"#,
        )
        .bind(entry.content.id)
        .bind(entry.content.current_revision)
        .bind(&entry.category)
        .bind(entry.author_display_name.as_deref())
        .bind(entry.cover_media_id)
        .bind(entry.featured)
        .bind(entry.published_at)
        .bind(origin)
        .execute(&mut *transaction)
        .await?;
        let canonical_path = entry
            .content
            .seo
            .canonical_path
            .as_deref()
            .ok_or_else(|| ApiError::bad_request("Published News requires canonicalPath."))?;
        sqlx::query("DELETE FROM public_routes WHERE entity_type='content' AND entity_id=$1")
            .bind(entry.content.id)
            .execute(&mut *transaction)
            .await?;
        let owner = sqlx::query_scalar::<_, Uuid>(
            "SELECT entity_id FROM public_routes WHERE canonical_path=$1",
        )
        .bind(canonical_path)
        .fetch_optional(&mut *transaction)
        .await?;
        if owner.is_some_and(|owner| owner != entry.content.id) {
            transaction.rollback().await?;
            return Err(ApiError::conflict(
                "Another published entity already owns this News canonical path.",
            ));
        }
        sqlx::query(
            r#"INSERT INTO public_routes
               (id,entity_type,entity_id,locale,canonical_path,indexable,updated_at)
               VALUES ($1,'content',$2,$3,$4,$5,$6)
               ON CONFLICT (entity_type,entity_id,locale) DO UPDATE SET
                 canonical_path=EXCLUDED.canonical_path,indexable=EXCLUDED.indexable,
                 updated_at=EXCLUDED.updated_at"#,
        )
        .bind(Uuid::new_v4())
        .bind(entry.content.id)
        .bind(&entry.content.locale)
        .bind(canonical_path)
        .bind(entry.content.seo.indexable && !entry.content.is_placeholder)
        .bind(entry.content.updated_at)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            r#"INSERT INTO outbox_events(id,topic,aggregate_type,aggregate_id,payload)
               VALUES ($1,'public.news.published','content',$2,$3)"#,
        )
        .bind(Uuid::new_v4())
        .bind(entry.content.id)
        .bind(json!({
            "entityId": entry.content.id,
            "revision": entry.content.current_revision,
            "locale": entry.content.locale,
            "event": publication_event
        }))
        .execute(&mut *transaction)
        .await?;
    }
    insert_audit_event_in_transaction(&mut transaction, audit).await?;
    transaction.commit().await?;
    Ok(())
}
