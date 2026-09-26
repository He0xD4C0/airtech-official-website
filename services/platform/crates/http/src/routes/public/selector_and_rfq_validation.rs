use super::*;
use airtek_domain::models::{
    ProductRfqContext, ProjectRfqContext, ReplacementRfqContext, SelectionRfqContext,
};
pub(super) use airtek_domain::models::{RfqDutyPoint, RfqElectricalContext, RfqQuantity};

pub(super) fn validate_selector(request: &SelectorRequest) -> Result<(), ApiError> {
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
            || value.len() > 120
            || value.chars().any(|character| character.is_control())
    }) {
        errors.insert(
            "motorTechnology".into(),
            vec!["Must contain 1 to 120 printable characters.".into()],
        );
    }
    if request
        .ambient_temperature_c
        .is_some_and(|value| !value.is_finite() || !(-100.0..=300.0).contains(&value))
    {
        errors.insert(
            "ambientTemperatureC".into(),
            vec!["Must be a finite value between -100 and 300 °C.".into()],
        );
    }
    validate_optional_finite_positive(
        "maximumDiameterMm",
        request.maximum_diameter_mm,
        100_000.0,
        &mut errors,
    );
    validate_optional_text("voltage", request.voltage.as_deref(), 120, &mut errors);
    validate_optional_finite_positive("frequencyHz", request.frequency_hz, 1_000.0, &mut errors);
    validate_string_list(
        "requiredCertifications",
        &request.required_certifications,
        20,
        120,
        &mut errors,
    );
    if errors.is_empty() {
        Ok(())
    } else {
        Err(ApiError::validation(errors))
    }
}

pub(super) fn validate_rfq(request: &CreateRfqRequest) -> Result<(), ApiError> {
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
                if context
                    .ambient_temperature_c
                    .is_some_and(|value| !value.is_finite() || !(-273.15..=1000.0).contains(&value))
                {
                    errors.insert(
                        "context.ambientTemperatureC".into(),
                        vec!["Temperature must be between -273.15 and 1000 °C.".into()],
                    );
                }
                validate_optional_text(
                    "context.motorTechnology",
                    context.motor_technology.as_deref(),
                    120,
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
