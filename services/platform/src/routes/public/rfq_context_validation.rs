fn parse_rfq_context<T: for<'de> Deserialize<'de>>(
    value: Value,
    journey: &str,
    errors: &mut BTreeMap<String, Vec<String>>,
) -> Option<T> {
    match serde_json::from_value(value) {
        Ok(context) => Some(context),
        Err(_) => {
            errors.entry("context".into()).or_default().push(format!(
                "Context does not match the typed {journey} RFQ contract."
            ));
            None
        }
    }
}

fn reject_product_context(request: &CreateRfqRequest, errors: &mut BTreeMap<String, Vec<String>>) {
    if request.product_context.is_some() {
        errors.insert(
            "productContext".into(),
            vec!["Product context is accepted only for a Product RFQ.".into()],
        );
    }
}

fn validate_product_context(
    context: &crate::models::ProductContext,
    errors: &mut BTreeMap<String, Vec<String>>,
) {
    validate_required_text("productContext.stableId", &context.stable_id, 200, errors);
    match context.model.as_deref() {
        Some(model) => validate_required_text("productContext.model", model, 200, errors),
        None => {
            errors.insert(
                "productContext.model".into(),
                vec!["Product RFQ requires the published model identifier.".into()],
            );
        }
    }
    if context.published_revision <= 0 {
        errors.insert(
            "productContext.publishedRevision".into(),
            vec!["Published revision must be a positive integer.".into()],
        );
    }
}

fn validate_common_rfq_context(
    application: &str,
    quantity: Option<&RfqQuantity>,
    electrical: Option<&RfqElectricalContext>,
    environment: Option<&str>,
    priority: Option<&str>,
    additional_message: Option<&str>,
    errors: &mut BTreeMap<String, Vec<String>>,
) {
    validate_required_text("context.application", application, 500, errors);
    if let Some(quantity) = quantity {
        let valid = match quantity {
            RfqQuantity::Integer(value) => (1..=1_000_000).contains(value),
            RfqQuantity::Text(value) => {
                !value.is_empty()
                    && value.len() <= 7
                    && value.bytes().all(|byte| byte.is_ascii_digit())
                    && value
                        .parse::<u64>()
                        .is_ok_and(|value| (1..=1_000_000).contains(&value))
            }
        };
        if !valid {
            errors.insert(
                "context.quantity".into(),
                vec!["Quantity must be a whole number from 1 to 1,000,000.".into()],
            );
        }
    }
    if let Some(electrical) = electrical {
        if electrical.voltage.is_none() && electrical.frequency_hz.is_none() {
            errors.insert(
                "context.electrical".into(),
                vec!["Electrical context must contain voltage or frequencyHz.".into()],
            );
        }
        validate_optional_text(
            "context.electrical.voltage",
            electrical.voltage.as_deref(),
            40,
            errors,
        );
        validate_optional_finite_positive(
            "context.electrical.frequencyHz",
            electrical.frequency_hz,
            1_000.0,
            errors,
        );
    }
    validate_optional_text("context.environment", environment, 4_000, errors);
    if priority.is_some_and(|value| !["efficiency", "noise", "size", "headroom"].contains(&value)) {
        errors.insert(
            "context.priority".into(),
            vec!["Priority is not supported.".into()],
        );
    }
    validate_optional_text(
        "context.additionalMessage",
        additional_message,
        10_000,
        errors,
    );
}

fn validate_duty_point(duty_point: &RfqDutyPoint, errors: &mut BTreeMap<String, Vec<String>>) {
    validate_optional_finite_positive(
        "context.dutyPoint.airflow",
        Some(duty_point.airflow),
        1_000_000_000.0,
        errors,
    );
    validate_optional_finite_positive(
        "context.dutyPoint.pressure",
        Some(duty_point.pressure),
        100_000_000.0,
        errors,
    );
    if !["m3/h", "m³/h", "cfm"].contains(&duty_point.airflow_unit.to_lowercase().as_str()) {
        errors.insert(
            "context.dutyPoint.airflowUnit".into(),
            vec!["Unsupported airflow unit.".into()],
        );
    }
    if !["pa", "kpa", "inh2o"].contains(&duty_point.pressure_unit.to_lowercase().as_str()) {
        errors.insert(
            "context.dutyPoint.pressureUnit".into(),
            vec!["Unsupported pressure unit.".into()],
        );
    }
}

fn validate_optional_finite_positive(
    field: &str,
    value: Option<f64>,
    maximum: f64,
    errors: &mut BTreeMap<String, Vec<String>>,
) {
    if value.is_some_and(|value| !value.is_finite() || value <= 0.0 || value > maximum) {
        errors.insert(
            field.into(),
            vec![format!(
                "Must be a positive finite value no greater than {maximum}."
            )],
        );
    }
}

fn validate_string_list(
    field: &str,
    values: &[String],
    maximum_items: usize,
    maximum_length: usize,
    errors: &mut BTreeMap<String, Vec<String>>,
) {
    let valid = values.len() <= maximum_items
        && values.iter().all(|value| {
            let value = value.trim();
            !value.is_empty()
                && value.chars().count() <= maximum_length
                && !value.chars().any(char::is_control)
        });
    if !valid {
        errors.insert(
            field.into(),
            vec!["The list contains too many or invalid values.".into()],
        );
    }
}

fn contains_attachment_field(value: &Value) -> bool {
    const ATTACHMENT_KEYS: &[&str] = &[
        "attachment",
        "attachments",
        "attachmentname",
        "attachmenturl",
        "file",
        "files",
        "filename",
        "filenames",
        "fileurl",
        "upload",
        "uploads",
        "uploadid",
    ];
    match value {
        Value::Object(map) => map.iter().any(|(key, value)| {
            let normalized = key
                .chars()
                .filter(|character| character.is_ascii_alphanumeric())
                .flat_map(char::to_lowercase)
                .collect::<String>();
            ATTACHMENT_KEYS.contains(&normalized.as_str()) || contains_attachment_field(value)
        }),
        Value::Array(values) => values.iter().any(contains_attachment_field),
        _ => false,
    }
}
