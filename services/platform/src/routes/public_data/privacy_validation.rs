fn normalize_guest_visit(mut request: CreateGuestVisit) -> CreateGuestVisit {
    request.referrer_domain = request
        .referrer_domain
        .map(|value| value.trim().to_ascii_lowercase());
    request.source = normalize_optional_attribution(request.source);
    request.medium = normalize_optional_attribution(request.medium);
    request.campaign = normalize_optional_attribution(request.campaign);
    request
}

fn normalize_optional_attribution(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_ascii_lowercase();
        (!value.is_empty()).then_some(value)
    })
}

fn valid_referrer_hostname(value: &str) -> bool {
    if value.chars().any(char::is_control) {
        return false;
    }
    let hostname = value.trim();
    if hostname.is_empty()
        || hostname.len() > 253
        || !hostname.is_ascii()
        || hostname.parse::<IpAddr>().is_ok()
        || contains_pii_like_value(hostname)
        || hostname.starts_with('.')
        || hostname.ends_with('.')
    {
        return false;
    }
    hostname.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !looks_like_phone(label)
            && label
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphanumeric)
            && label
                .as_bytes()
                .last()
                .is_some_and(u8::is_ascii_alphanumeric)
            && label
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    })
}

fn valid_guest_attribution(value: &str, maximum: usize, allowlist: &BTreeSet<String>) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return true;
    }
    let normalized = trimmed.to_ascii_lowercase();
    normalized.len() <= maximum
        && normalized.is_ascii()
        && normalized
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        && normalized
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric)
        && normalized
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
        && !contains_pii_like_value(&normalized)
        && allowlist.contains(&normalized)
}

pub(super) fn valid_guest_landing_path(path: &str) -> bool {
    if validate_public_path(path).is_err()
        || path.chars().any(char::is_control)
        || path.contains('\\')
    {
        return false;
    }
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .all(|segment| {
            let Some(decoded) = decode_safe_path_segment(segment) else {
                return false;
            };
            !contains_pii_like_value(segment) && !contains_pii_like_value(&decoded)
        })
}

/// Decode one path segment while rejecting encoded delimiters and identifier
/// markers. Encoded `%` is rejected as well so double encoding cannot bypass
/// the PII checks on a second decode.
fn decode_safe_path_segment(segment: &str) -> Option<String> {
    let bytes = segment.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            decoded.push(bytes[index]);
            index += 1;
            continue;
        }
        let high = *bytes.get(index + 1)?;
        let low = *bytes.get(index + 2)?;
        let byte = hex_value(high)? * 16 + hex_value(low)?;
        if byte.is_ascii_control()
            || matches!(
                byte,
                b'%' | b'@' | b'+' | b':' | b'?' | b'#' | b'/' | b'\\' | b'=' | b'&'
            )
        {
            return None;
        }
        decoded.push(byte);
        index += 3;
    }
    String::from_utf8(decoded).ok()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn contains_pii_like_value(value: &str) -> bool {
    looks_like_email(value)
        || contains_phone_like(value)
        || contains_ip_address(value)
        || contains_secret_like_value(value)
        || value
            .split(|character: char| !character.is_ascii_alphanumeric() && character != '-')
            .any(|token| Uuid::parse_str(token).is_ok())
}

fn looks_like_email(value: &str) -> bool {
    value.split_whitespace().any(|token| {
        let token = token.trim_matches(|character: char| {
            matches!(
                character,
                '<' | '>' | '(' | ')' | '[' | ']' | ',' | ';' | '"' | '\''
            )
        });
        token.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty() && domain.contains('.') && !domain.ends_with('.')
        })
    })
}

fn contains_phone_like(value: &str) -> bool {
    if looks_like_phone(value.trim()) {
        return true;
    }
    let mut digit_count = 0_usize;
    for byte in value.bytes().chain(std::iter::once(b'!')) {
        if byte.is_ascii_digit() {
            digit_count += 1;
        } else if !matches!(byte, b' ' | b'+' | b'-' | b'(' | b')' | b'.') {
            if (10..=15).contains(&digit_count) {
                return true;
            }
            digit_count = 0;
        }
    }
    false
}

fn looks_like_phone(value: &str) -> bool {
    let digit_count = value.bytes().filter(u8::is_ascii_digit).count();
    (8..=15).contains(&digit_count)
        && value.bytes().all(|byte| {
            byte.is_ascii_digit() || matches!(byte, b' ' | b'+' | b'-' | b'(' | b')' | b'.')
        })
}

fn contains_ip_address(value: &str) -> bool {
    value.parse::<IpAddr>().is_ok()
        || value
            .split(|character: char| {
                !(character.is_ascii_hexdigit() || matches!(character, '.' | ':'))
            })
            .filter(|candidate| !candidate.is_empty())
            .any(|candidate| candidate.parse::<IpAddr>().is_ok())
}

fn contains_secret_like_value(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.starts_with("bearer ")
        || lower.starts_with("basic ")
        || lower.starts_with("sk-")
        || lower.contains("api_key=")
        || lower.contains("apikey=")
        || lower.contains("token=")
        || lower.contains("mailto:")
        || lower.contains("tel:")
}

fn classify_source(request: &CreateGuestVisit) -> String {
    let source = request.source.as_deref().unwrap_or_default().to_lowercase();
    let medium = request.medium.as_deref().unwrap_or_default().to_lowercase();
    if source.is_empty() && request.referrer_domain.is_none() {
        "direct"
    } else if medium.contains("cpc") || medium.contains("paid") || medium.contains("ppc") {
        "paidSearch"
    } else if medium.contains("organic") {
        "organicSearch"
    } else if medium.contains("social") {
        "social"
    } else if medium.contains("email") {
        "email"
    } else if request.referrer_domain.is_some() {
        "referral"
    } else if !source.is_empty() {
        "other"
    } else {
        "unknown"
    }
    .into()
}
