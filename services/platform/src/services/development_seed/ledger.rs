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
