use airtek_domain::models::ValidationIssue;

pub(super) struct SpecificationDefinition {
    pub(super) key: &'static str,
    pub(super) label: &'static str,
    pub(super) unit_expected: bool,
    pub(super) condition_required: bool,
}

pub(super) fn specification_definitions() -> Vec<SpecificationDefinition> {
    vec![
        spec("voltage", "Rated voltage", true, false),
        spec("frequency", "Frequency", true, false),
        spec("speed", "Speed", true, true),
        spec("current", "Current", true, true),
        spec("power", "Power", true, true),
        spec("airflow", "Airflow", true, true),
        spec("pressure", "Pressure", true, true),
        spec("diameter", "Diameter", true, false),
        spec("material", "Material", false, false),
        spec("protection", "Protection class", false, false),
        spec("insulation", "Insulation class", false, false),
        spec("ambientTemperature", "Ambient temperature", true, false),
        spec("productDimensions", "Product dimensions", true, false),
        spec("packageDimensions", "Package dimensions", true, false),
        spec("noise", "Noise", true, true),
    ]
}

fn spec(
    key: &'static str,
    label: &'static str,
    unit_expected: bool,
    condition_required: bool,
) -> SpecificationDefinition {
    SpecificationDefinition {
        key,
        label,
        unit_expected,
        condition_required,
    }
}

pub(super) fn optional_warning(field: &str, code: &str, detail: String) -> ValidationIssue {
    ValidationIssue {
        field_path: format!("specifications.{field}"),
        code: code.into(),
        detail,
    }
}

pub(super) fn declared_unit(key: &str, field_name: &str) -> Option<&'static str> {
    let lower = field_name.to_ascii_lowercase();
    let compact = lower.replace(' ', "");
    match key {
        "voltage" if compact.contains("(v)") || compact.contains("（v）") => Some("V"),
        "frequency" if compact.contains("hz") || compact.contains("赫兹") => Some("Hz"),
        "speed"
            if compact.contains("rpm")
                || compact.contains("r/min")
                || compact.contains("转/分") =>
        {
            Some("rpm")
        }
        "current" if compact.contains("(a)") || compact.contains("（a）") => Some("A"),
        "power" if compact.contains("kw") => Some("kW"),
        "power" if compact.contains("(w)") || compact.contains("（w）") => Some("W"),
        "airflow"
            if compact.contains("m³/h")
                || compact.contains("m3/h")
                || compact.contains("m^3/h") =>
        {
            Some("m³/h")
        }
        "airflow" if compact.contains("cfm") => Some("cfm"),
        "pressure" if compact.contains("kpa") => Some("kPa"),
        "pressure" if compact.contains("pa") => Some("Pa"),
        "diameter" | "productDimensions" | "packageDimensions" if compact.contains("mm") => {
            Some("mm")
        }
        "diameter" | "productDimensions" | "packageDimensions" if compact.contains("cm") => {
            Some("cm")
        }
        "ambientTemperature"
            if compact.contains('℃') || compact.contains("°c") || compact.contains("celsius") =>
        {
            Some("°C")
        }
        "noise" if compact.contains("db(a)") || compact.contains("dba") => Some("dB(A)"),
        _ => None,
    }
}
