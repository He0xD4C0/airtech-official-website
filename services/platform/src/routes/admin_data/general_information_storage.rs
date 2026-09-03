async fn load_general_information_by_locale(
    state: &AppState,
    locale: &str,
) -> Result<Option<GeneralInformation>, ApiError> {
    if let Some(pool) = &state.pool {
        return sqlx::query(
            r#"SELECT id,locale,payload,status,current_revision,published_revision,
                      is_placeholder,updated_at FROM general_information
               WHERE scope='site' AND locale=$1"#,
        )
        .bind(locale)
        .fetch_optional(pool)
        .await?
        .map(decode_general_information)
        .transpose();
    }
    Ok(state
        .data
        .read()
        .await
        .general_information
        .values()
        .find(|entry| entry.locale == locale)
        .cloned())
}

async fn load_general_information(
    state: &AppState,
    id: Uuid,
) -> Result<GeneralInformation, ApiError> {
    if let Some(pool) = &state.pool {
        return sqlx::query(
            r#"SELECT id,locale,payload,status,current_revision,published_revision,
                      is_placeholder,updated_at FROM general_information WHERE id=$1"#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await?
        .map(decode_general_information)
        .transpose()?
        .ok_or_else(|| ApiError::not_found("General Information was not found."));
    }
    state
        .data
        .read()
        .await
        .general_information
        .get(&id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("General Information was not found."))
}

async fn load_general_information_revision(
    state: &AppState,
    id: Uuid,
    revision_number: i64,
) -> Result<GeneralInformation, ApiError> {
    if let Some(pool) = &state.pool {
        let row = sqlx::query(
            r#"SELECT information.id,revision.locale,revision.payload,
                      revision.is_placeholder,revision.created_at
               FROM general_information information
               JOIN general_information_revisions revision
                 ON revision.general_information_id=information.id
               WHERE information.id=$1 AND revision.revision=$2"#,
        )
        .bind(id)
        .bind(revision_number)
        .fetch_optional(pool)
        .await?;
        return row
            .map(|row| {
                Ok::<GeneralInformation, ApiError>(GeneralInformation {
                    id: row.try_get("id")?,
                    locale: row.try_get("locale")?,
                    payload: row.try_get("payload")?,
                    status: PublicationStatus::Draft,
                    current_revision: revision_number,
                    published_revision: None,
                    is_placeholder: row.try_get("is_placeholder")?,
                    updated_at: row.try_get("created_at")?,
                })
            })
            .transpose()?
            .ok_or_else(|| ApiError::not_found("General Information revision was not found."));
    }
    state
        .data
        .read()
        .await
        .general_information_revisions
        .get(&id)
        .and_then(|values| values.get(&revision_number))
        .cloned()
        .ok_or_else(|| ApiError::not_found("General Information revision was not found."))
}

fn decode_general_information(row: sqlx::postgres::PgRow) -> Result<GeneralInformation, ApiError> {
    Ok(GeneralInformation {
        id: row.try_get("id")?,
        locale: row.try_get("locale")?,
        payload: row.try_get("payload")?,
        status: decode_publication_status(row.try_get("status")?),
        current_revision: row.try_get("current_revision")?,
        published_revision: row.try_get("published_revision")?,
        is_placeholder: row.try_get("is_placeholder")?,
        updated_at: row.try_get("updated_at")?,
    })
}

async fn persist_general_information_revision(
    state: &AppState,
    entry: &GeneralInformation,
    expected_revision: Option<i64>,
    publish: bool,
    actor: &str,
    audit: &AuditEvent,
    publication_event: Option<&str>,
) -> Result<(), ApiError> {
    let Some(pool) = &state.pool else {
        return Ok(());
    };
    let mut transaction = pool.begin().await?;
    let origin = match expected_revision {
        None => {
            sqlx::query(
                r#"INSERT INTO general_information
                   (id,scope,locale,status,is_placeholder,data_origin,current_revision,
                    published_revision,payload,updated_by,updated_at)
                   VALUES ($1,'site',$2,'draft',$3,'editorial',1,NULL,$4,$5,$6)"#,
            )
            .bind(entry.id)
            .bind(&entry.locale)
            .bind(entry.is_placeholder)
            .bind(&entry.payload)
            .bind(actor)
            .bind(entry.updated_at)
            .execute(&mut *transaction)
            .await?;
            "editorial".to_owned()
        }
        Some(expected) => {
            let result = sqlx::query(
                r#"UPDATE general_information SET locale=$2,status=$3,is_placeholder=$4,
                          data_origin=CASE
                            WHEN data_origin='developmentFixture' AND NOT $4 THEN 'editorial'
                            ELSE data_origin
                          END,
                          current_revision=$5,published_revision=COALESCE($6,published_revision),
                          scheduled_for=NULL,payload=$7,updated_by=$8,updated_at=$9
                   WHERE id=$1 AND current_revision=$10
                   RETURNING data_origin"#,
            )
            .bind(entry.id)
            .bind(&entry.locale)
            .bind(if publish { "published" } else { "draft" })
            .bind(entry.is_placeholder)
            .bind(entry.current_revision)
            .bind(publish.then_some(entry.current_revision))
            .bind(&entry.payload)
            .bind(actor)
            .bind(entry.updated_at)
            .bind(expected)
            .fetch_optional(&mut *transaction)
            .await?;
            let Some(result) = result else {
                transaction.rollback().await?;
                return Err(ApiError::conflict(
                    "General Information changed; reload before saving.",
                ));
            };
            result.try_get::<String, _>("data_origin")?
        }
    };
    let immutable_revision_matches = sqlx::query_scalar::<_, bool>(
        r#"WITH inserted AS (
               INSERT INTO general_information_revisions
                   (general_information_id,revision,payload,locale,is_placeholder,data_origin,
                    created_by,created_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
               ON CONFLICT (general_information_id,revision) DO NOTHING
               RETURNING true
           )
           SELECT EXISTS(SELECT 1 FROM inserted)
              OR EXISTS (
                   SELECT 1 FROM general_information_revisions
                   WHERE general_information_id=$1 AND revision=$2
                     AND payload=$3 AND locale=$4 AND is_placeholder=$5
                     AND data_origin=$6
              )"#,
    )
    .bind(entry.id)
    .bind(entry.current_revision)
    .bind(&entry.payload)
    .bind(&entry.locale)
    .bind(entry.is_placeholder)
    .bind(&origin)
    .bind(actor)
    .bind(entry.updated_at)
    .fetch_one(&mut *transaction)
    .await?;
    if !immutable_revision_matches {
        transaction.rollback().await?;
        return Err(ApiError::service_unavailable(
            "Stored General Information revision conflicts with immutable history.",
        ));
    }
    if publish {
        sqlx::query(
            r#"INSERT INTO outbox_events(id,topic,aggregate_type,aggregate_id,payload)
               VALUES ($1,'public.generalInformation.published','generalInformation',$2,$3)"#,
        )
        .bind(Uuid::new_v4())
        .bind(entry.id)
        .bind(json!({
            "entityId": entry.id,
            "revision": entry.current_revision,
            "locale": entry.locale,
            "event": publication_event.unwrap_or("publish")
        }))
        .execute(&mut *transaction)
        .await?;
    }
    insert_audit_event_in_transaction(&mut transaction, audit).await?;
    transaction.commit().await?;
    Ok(())
}
