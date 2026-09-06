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
    let template_key = match fixture.kind {
        "navigation" => "navigation",
        "footer" => "footer",
        _ => fixture
            .page_slots
            .get("templateKey")
            .and_then(Value::as_str)
            .expect("routable development fixtures declare a templateKey"),
    };
    let updated_at = fixture_timestamp()?;

    if existing.is_none() {
        sqlx::query(
            r#"INSERT INTO content_entries
               (id, kind, slug, locale, title, status, is_placeholder,
                current_revision, published_revision, scheduled_for, payload,
                updated_at, data_origin, template_key)
               VALUES ($1,$2,$3,'en',$4,'published',true,$5,$5,NULL,$6,$7,
                       'developmentFixture',$8)"#,
        )
        .bind(fixture.id)
        .bind(fixture.kind)
        .bind(fixture.slug)
        .bind(fixture.title)
        .bind(revision)
        .bind(&payload)
        .bind(updated_at)
        .bind(template_key)
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
