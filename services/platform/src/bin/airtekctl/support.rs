fn validate_mapping_version(value: &str) -> Result<(), Box<dyn std::error::Error>> {
    let value = value.trim();
    if value.is_empty() || value.len() > 120 {
        return Err("mapping version must contain 1 to 120 characters".into());
    }
    Ok(())
}

fn validate_cursor(value: &Option<String>) -> Result<(), Box<dyn std::error::Error>> {
    if value.as_ref().is_some_and(|value| {
        value.trim().is_empty() || value.len() > 2_048 || value.chars().any(char::is_control)
    }) {
        return Err("cursor must be a non-empty opaque value of at most 2048 characters".into());
    }
    Ok(())
}

async fn insert_cli_audit(
    transaction: &mut Transaction<'_, Postgres>,
    action: &str,
    entity_type: &str,
    entity_id: Option<Uuid>,
    after: Value,
    reason: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO audit_log
           (id, actor, action, entity_type, entity_id, before_value, after_value,
            reason, request_id, occurred_at)
           VALUES ($1,$2,$3,$4,$5,NULL,$6,$7,$8,now())"#,
    )
    .bind(Uuid::new_v4())
    .bind(cli_actor())
    .bind(action)
    .bind(entity_type)
    .bind(entity_id)
    .bind(after)
    .bind(reason)
    .bind(Uuid::new_v4())
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

fn cli_actor() -> String {
    std::process::Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .filter(|value| value.status.success())
        .map(|value| format!("osUid:{}", String::from_utf8_lossy(&value.stdout).trim()))
        .unwrap_or_else(|| "osUid:unavailable".into())
}

fn print_json(value: Value) -> Result<(), serde_json::Error> {
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

fn ensure_development_runtime() -> Result<(), Box<dyn std::error::Error>> {
    if cfg!(feature = "production") {
        return Err("airtekctl is unavailable in production builds".into());
    }
    for key in [
        "AIRTEK_RUNTIME_ENV",
        "AIRTEK_ENVIRONMENT",
        "AIRTEK_ENV",
        "APP_ENV",
        "NODE_ENV",
    ] {
        if std::env::var(key)
            .ok()
            .as_deref()
            .is_some_and(is_production_marker)
        {
            return Err(format!(
                "airtekctl refused to run because {key} marks a production runtime"
            )
            .into());
        }
    }
    if std::env::var("AIRTEK_PRODUCTION")
        .ok()
        .as_deref()
        .is_some_and(is_true_marker)
    {
        return Err("airtekctl refused to run because AIRTEK_PRODUCTION is enabled".into());
    }
    Ok(())
}

fn is_production_marker(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "production" | "prod" | "live"
    )
}

fn is_true_marker(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}
