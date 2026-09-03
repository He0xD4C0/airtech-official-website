use super::*;

impl AppState {
    pub async fn integer_setting(
        &self,
        key: &str,
        default: i64,
        minimum: i64,
        maximum: i64,
    ) -> Result<i64, ApiError> {
        let Some(pool) = &self.pool else {
            let settings = &self.data.read().await.settings;
            let value = match key {
                "rfqRetentionDays" => settings.rfq_retention_days,
                "retentionDeletionGraceDays" => settings.retention_deletion_grace_days,
                "temporaryOverrideDefaultDays" => settings.temporary_override_default_days,
                _ => default,
            };
            return value
                .clamp(minimum, maximum)
                .eq(&value)
                .then_some(value)
                .ok_or_else(|| {
                    ApiError::service_unavailable(format!(
                        "The `{key}` setting must be an integer between {minimum} and {maximum}."
                    ))
                });
        };
        let value = sqlx::query_scalar::<_, Value>("SELECT value FROM app_settings WHERE key=$1")
            .bind(key)
            .fetch_optional(pool)
            .await?
            .unwrap_or(Value::from(default));
        value
            .as_i64()
            .filter(|value| (minimum..=maximum).contains(value))
            .ok_or_else(|| {
                ApiError::service_unavailable(format!(
                    "The `{key}` setting must be an integer between {minimum} and {maximum}."
                ))
            })
    }

    pub async fn platform_settings(&self) -> Result<PlatformSettings, ApiError> {
        let Some(pool) = &self.pool else {
            return Ok(self.data.read().await.settings.clone());
        };
        let row = sqlx::query(
            r#"SELECT state.revision,
                      (SELECT value FROM app_settings WHERE key='rfqRetentionDays') AS rfq_retention_days,
                      (SELECT value FROM app_settings WHERE key='retentionDeletionGraceDays') AS retention_deletion_grace_days,
                      (SELECT value FROM app_settings WHERE key='temporaryOverrideDefaultDays') AS temporary_override_default_days,
                      (SELECT value FROM app_settings WHERE key='publicLocale') AS public_locale
               FROM platform_settings_state state
               WHERE state.singleton=true"#,
        )
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| {
            ApiError::service_unavailable("The platform settings revision row is missing.")
        })?;
        decode_platform_settings_row(&row)
    }

    pub async fn update_platform_settings(
        &self,
        expected_revision: i64,
        update: &UpdatePlatformSettings,
        actor: &str,
        request_id: Uuid,
    ) -> Result<PlatformSettings, ApiError> {
        if let Some(pool) = &self.pool {
            let mut transaction = pool.begin().await?;
            let row = sqlx::query(
                r#"SELECT state.revision,
                          (SELECT value FROM app_settings WHERE key='rfqRetentionDays') AS rfq_retention_days,
                          (SELECT value FROM app_settings WHERE key='retentionDeletionGraceDays') AS retention_deletion_grace_days,
                          (SELECT value FROM app_settings WHERE key='temporaryOverrideDefaultDays') AS temporary_override_default_days,
                          (SELECT value FROM app_settings WHERE key='publicLocale') AS public_locale
                   FROM platform_settings_state state
                   WHERE state.singleton=true
                   FOR UPDATE OF state"#,
            )
            .fetch_optional(&mut *transaction)
            .await?
            .ok_or_else(|| {
                ApiError::service_unavailable("The platform settings revision row is missing.")
            })?;
            let before = decode_platform_settings_row(&row)?;
            if before.revision != expected_revision {
                return Err(ApiError::conflict(
                    "The platform settings changed; reload before saving.",
                ));
            }
            let after = apply_platform_settings_update(&before, update)?;
            let now = Utc::now();

            for (key, value) in [
                ("rfqRetentionDays", Value::from(after.rfq_retention_days)),
                (
                    "retentionDeletionGraceDays",
                    Value::from(after.retention_deletion_grace_days),
                ),
                (
                    "temporaryOverrideDefaultDays",
                    Value::from(after.temporary_override_default_days),
                ),
            ] {
                sqlx::query(
                    r#"UPDATE app_settings
                       SET value=$1, updated_at=$2, updated_by=$3
                       WHERE key=$4"#,
                )
                .bind(value)
                .bind(now)
                .bind(actor)
                .bind(key)
                .execute(&mut *transaction)
                .await?;
            }
            let updated = sqlx::query(
                r#"UPDATE platform_settings_state
                   SET revision=$1, updated_at=$2, updated_by=$3
                   WHERE singleton=true AND revision=$4"#,
            )
            .bind(after.revision)
            .bind(now)
            .bind(actor)
            .bind(expected_revision)
            .execute(&mut *transaction)
            .await?;
            if updated.rows_affected() != 1 {
                return Err(ApiError::conflict(
                    "The platform settings changed; reload before saving.",
                ));
            }

            let event = settings_audit_event(&before, &after, update, actor, request_id, now);
            insert_audit_in_transaction(&mut transaction, &event).await?;
            transaction.commit().await?;
            return Ok(after);
        }

        let mut data = self.data.write().await;
        let before = data.settings.clone();
        if before.revision != expected_revision {
            return Err(ApiError::conflict(
                "The platform settings changed; reload before saving.",
            ));
        }
        let after = apply_platform_settings_update(&before, update)?;
        let event = settings_audit_event(&before, &after, update, actor, request_id, Utc::now());
        data.settings = after.clone();
        data.audit_events.push(event);
        Ok(after)
    }
}

fn decode_platform_settings_row(row: &PgRow) -> Result<PlatformSettings, ApiError> {
    fn json_value(row: &PgRow, column: &str) -> Result<Value, ApiError> {
        row.try_get::<Option<Value>, _>(column)?.ok_or_else(|| {
            ApiError::service_unavailable(format!("The `{column}` platform setting is missing."))
        })
    }

    fn integer(value: Value, key: &str, minimum: i64, maximum: i64) -> Result<i64, ApiError> {
        value
            .as_i64()
            .filter(|value| (minimum..=maximum).contains(value))
            .ok_or_else(|| {
                ApiError::service_unavailable(format!(
                    "The `{key}` setting must be an integer between {minimum} and {maximum}."
                ))
            })
    }

    let public_locale = json_value(row, "public_locale")?
        .as_str()
        .filter(|value| *value == "en")
        .map(str::to_owned)
        .ok_or_else(|| {
            ApiError::service_unavailable("The `publicLocale` setting must remain `en`.")
        })?;
    Ok(PlatformSettings {
        rfq_retention_days: integer(
            json_value(row, "rfq_retention_days")?,
            "rfqRetentionDays",
            30,
            3_650,
        )?,
        retention_deletion_grace_days: integer(
            json_value(row, "retention_deletion_grace_days")?,
            "retentionDeletionGraceDays",
            1,
            365,
        )?,
        temporary_override_default_days: integer(
            json_value(row, "temporary_override_default_days")?,
            "temporaryOverrideDefaultDays",
            1,
            365,
        )?,
        public_locale,
        revision: row.try_get("revision")?,
    })
}

fn apply_platform_settings_update(
    before: &PlatformSettings,
    update: &UpdatePlatformSettings,
) -> Result<PlatformSettings, ApiError> {
    let mut errors = BTreeMap::new();
    let reason = update.reason.trim();
    if reason.chars().count() < 12 {
        errors.insert(
            "reason".into(),
            vec!["Reason must contain at least 12 characters.".into()],
        );
    }
    for (field, value, minimum, maximum) in [
        ("rfqRetentionDays", update.rfq_retention_days, 30, 3_650),
        (
            "retentionDeletionGraceDays",
            update.retention_deletion_grace_days,
            1,
            365,
        ),
        (
            "temporaryOverrideDefaultDays",
            update.temporary_override_default_days,
            1,
            365,
        ),
    ] {
        if value.is_some_and(|value| !(minimum..=maximum).contains(&value)) {
            errors.insert(
                field.into(),
                vec![format!("Value must be between {minimum} and {maximum}.")],
            );
        }
    }
    if update.rfq_retention_days.is_none()
        && update.retention_deletion_grace_days.is_none()
        && update.temporary_override_default_days.is_none()
    {
        errors.insert(
            "settings".into(),
            vec!["At least one mutable setting is required.".into()],
        );
    }
    if !errors.is_empty() {
        return Err(ApiError::validation(errors));
    }

    let mut after = before.clone();
    if let Some(value) = update.rfq_retention_days {
        after.rfq_retention_days = value;
    }
    if let Some(value) = update.retention_deletion_grace_days {
        after.retention_deletion_grace_days = value;
    }
    if let Some(value) = update.temporary_override_default_days {
        after.temporary_override_default_days = value;
    }
    if after.rfq_retention_days == before.rfq_retention_days
        && after.retention_deletion_grace_days == before.retention_deletion_grace_days
        && after.temporary_override_default_days == before.temporary_override_default_days
    {
        let mut errors = BTreeMap::new();
        errors.insert(
            "settings".into(),
            vec!["At least one setting must change.".into()],
        );
        return Err(ApiError::validation(errors));
    }
    after.revision = before
        .revision
        .checked_add(1)
        .ok_or_else(|| ApiError::service_unavailable("Settings revision is exhausted."))?;
    Ok(after)
}

fn settings_audit_event(
    before: &PlatformSettings,
    after: &PlatformSettings,
    update: &UpdatePlatformSettings,
    actor: &str,
    request_id: Uuid,
    occurred_at: DateTime<Utc>,
) -> AuditEvent {
    AuditEvent {
        id: Uuid::new_v4(),
        actor: actor.into(),
        action: "settings.update".into(),
        entity_type: "platformSettings".into(),
        entity_id: None,
        before: serde_json::to_value(before).ok(),
        after: serde_json::to_value(after).ok(),
        reason: Some(update.reason.trim().into()),
        request_id,
        occurred_at,
    }
}

async fn insert_audit_in_transaction(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    event: &AuditEvent,
) -> Result<(), ApiError> {
    sqlx::query(
        r#"INSERT INTO audit_log
           (id, actor, action, entity_type, entity_id, before_value, after_value,
            reason, request_id, occurred_at)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)"#,
    )
    .bind(event.id)
    .bind(&event.actor)
    .bind(&event.action)
    .bind(&event.entity_type)
    .bind(event.entity_id)
    .bind(&event.before)
    .bind(&event.after)
    .bind(&event.reason)
    .bind(event.request_id)
    .bind(event.occurred_at)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}
