use super::*;

pub struct GuestVisitWrite<'a> {
    pub storage_session_id: Uuid,
    pub browser_session_id: Uuid,
    pub consent_receipt: Uuid,
    pub locale: &'a str,
    pub landing_path: &'a str,
    pub source: &'a str,
    pub referrer_domain: Option<&'a str>,
    pub utm_source: Option<&'a str>,
    pub medium: Option<&'a str>,
    pub campaign: Option<&'a str>,
    pub now: DateTime<Utc>,
    pub retention_until: DateTime<Utc>,
}

pub async fn upsert_guest_visit(
    pool: &sqlx::PgPool,
    write: GuestVisitWrite<'_>,
) -> Result<(GuestVisit, bool), ApiError> {
    let existing_id = sqlx::query_scalar::<_, Uuid>(
        r#"SELECT id FROM guest_visits
           WHERE anonymous_session_id=$1 AND consent_record_id=$2 AND retention_until > now()
           ORDER BY first_seen_at DESC LIMIT 1"#,
    )
    .bind(write.storage_session_id)
    .bind(write.consent_receipt)
    .fetch_optional(pool)
    .await?;
    let id = existing_id.unwrap_or_else(Uuid::new_v4);
    let row = sqlx::query(
        r#"INSERT INTO guest_visits
           (id, anonymous_session_id, consent_record_id, consent_analytics_allowed,
            locale, landing_path, source_type, referrer_host, utm_source, utm_medium,
            utm_campaign, first_seen_at, last_seen_at, retention_until, created_at)
           VALUES ($1,$2,$3,true,$4,$5,$6,$7,$8,$9,$10,$11,$11,$12,$11)
           ON CONFLICT (id) DO UPDATE SET last_seen_at=EXCLUDED.last_seen_at
           RETURNING id, anonymous_session_id, landing_path, referrer_host, source_type,
                     utm_medium, utm_campaign, first_seen_at, last_seen_at, retention_until"#,
    )
    .bind(id)
    .bind(write.storage_session_id)
    .bind(write.consent_receipt)
    .bind(write.locale)
    .bind(write.landing_path)
    .bind(write.source)
    .bind(write.referrer_domain)
    .bind(write.utm_source)
    .bind(write.medium)
    .bind(write.campaign)
    .bind(write.now)
    .bind(write.retention_until)
    .fetch_one(pool)
    .await?;
    let mut visit = GuestVisit {
        id: row.try_get("id")?,
        anonymous_session_id: row.try_get("anonymous_session_id")?,
        landing_path: row.try_get("landing_path")?,
        referrer_domain: row.try_get("referrer_host")?,
        source: row.try_get("source_type")?,
        medium: row.try_get("utm_medium")?,
        campaign: row.try_get("utm_campaign")?,
        first_seen_at: row.try_get("first_seen_at")?,
        last_seen_at: row.try_get("last_seen_at")?,
        retention_until: row.try_get("retention_until")?,
    };
    visit.anonymous_session_id = write.browser_session_id;
    Ok((visit, existing_id.is_none()))
}
