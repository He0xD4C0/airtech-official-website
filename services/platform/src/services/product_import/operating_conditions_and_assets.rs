use super::*;

pub(super) fn normalized_operating_conditions(
    header_index: &HashMap<String, usize>,
    record: &[String],
) -> Vec<Value> {
    value_for(
        header_index,
        record,
        &["frequency", "频率frequency", "频率"],
    )
    .map(|value| {
        split_paired(&value)
            .into_iter()
            .enumerate()
            .map(|(index, frequency)| {
                json!({
                    "key": format!("frequency-{}", index + 1),
                    "frequencyHz": frequency,
                    "label": format!("{frequency} Hz"),
                })
            })
            .collect()
    })
    .unwrap_or_default()
}

pub(super) fn value_for(
    header_index: &HashMap<String, usize>,
    record: &[String],
    aliases: &[&str],
) -> Option<String> {
    non_empty(field(record, find_header(header_index, aliases)))
}

pub(super) fn split_paired(value: &str) -> Vec<String> {
    value
        .split('/')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
}

pub(super) fn is_asset_header(name: &str) -> bool {
    name.contains("asset")
        || name.contains("image")
        || name.contains("drawing")
        || name.contains("cad")
        || name.contains("certificate")
        || name.contains("datasheet")
        || name.contains("曲线图")
        || name.contains("图纸")
        || name.contains("规格书")
}

pub(super) fn asset_type(name: &str) -> &'static str {
    if name.contains("cad")
        || name.contains("drawing")
        || name.contains("2d图纸")
        || name.contains("3d图纸")
    {
        "cad"
    } else if name.contains("certificate") {
        "certificate"
    } else if name.contains("datasheet") || name.contains("规格书") {
        "datasheet"
    } else {
        "image"
    }
}

pub(super) fn split_asset_references(value: &str) -> impl Iterator<Item = String> + '_ {
    value
        .split([';', '|'])
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

pub(super) fn is_safe_optional_header(name: &str) -> bool {
    // Do not ever place confidential price, source noise measurements, or
    // asset locator fields in the public candidate payload.
    !name.contains("price")
        && !name.contains("cost")
        && !name.contains("noise")
        && !name.contains("sound")
        && !name.contains("db")
        && !is_asset_header(name)
        && matches!(
            name,
            "voltage"
                | "frequency"
                | "frequencyhz"
                | "power"
                | "powerunit"
                | "airflow"
                | "airflowunit"
                | "pressure"
                | "pressureunit"
                | "speed"
                | "speedunit"
                | "diameter"
                | "diameterunit"
                | "certifications"
                | "iprating"
        )
}

pub(super) fn import_error(
    row_number: i32,
    stable_id: Option<&str>,
    field_name: &str,
    code: &str,
    detail: &str,
) -> ProductImportError {
    ProductImportError {
        row_number,
        stable_id: stable_id.map(str::to_owned),
        field_name: Some(field_name.into()),
        severity: "error".into(),
        code: code.into(),
        detail: detail.into(),
    }
}
