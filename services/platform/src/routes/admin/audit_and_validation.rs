async fn list_audit(
    State(state): State<AppState>,
    Query(query): Query<CursorQuery>,
) -> Result<Json<CursorPage<AuditEvent>>, ApiError> {
    let mut values = state.list_stored_audit().await?;
    values.sort_by_key(|event| Reverse(event.occurred_at));
    Ok(Json(paginate_by_id(
        "admin.audit",
        values,
        query,
        |event| event.id,
    )?))
}

const DEDICATED_NEWS_API_DETAIL: &str =
    "News must be managed through the dedicated /api/admin/v1/news API.";

fn reject_generic_news_input(kind: ContentKind) -> Result<(), ApiError> {
    if kind == ContentKind::News {
        return Err(ApiError::validation(BTreeMap::from([(
            "kind".into(),
            vec![DEDICATED_NEWS_API_DETAIL.into()],
        )])));
    }
    Ok(())
}

fn reject_generic_news_entry(entry: &ContentEntry) -> Result<(), ApiError> {
    if entry.kind == ContentKind::News {
        return Err(ApiError::conflict(DEDICATED_NEWS_API_DETAIL));
    }
    Ok(())
}

pub(super) fn validate_content_input(input: &ContentDraftInput) -> Result<(), ApiError> {
    let mut errors = BTreeMap::new();
    if input.title.trim().is_empty() || input.title.len() > 300 {
        errors.insert(
            "title".into(),
            vec!["Title must contain 1 to 300 characters.".into()],
        );
    }
    if !valid_slug(&input.slug) {
        errors.insert(
            "slug".into(),
            vec!["Slug must contain lowercase letters, digits and single hyphens.".into()],
        );
    }
    if input.locale != "en" {
        errors.insert(
            "locale".into(),
            vec!["Only the launch locale `en` is enabled.".into()],
        );
    }
    if input.body.schema_version != 1 {
        errors.insert(
            "body.schemaVersion".into(),
            vec!["Only schema version 1 is supported.".into()],
        );
    }
    if let Err(detail) = validate_rich_text_node(&input.body.doc) {
        errors.insert("body.doc".into(), vec![detail]);
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ApiError::validation(errors))
    }
}

fn validate_rich_text_node(value: &Value) -> Result<(), String> {
    const ALLOWED_TYPES: &[&str] = &[
        "doc",
        "paragraph",
        "text",
        "heading",
        "bulletList",
        "orderedList",
        "listItem",
        "blockquote",
        "callout",
        "hardBreak",
        "image",
        "gallery",
        "table",
        "tableRow",
        "tableCell",
        "tableHeader",
        "codeBlock",
        "mathBlock",
        "cta",
        "relatedContent",
        "entityBlock",
        "bold",
        "italic",
        "strike",
        "underline",
        "code",
        "link",
    ];
    match value {
        Value::Object(map) => {
            if map.keys().any(|key| {
                matches!(
                    key.to_ascii_lowercase().as_str(),
                    "html" | "rawhtml" | "script" | "style" | "iframe"
                )
            }) {
                return Err("Raw HTML, script, style and iframe fields are not allowed.".into());
            }
            if let Some(node_type) = map.get("type").and_then(Value::as_str) {
                if !ALLOWED_TYPES.contains(&node_type) {
                    return Err(format!("Node or mark type `{node_type}` is not allowed."));
                }
                if node_type == "entityBlock" {
                    const ENTITY_KINDS: &[&str] = &[
                        "media",
                        "cta",
                        "related-product",
                        "related-content",
                        "formula",
                    ];
                    let kind = map
                        .get("attrs")
                        .and_then(Value::as_object)
                        .and_then(|attrs| attrs.get("kind"))
                        .and_then(Value::as_str)
                        .ok_or_else(|| "entityBlock requires a string attrs.kind.".to_owned())?;
                    if !ENTITY_KINDS.contains(&kind) {
                        return Err(format!("entityBlock attrs.kind `{kind}` is not allowed."));
                    }
                }
            }
            for child in map.values() {
                validate_rich_text_node(child)?;
            }
        }
        Value::Array(items) => {
            for child in items {
                validate_rich_text_node(child)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 180
        && !value.starts_with('-')
        && !value.ends_with('-')
        && !value.contains("--")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn confirmation_phrase(kind: OperationKind) -> &'static str {
    match kind {
        OperationKind::MigrationPreflight => "PREFLIGHT MIGRATION",
        OperationKind::MigrationApply => "APPLY MIGRATION",
        OperationKind::Backup => "CREATE BACKUP",
        OperationKind::RestoreValidate => "VALIDATE RESTORE",
        OperationKind::RetentionApply => "APPLY RETENTION",
        OperationKind::SearchReindex => "REBUILD SEARCH INDEX",
        OperationKind::CacheInvalidate => "INVALIDATE PUBLIC CACHE",
        OperationKind::FeishuSync => "START FEISHU SYNC",
        OperationKind::ProductImport => "IMPORT PRODUCT MASTER",
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
