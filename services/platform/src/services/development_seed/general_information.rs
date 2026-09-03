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
