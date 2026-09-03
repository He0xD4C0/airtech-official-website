fn find_header(index: &HashMap<String, usize>, aliases: &[&str]) -> Option<usize> {
    aliases.iter().find_map(|alias| index.get(*alias).copied())
}

fn field(record: &[String], index: Option<usize>) -> &str {
    index
        .and_then(|index| record.get(index))
        .map(String::as_str)
        .unwrap_or_default()
}

fn normalize_header(value: &str) -> String {
    value
        .trim()
        .trim_start_matches('\u{feff}')
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn normalize_family(value: &str) -> Option<&'static str> {
    let compact = normalize_header(value);
    match compact.as_str() {
        "centrifugal"
        | "centrifugalfan"
        | "centrifugalfans"
        | "plugfan"
        | "离心"
        | "离心风机"
        | "离心风机centrifugalfans" => Some("centrifugal"),
        "axial" | "axialfan" | "轴流" | "轴流风机" => Some("axial"),
        "crossflow" | "crossflowfan" | "贯流" | "横流" | "贯流风机" => Some("crossFlow"),
        "inlineduct" | "inline" | "duct" | "ductfan" | "inlineductfan" | "管道" | "管道风机" => {
            Some("inlineDuct")
        }
        "motor" | "motors" | "电机" => Some("motors"),
        _ => None,
    }
}

fn normalize_motor_technology(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let compact = normalize_header(value);
    let normalized = match compact.as_str() {
        "ec" | "electronicallycommutated" => "EC",
        "ac" => "AC",
        "dc" => "DC",
        "bldc" => "BLDC",
        _ if compact.starts_with("ec") => "EC",
        _ if compact.starts_with("ac") => "AC",
        _ => value,
    };
    Some(normalized.to_owned())
}

fn slugify(value: &str) -> String {
    let mut slug = String::new();
    let mut separator = false;
    for character in value.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            if separator && !slug.is_empty() {
                slug.push('-');
            }
            slug.push(character);
            separator = false;
        } else {
            separator = true;
        }
    }
    slug
}

fn valid_locale(value: &str) -> bool {
    (2..=35).contains(&value.len())
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn non_empty(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn none_if_empty(value: &str) -> Option<&str> {
    (!value.trim().is_empty()).then_some(value)
}
