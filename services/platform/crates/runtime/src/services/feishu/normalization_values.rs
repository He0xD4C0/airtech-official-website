use serde_json::{json, Value};

/// Accept only units explicitly written in a field name or source cell.
pub(super) fn cell_value(key: &str, raw: &Value) -> Option<(Value, Option<&'static str>)> {
    let Some(text) = raw.as_str() else {
        return Some((raw.clone(), None));
    };
    let text = text.trim();
    if ["", "/", "—", "-", "N/A", "n/a"].contains(&text) {
        return None;
    }
    let suffixes: &[(&str, &str)] = match key {
        "diameter" | "impellerLength" | "dimensionA" | "dimensionB" | "dimensionC"
        | "productDimensions" | "packageDimensions" => &[("mm", "mm"), ("cm", "cm")],
        "weight" => &[("kg", "kg"), ("g", "g")],
        "ambientTemperature" => &[("℃", "°C"), ("°C", "°C"), ("°c", "°C")],
        "voltage" => &[("V", "V"), ("v", "V")],
        "frequency" => &[("Hz", "Hz"), ("hz", "Hz")],
        _ => &[],
    };
    let (value, unit) = suffixes
        .iter()
        .find_map(|(suffix, unit)| {
            text.strip_suffix(suffix)
                .map(|value| (value.trim(), Some(*unit)))
        })
        .unwrap_or((text, None));
    let parts: Vec<_> = value.split(',').collect();
    let thousands = parts.len() > 1
        && (1..=3).contains(&parts[0].len())
        && parts[0].bytes().all(|byte| byte.is_ascii_digit())
        && parts[1..]
            .iter()
            .all(|part| part.len() == 3 && part.bytes().all(|byte| byte.is_ascii_digit()));
    let number_text = if thousands {
        parts.concat()
    } else {
        value.to_owned()
    };
    // Multi-frequency values and ranges remain source text; no operating point is invented.
    let value = number_text
        .parse::<f64>()
        .ok()
        .filter(|n| n.is_finite())
        .map(|number| json!(number))
        .unwrap_or_else(|| json!(value));
    Some((value, unit))
}

pub(super) fn motor_technology(explicit: Option<&str>, subtype: Option<&str>) -> Option<String> {
    let text = explicit.or(subtype)?.trim();
    let first = text
        .split(|character: char| !character.is_ascii_alphabetic())
        .next()?;
    let technologies: std::collections::BTreeSet<_> = text
        .split(|character: char| !character.is_ascii_alphabetic())
        .filter_map(|token| {
            ["AC", "DC", "EC"]
                .into_iter()
                .find(|value| token.eq_ignore_ascii_case(value))
        })
        .collect();
    if technologies.len() != 1 {
        return None;
    }
    ["AC", "DC", "EC"]
        .iter()
        .find(|value| first.eq_ignore_ascii_case(value))
        .map(|value| (*value).into())
}

pub(super) fn attachment_usage(name: &str) -> &'static str {
    let lower = name.to_ascii_lowercase();
    if name.contains("曲线") || lower.contains("pq") {
        "curve"
    } else if lower.contains("3d") || lower.contains("step") || lower.contains("cad") {
        "cad"
    } else if name.contains("图纸") || lower.contains("drawing") || lower.contains("2d") {
        "drawing"
    } else if lower.contains("cad")
        || lower.contains("3d")
        || lower.contains("step")
        || lower.contains("dwg")
    {
        "cad"
    } else if name.contains("规格") || name.contains("说明书") || lower.contains("datasheet") {
        "datasheet"
    } else {
        "technicalDocument"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_multi_values_and_parses_only_explicit_units() {
        assert_eq!(
            cell_value("diameter", &json!("30mm")),
            Some((json!(30.0), Some("mm")))
        );
        assert_eq!(
            cell_value("airflow", &json!(" 2,949 ")),
            Some((json!(2949.0), None))
        );
        assert_eq!(
            cell_value("frequency", &json!("50/60")),
            Some((json!("50/60"), None))
        );
        assert_eq!(cell_value("power", &json!("/")), None);
        assert_eq!(cell_value("power", &json!("0")), Some((json!(0.0), None)));
        assert_eq!(
            cell_value("ambientTemperature", &json!("-25~+60℃")),
            Some((json!("-25~+60"), Some("°C")))
        );
    }
    #[test]
    fn does_not_infer_motor_from_model_or_expand_ambiguous_technology() {
        assert_eq!(
            motor_technology(None, Some("EC后倾式_EC Backward fans")),
            Some("EC".into())
        );
        assert_eq!(
            motor_technology(None, Some("DC贯流风机")),
            Some("DC".into())
        );
        assert_eq!(motor_technology(None, Some("BLEC Motor")), None);
        assert_eq!(motor_technology(None, Some("AC/DC")), None);
    }
}
