fn validate_selector(request: &SelectorRequest) -> Result<(), ApiError> {
    let mut errors = BTreeMap::new();
    if !request.airflow.is_finite() || request.airflow <= 0.0 {
        errors.insert(
            "airflow".into(),
            vec!["Must be a positive finite value.".into()],
        );
    }
    if !request.pressure.is_finite() || request.pressure <= 0.0 {
        errors.insert(
            "pressure".into(),
            vec!["Must be a positive finite value.".into()],
        );
    }
    if !["m3/h", "m³/h", "cfm"].contains(&request.airflow_unit.to_lowercase().as_str()) {
        errors.insert(
            "airflowUnit".into(),
            vec!["Unsupported airflow unit.".into()],
        );
    }
    if !["pa", "kpa", "inh2o"].contains(&request.pressure_unit.to_lowercase().as_str()) {
        errors.insert(
            "pressureUnit".into(),
            vec!["Unsupported pressure unit.".into()],
        );
    }
    if request.motor_technology.as_ref().is_some_and(|value| {
        value.trim().is_empty()
            || value.len() > 80
            || value.chars().any(|character| character.is_control())
    }) {
        errors.insert(
            "motorTechnology".into(),
            vec!["Must contain 1 to 80 printable characters.".into()],
        );
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ApiError::validation(errors))
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RfqQuantity {
    Integer(u64),
    Text(String),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RfqDutyPoint {
    airflow: f64,
    airflow_unit: String,
    pressure: f64,
    pressure_unit: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RfqElectricalContext {
    voltage: Option<String>,
    frequency_hz: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProductRfqContext {
    application: String,
    quantity: Option<RfqQuantity>,
    electrical: Option<RfqElectricalContext>,
    environment: Option<String>,
    priority: Option<String>,
    additional_message: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SelectionRfqContext {
    application: String,
    duty_point: RfqDutyPoint,
    quantity: Option<RfqQuantity>,
    electrical: Option<RfqElectricalContext>,
    environment: Option<String>,
    priority: Option<String>,
    additional_message: Option<String>,
    maximum_diameter_mm: Option<f64>,
    #[serde(default)]
    required_certifications: Vec<String>,
    control: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectRfqContext {
    application: String,
    project_stage: String,
    quantity: Option<RfqQuantity>,
    electrical: Option<RfqElectricalContext>,
    environment: Option<String>,
    priority: Option<String>,
    additional_message: Option<String>,
    project_scale: Option<String>,
    schedule: Option<String>,
    engineering_needs: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReplacementRfqContext {
    application: String,
    existing_model: String,
    duty_point: RfqDutyPoint,
    quantity: Option<RfqQuantity>,
    electrical: Option<RfqElectricalContext>,
    environment: Option<String>,
    priority: Option<String>,
    additional_message: Option<String>,
    installation_constraints: Option<String>,
    replacement_goal: Option<String>,
}

fn validate_rfq(request: &CreateRfqRequest) -> Result<(), ApiError> {
    let mut errors = validate_business_contact(&request.contact);
    if !request.consent {
        errors.insert("consent".into(), vec!["Consent is required.".into()]);
    }
    validate_public_source_and_locale(
        &request.source_path,
        &request.locale,
        "/en/request-a-quote/",
        &mut errors,
    );

    let context_value = Value::Object(request.context.clone().into_iter().collect());
    if contains_attachment_field(&context_value) {
        errors.insert(
            "context".into(),
            vec!["Public RFQ attachments and file metadata are not accepted.".into()],
        );
    }

    match request.journey {
        RfqJourney::Product => {
            let product_context = match request.product_context.as_ref() {
                Some(context) => Some(context),
                None => {
                    errors.insert(
                        "productContext".into(),
                        vec!["Product RFQ requires an immutable published product context.".into()],
                    );
                    None
                }
            };
            if let Some(context) = product_context {
                validate_product_context(context, &mut errors);
            }
            if let Some(context) =
                parse_rfq_context::<ProductRfqContext>(context_value, "product", &mut errors)
            {
                validate_common_rfq_context(
                    &context.application,
                    context.quantity.as_ref(),
                    context.electrical.as_ref(),
                    context.environment.as_deref(),
                    context.priority.as_deref(),
                    context.additional_message.as_deref(),
                    &mut errors,
                );
            }
        }
        RfqJourney::Selection => {
            reject_product_context(request, &mut errors);
            if let Some(context) =
                parse_rfq_context::<SelectionRfqContext>(context_value, "selection", &mut errors)
            {
                validate_common_rfq_context(
                    &context.application,
                    context.quantity.as_ref(),
                    context.electrical.as_ref(),
                    context.environment.as_deref(),
                    context.priority.as_deref(),
                    context.additional_message.as_deref(),
                    &mut errors,
                );
                validate_duty_point(&context.duty_point, &mut errors);
                validate_optional_finite_positive(
                    "context.maximumDiameterMm",
                    context.maximum_diameter_mm,
                    100_000.0,
                    &mut errors,
                );
                validate_string_list(
                    "context.requiredCertifications",
                    &context.required_certifications,
                    20,
                    120,
                    &mut errors,
                );
                validate_optional_text(
                    "context.control",
                    context.control.as_deref(),
                    120,
                    &mut errors,
                );
            }
        }
        RfqJourney::Project => {
            reject_product_context(request, &mut errors);
            if let Some(context) =
                parse_rfq_context::<ProjectRfqContext>(context_value, "project", &mut errors)
            {
                validate_common_rfq_context(
                    &context.application,
                    context.quantity.as_ref(),
                    context.electrical.as_ref(),
                    context.environment.as_deref(),
                    context.priority.as_deref(),
                    context.additional_message.as_deref(),
                    &mut errors,
                );
                if !["Concept", "Engineering", "Prototype", "Production planning"]
                    .contains(&context.project_stage.as_str())
                {
                    errors.insert(
                        "context.projectStage".into(),
                        vec!["Project stage must be one of the supported values.".into()],
                    );
                }
                validate_optional_text(
                    "context.projectScale",
                    context.project_scale.as_deref(),
                    500,
                    &mut errors,
                );
                validate_optional_text(
                    "context.schedule",
                    context.schedule.as_deref(),
                    500,
                    &mut errors,
                );
                validate_optional_text(
                    "context.engineeringNeeds",
                    context.engineering_needs.as_deref(),
                    4_000,
                    &mut errors,
                );
            }
        }
        RfqJourney::Replacement => {
            reject_product_context(request, &mut errors);
            if let Some(context) = parse_rfq_context::<ReplacementRfqContext>(
                context_value,
                "replacement",
                &mut errors,
            ) {
                validate_common_rfq_context(
                    &context.application,
                    context.quantity.as_ref(),
                    context.electrical.as_ref(),
                    context.environment.as_deref(),
                    context.priority.as_deref(),
                    context.additional_message.as_deref(),
                    &mut errors,
                );
                validate_required_text(
                    "context.existingModel",
                    &context.existing_model,
                    300,
                    &mut errors,
                );
                validate_duty_point(&context.duty_point, &mut errors);
                validate_optional_text(
                    "context.installationConstraints",
                    context.installation_constraints.as_deref(),
                    4_000,
                    &mut errors,
                );
                validate_optional_text(
                    "context.replacementGoal",
                    context.replacement_goal.as_deref(),
                    2_000,
                    &mut errors,
                );
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(ApiError::validation(errors))
    }
}
