fn validate_time_range(
    from: Option<chrono::DateTime<Utc>>,
    to: Option<chrono::DateTime<Utc>>,
) -> Result<(), ApiError> {
    if from.zip(to).is_some_and(|(from, to)| from >= to) {
        Err(ApiError::bad_request("from must be before to."))
    } else {
        Ok(())
    }
}

fn analytics_cursor_scope(
    endpoint: &str,
    from: Option<chrono::DateTime<Utc>>,
    to: Option<chrono::DateTime<Utc>>,
) -> String {
    let bound = |value: Option<chrono::DateTime<Utc>>| {
        value
            .map(|value| value.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true))
            .unwrap_or_else(|| "*".into())
    };
    format!("{endpoint}|from={}|to={}", bound(from), bound(to))
}

fn decode_analytics_cursor(
    scope: &str,
    value: Option<&str>,
) -> Result<Option<AnalyticsCursor>, ApiError> {
    value
        .map(|value| decode_scoped_cursor::<AnalyticsCursor>(scope, value))
        .transpose()?
        .map(|cursor| {
            if cursor.dimension_hash.len() != 32 {
                Err(ApiError::bad_request("cursor is invalid."))
            } else {
                Ok(cursor)
            }
        })
        .transpose()
}

fn analytics_cursor_from_database_row(
    row: &sqlx::postgres::PgRow,
) -> Result<AnalyticsCursor, ApiError> {
    let dimension_hash: Vec<u8> = row.try_get("dimension_hash")?;
    if dimension_hash.len() != 32 {
        return Err(ApiError::service_unavailable(
            "Stored analytics dimension identity is invalid.",
        ));
    }
    Ok(AnalyticsCursor {
        bucket_date: row.try_get("bucket_date")?,
        dimension_hash,
    })
}

fn ensure_no_analytics_hash_collision(count: i64) -> Result<(), ApiError> {
    if count == 1 {
        Ok(())
    } else {
        Err(ApiError::service_unavailable(
            "Analytics dimension identity collision detected.",
        ))
    }
}

fn analytics_dimension_hash(parts: &[&str]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part.as_bytes());
    }
    hasher.finalize().to_vec()
}

fn analytics_key_is_after(value: &AnalyticsCursor, after: &AnalyticsCursor) -> bool {
    value.bucket_date < after.bucket_date
        || (value.bucket_date == after.bucket_date && value.dimension_hash > after.dimension_hash)
}

fn finish_analytics_page<T>(
    scope: &str,
    mut values: Vec<(T, AnalyticsCursor)>,
    limit: usize,
) -> Result<CursorPage<T>, ApiError> {
    let has_more = values.len() > limit;
    values.truncate(limit);
    let next_cursor = if has_more {
        values
            .last()
            .map(|(_, cursor)| encode_scoped_cursor(scope, cursor))
            .transpose()?
    } else {
        None
    };
    Ok(CursorPage {
        items: values.into_iter().map(|(value, _)| value).collect(),
        next_cursor,
    })
}

fn paginate_memory_analytics<T>(
    scope: &str,
    mut values: Vec<(T, AnalyticsCursor)>,
    after: Option<&AnalyticsCursor>,
    limit: usize,
) -> Result<CursorPage<T>, ApiError> {
    let mut identities = HashSet::with_capacity(values.len());
    if values
        .iter()
        .any(|(_, cursor)| !identities.insert(cursor.clone()))
    {
        return Err(ApiError::service_unavailable(
            "Analytics dimension identity collision detected.",
        ));
    }
    values.sort_by(|(_, left), (_, right)| {
        right
            .bucket_date
            .cmp(&left.bucket_date)
            .then_with(|| left.dimension_hash.cmp(&right.dimension_hash))
    });
    if let Some(after) = after {
        values.retain(|(_, cursor)| analytics_key_is_after(cursor, after));
    }
    values.truncate(limit + 1);
    finish_analytics_page(scope, values, limit)
}
