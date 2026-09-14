#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuditQuery {
    cursor: Option<String>,
    limit: Option<usize>,
    actor: Option<String>,
    action: Option<String>,
    resource_type: Option<String>,
    resource_id: Option<Uuid>,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    q: Option<String>,
}

async fn list_audit(
    State(state): State<AppState>,
    Query(query): Query<AuditQuery>,
) -> Result<Json<crate::models::AuditEventPage>, ApiError> {
    let (values, scope) = filtered_audit(&state, &query).await?;
    let total = values.len();
    let page = crate::pagination::paginate_by_id_scoped(
        &scope,
        values,
        CursorQuery {
            cursor: query.cursor,
            limit: query.limit,
        },
        |event| event.id,
    )?;
    Ok(Json(crate::models::AuditEventPage {
        items: page.items,
        next_cursor: page.next_cursor,
        total,
    }))
}

async fn export_audit_csv(
    State(state): State<AppState>,
    Query(query): Query<AuditQuery>,
) -> Result<Response, ApiError> {
    let (values, _) = filtered_audit(&state, &query).await?;
    let mut csv = String::from(
        "id,occurred_at,actor,action,resource_type,resource_id,request_id,reason\r\n",
    );
    for event in values {
        let fields = [
            event.id.to_string(),
            event.occurred_at.to_rfc3339(),
            event.actor,
            event.action,
            event.entity_type,
            event.entity_id.map(|value| value.to_string()).unwrap_or_default(),
            event.request_id.to_string(),
            event.reason.unwrap_or_default(),
        ];
        csv.push_str(&fields.map(|field| csv_cell(&field)).join(","));
        csv.push_str("\r\n");
    }
    let mut response = (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/csv; charset=utf-8")],
        csv,
    )
        .into_response();
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=airtek-audit.csv"),
    );
    Ok(response)
}

async fn filtered_audit(
    state: &AppState,
    query: &AuditQuery,
) -> Result<(Vec<AuditEvent>, String), ApiError> {
    if query.from.zip(query.to).is_some_and(|(from, to)| from >= to) {
        return Err(ApiError::bad_request("from must be earlier than to."));
    }
    let q = parse_query_text(query.q.clone())?.map(|value| value.to_lowercase());
    let actor = query.actor.as_deref().map(str::trim).filter(|value| !value.is_empty());
    let action = query.action.as_deref().map(str::trim).filter(|value| !value.is_empty());
    let resource_type = query.resource_type.as_deref().map(str::trim).filter(|value| !value.is_empty());
    let mut values = state.list_stored_audit().await?;
    values.retain(|event| {
        actor.is_none_or(|value| event.actor.eq_ignore_ascii_case(value))
            && action.is_none_or(|value| event.action == value)
            && resource_type.is_none_or(|value| event.entity_type == value)
            && query.resource_id.is_none_or(|value| event.entity_id == Some(value))
            && query.from.is_none_or(|value| event.occurred_at >= value)
            && query.to.is_none_or(|value| event.occurred_at < value)
            && q.as_ref().is_none_or(|needle| {
                format!(
                    "{} {} {} {} {} {}",
                    event.actor,
                    event.action,
                    event.entity_type,
                    event.entity_id.map(|value| value.to_string()).unwrap_or_default(),
                    event.request_id,
                    event.reason.as_deref().unwrap_or("")
                )
                .to_lowercase()
                .contains(needle)
            })
    });
    values.sort_by_key(|event| Reverse((event.occurred_at, event.id)));
    let scope = format!(
        "admin.audit|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
        actor, action, resource_type, query.resource_id, query.from, query.to, q
    );
    Ok((values, scope))
}

fn csv_cell(value: &str) -> String {
    if value.chars().any(|character| matches!(character, ',' | '"' | '\r' | '\n')) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

fn entity_response<T: serde::Serialize>(status: StatusCode, value: &T, revision: i64) -> Response {
    let mut response = (status, Json(value)).into_response();
    response.headers_mut().insert(
        "etag",
        HeaderValue::from_str(&etag(revision)).expect("revision ETag is valid"),
    );
    response
}
