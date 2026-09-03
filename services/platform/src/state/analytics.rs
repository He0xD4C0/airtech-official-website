use super::*;

impl AppState {
    /// Derive the database identifier used for a browser-provided anonymous
    /// session. PostgreSQL never stores the client UUID verbatim. The
    /// in-memory adapter intentionally keeps the original ID for isolated unit
    /// tests that do not have deployment secrets.
    pub(crate) fn analytics_storage_session_id(&self, external_id: Uuid) -> Result<Uuid, ApiError> {
        if self.pool.is_none() {
            return Ok(external_id);
        }
        let secret = self
            .config
            .analytics_token_hmac_key
            .as_ref()
            .ok_or_else(|| {
                ApiError::service_unavailable(
                    "Analytics HMAC storage key is not configured for PostgreSQL.",
                )
            })?;
        // RFC 9562 UUIDv8 with the RFC variant. This is only a compact database
        // representation of a keyed digest, not a public capability.
        Ok(derive_analytics_storage_id(external_id, secret.as_bytes()))
    }
    pub async fn persist_analytics_consent(
        &self,
        receipt: &AnalyticsConsentReceipt,
    ) -> Result<(), ApiError> {
        let Some(pool) = &self.pool else {
            return Ok(());
        };
        let storage_session_id = self.analytics_storage_session_id(receipt.anonymous_session_id)?;
        sqlx::query(
            r#"INSERT INTO consent_records
               (id, anonymous_session_id, policy_version, analytics_allowed, granted_at, expires_at)
               VALUES ($1,$2,$3,$4,$5,$6)"#,
        )
        .bind(receipt.consent_receipt)
        .bind(storage_session_id)
        .bind(&receipt.policy_version)
        .bind(receipt.analytics_allowed)
        .bind(receipt.granted_at)
        .bind(receipt.expires_at)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Returns a consent receipt only when it is the latest decision for its
    /// anonymous session. A later denial therefore invalidates every older
    /// allow receipt without needing a separate revocation flag.
    pub async fn current_analytics_consent(
        &self,
        receipt_id: Uuid,
    ) -> Result<Option<AnalyticsConsentReceipt>, ApiError> {
        if let Some(pool) = &self.pool {
            let row = sqlx::query(
                r#"SELECT requested.id, requested.anonymous_session_id,
                          requested.policy_version, requested.analytics_allowed,
                          requested.granted_at, requested.expires_at
                   FROM consent_records requested
                   WHERE requested.id = $1
                     AND requested.id = (
                       SELECT latest.id
                       FROM consent_records latest
                       WHERE latest.anonymous_session_id = requested.anonymous_session_id
                       ORDER BY latest.granted_at DESC, latest.id DESC
                       LIMIT 1
                     )"#,
            )
            .bind(receipt_id)
            .fetch_optional(pool)
            .await?;
            return row
                .map(|row| {
                    Ok(AnalyticsConsentReceipt {
                        consent_receipt: row.try_get("id")?,
                        anonymous_session_id: row.try_get("anonymous_session_id")?,
                        policy_version: row.try_get("policy_version")?,
                        analytics_allowed: row.try_get("analytics_allowed")?,
                        granted_at: row.try_get("granted_at")?,
                        expires_at: row.try_get("expires_at")?,
                    })
                })
                .transpose();
        }

        let data = self.data.read().await;
        let requested = match data.analytics_consents.get(&receipt_id) {
            Some(receipt) => receipt,
            None => return Ok(None),
        };
        let latest = data
            .analytics_consents
            .values()
            .filter(|candidate| candidate.anonymous_session_id == requested.anonymous_session_id)
            .max_by(|left, right| {
                left.granted_at
                    .cmp(&right.granted_at)
                    .then_with(|| left.consent_receipt.cmp(&right.consent_receipt))
            });
        Ok(latest
            .filter(|latest| latest.consent_receipt == receipt_id)
            .cloned())
    }

    pub async fn persist_analytics_event(
        &self,
        event_id: Uuid,
        event: &Value,
        occurred_at: DateTime<Utc>,
    ) -> Result<(), ApiError> {
        let external_session_id = event
            .get("anonymousSessionId")
            .and_then(Value::as_str)
            .and_then(|value| Uuid::parse_str(value).ok())
            .ok_or_else(|| ApiError::bad_request("anonymousSessionId is invalid."))?;
        let consent_record_id = event
            .get("consentReceipt")
            .and_then(Value::as_str)
            .and_then(|value| Uuid::parse_str(value).ok())
            .ok_or_else(|| ApiError::bad_request("consentReceipt is invalid."))?;
        let event_name = event
            .get("eventName")
            .and_then(Value::as_str)
            .ok_or_else(|| ApiError::bad_request("eventName is invalid."))?
            .to_owned();
        let Some(pool) = &self.pool else {
            let mut data = self.data.write().await;
            let guest_visit_id = data
                .guest_visits
                .values()
                .filter(|visit| {
                    visit.anonymous_session_id == external_session_id
                        && data.guest_visit_consent_records.get(&visit.id)
                            == Some(&consent_record_id)
                        && visit.retention_until > occurred_at
                })
                .max_by_key(|visit| visit.last_seen_at)
                .map(|visit| visit.id)
                .ok_or_else(|| {
                    ApiError::validation(BTreeMap::from([(
                        "anonymousSessionId".into(),
                        vec![
                            "Create the consented guest visit before sending analytics events."
                                .into(),
                        ],
                    )]))
                })?;
            data.analytics_events.insert(
                event_id,
                StoredAnalyticsEvent {
                    event_name,
                    guest_visit_id,
                    occurred_at,
                },
            );
            return Ok(());
        };
        let storage_session_id = self.analytics_storage_session_id(external_session_id)?;
        let mut transaction = pool.begin().await?;
        let guest_visit_id = sqlx::query_scalar::<_, Uuid>(
            r#"SELECT id FROM guest_visits
               WHERE anonymous_session_id=$1 AND consent_record_id=$2
                 AND consent_analytics_allowed=true AND retention_until > now()
               ORDER BY last_seen_at DESC LIMIT 1"#,
        )
        .bind(storage_session_id)
        .bind(consent_record_id)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or_else(|| {
            ApiError::validation(BTreeMap::from([(
                "anonymousSessionId".into(),
                vec!["Create the consented guest visit before sending analytics events.".into()],
            )]))
        })?;
        sqlx::query(
            r#"INSERT INTO analytics_events
               (id, event_name, source_path, locale, anonymous_session_id, properties,
                occurred_at,guest_visit_id,consent_record_id)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)"#,
        )
        .bind(event_id)
        .bind(
            event
                .get("eventName")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        )
        .bind(
            event
                .get("sourcePath")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        )
        .bind(event.get("locale").and_then(Value::as_str).unwrap_or("en"))
        .bind(storage_session_id)
        .bind(
            event
                .get("properties")
                .cloned()
                .unwrap_or(Value::Object(Default::default())),
        )
        .bind(occurred_at)
        .bind(guest_visit_id)
        .bind(consent_record_id)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(())
    }
}
