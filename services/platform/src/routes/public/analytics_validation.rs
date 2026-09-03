fn validate_analytics_event(event: &CreateAnalyticsEvent) -> Result<(), ApiError> {
    let mut errors = BTreeMap::new();
    let Some(allowed_properties) = analytics_property_dictionary(&event.event_name) else {
        errors.insert(
            "eventName".into(),
            vec!["Event name is not in the approved analytics dictionary.".into()],
        );
        return Err(ApiError::validation(errors));
    };
    if !(event.source_path == "/en" || event.source_path.starts_with("/en/"))
        || !super::public_data::valid_guest_landing_path(&event.source_path)
    {
        errors.insert(
            "sourcePath".into(),
            vec!["Source path must be a canonical /en path without query or fragment data.".into()],
        );
    }
    if event.locale != "en" {
        errors.insert(
            "locale".into(),
            vec!["Only the published English locale is currently accepted.".into()],
        );
    }
    if event.properties.len() > allowed_properties.len() {
        errors.insert(
            "properties".into(),
            vec!["Analytics event contains more properties than its approved schema.".into()],
        );
    }
    for (key, value) in &event.properties {
        let field = format!("properties.{key}");
        if !allowed_properties.contains(&key.as_str()) {
            errors.insert(
                field,
                vec!["Property is not allowed for this analytics event.".into()],
            );
            continue;
        }
        if !analytics_scalar_is_valid(&event.event_name, key, value) {
            errors.insert(
                field,
                vec!["Property has an invalid type, value, length, or PII-like content.".into()],
            );
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ApiError::validation(errors))
    }
}

fn analytics_property_dictionary(event_name: &str) -> Option<&'static [&'static str]> {
    match event_name {
        "pageView" => Some(&["contentKind", "contentId", "publishedRevision"]),
        "internalSearch" => Some(&["queryLength", "resultCount"]),
        // Filter values and FAQ categories originate in editable content and
        // are intentionally not persisted as event properties. The event,
        // stable filter/FAQ identifier and aggregate count are sufficient for
        // funnel reporting without creating a free-text PII channel.
        "filterApplied" => Some(&["filterName", "resultCount"]),
        "selectorStarted" => Some(&["constraintCount", "preferredFamily", "priority"]),
        "selectorStepCompleted" => Some(&["step", "constraintCount"]),
        "selectorResult" => Some(&["outcome", "candidateCount"]),
        "compareChanged" => Some(&["action", "itemCount", "productId", "productRevision"]),
        "downloadStarted" => Some(&["downloadId", "productId", "productRevision"]),
        "faqExpanded" => Some(&["faqId"]),
        "ctaClicked" => Some(&["ctaId", "placement"]),
        "rfqRouteSelected" => Some(&["journey"]),
        "rfqStarted" => Some(&["journey", "productId", "productRevision"]),
        "rfqStepCompleted" => Some(&["journey", "step"]),
        "rfqValidationError" => Some(&["journey", "step", "fieldName", "errorCode"]),
        "rfqSubmitted" => Some(&["journey"]),
        "rfqSubmitFailed" => Some(&["journey", "errorCode"]),
        _ => None,
    }
}

fn analytics_scalar_is_valid(event_name: &str, key: &str, value: &Value) -> bool {
    match value {
        Value::String(value) => {
            if !valid_analytics_string(value, 160) {
                return false;
            }
            match key {
                "contentId" | "productId" | "downloadId" => Uuid::parse_str(value).is_ok(),
                "journey" => {
                    ["product", "selection", "project", "replacement"].contains(&value.as_str())
                }
                "priority" => ["efficiency", "noise", "size", "headroom"].contains(&value.as_str()),
                "preferredFamily" => [
                    "open",
                    "centrifugal",
                    "axial",
                    "crossFlow",
                    "inlineDuct",
                    "motors",
                ]
                .contains(&value.as_str()),
                "outcome" => [
                    "matched",
                    "noValidatedCandidates",
                    "engineeringReviewRequired",
                ]
                .contains(&value.as_str()),
                "action" if event_name == "compareChanged" => {
                    ["add", "remove", "clear"].contains(&value.as_str())
                }
                "contentKind" => ["content", "news", "product"].contains(&value.as_str()),
                "filterName" => [
                    "resourceType",
                    "applicableModel",
                    "contentType",
                    "catalogSearch",
                    "family",
                    "motorTechnology",
                    "catalogFilters",
                    "catalogPagination",
                ]
                .contains(&value.as_str()),
                "faqId" => valid_faq_analytics_id(value),
                "ctaId" => ["content-primary", "download-record-open", "request-quote"]
                    .contains(&value.as_str()),
                "placement" => ["content-panel", "downloads-list", "product-detail", "hero"]
                    .contains(&value.as_str()),
                "fieldName" => ["productContext"].contains(&value.as_str()),
                "errorCode" => {
                    ["publishedContextRequired", "apiRejected"].contains(&value.as_str())
                }
                _ => false,
            }
        }
        Value::Number(value) => {
            let Some(value) = value.as_u64() else {
                return false;
            };
            match key {
                "queryLength" => (1..=500).contains(&value),
                "resultCount" | "candidateCount" | "constraintCount" => value <= 1_000_000,
                "itemCount" => value <= 4,
                "step" => (1..=20).contains(&value),
                "publishedRevision" | "productRevision" => (1..=i64::MAX as u64).contains(&value),
                _ => false,
            }
        }
        // The approved dictionary currently has no nullable, boolean, array,
        // or object-valued properties. Reject them instead of recursively
        // accepting a future source of form content or identifiers.
        Value::Null | Value::Bool(_) | Value::Array(_) | Value::Object(_) => false,
    }
}

fn valid_faq_analytics_id(value: &str) -> bool {
    Uuid::parse_str(value).is_ok()
        || value.strip_prefix("faq-").is_some_and(|ordinal| {
            !ordinal.starts_with('0')
                && (1..=6).contains(&ordinal.len())
                && ordinal.bytes().all(|byte| byte.is_ascii_digit())
        })
}

fn valid_analytics_string(value: &str, maximum_length: usize) -> bool {
    let value = value.trim();
    !value.is_empty()
        && value.chars().count() <= maximum_length
        && !value.chars().any(char::is_control)
        && !looks_like_email(value)
        && !looks_like_phone(value)
        && value.parse::<std::net::IpAddr>().is_err()
        && !looks_like_secret(value)
}

fn looks_like_email(value: &str) -> bool {
    value.split_whitespace().any(|token| {
        let token = token.trim_matches(|character: char| {
            matches!(character, '<' | '>' | '(' | ')' | '[' | ']' | ',' | ';')
        });
        token.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty() && domain.contains('.') && !domain.ends_with('.')
        })
    })
}

fn looks_like_phone(value: &str) -> bool {
    let digit_count = value.bytes().filter(u8::is_ascii_digit).count();
    (8..=15).contains(&digit_count)
        && value.bytes().all(|byte| {
            byte.is_ascii_digit() || matches!(byte, b' ' | b'+' | b'-' | b'(' | b')' | b'.')
        })
}

fn looks_like_secret(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.starts_with("bearer ")
        || lower.starts_with("basic ")
        || lower.starts_with("sk-")
        || lower.contains("api_key=")
        || lower.contains("apikey=")
        || lower.contains("token=")
}

fn reference(prefix: &str, id: Uuid, at: chrono::DateTime<Utc>) -> String {
    let compact = id.simple().to_string();
    format!("{prefix}-{}-{}", at.format("%Y%m%d"), &compact[..8]).to_uppercase()
}

fn default_locale() -> String {
    "en".into()
}
