use std::cmp::Ordering;

use crate::models::{
    CurvePoint, FactState, Product, PublicationStatus, SelectorCandidate, SelectorOutcome,
    SelectorPriority, SelectorRequest, SelectorResponse, SpecValue,
};

struct DutyMatch<'a> {
    product: &'a Product,
    available_pressure: f64,
    curve_index: usize,
    ranking_value: f64,
}

impl SelectorPriority {
    fn specification(self) -> Option<(&'static str, &'static str, &'static str)> {
        match self {
            Self::Efficiency => Some(("efficiency", "%", "efficiency")),
            Self::Noise => Some(("noise", "dB(A)", "noise")),
            Self::Size => Some(("diameter", "mm", "diameter")),
            Self::Headroom => None,
        }
    }

    fn prefers_higher(self) -> bool {
        matches!(self, Self::Efficiency | Self::Headroom)
    }
}

pub fn evaluate(request: &SelectorRequest, products: &[Product]) -> SelectorResponse {
    let eligible: Vec<_> = products
        .iter()
        .filter(|product| {
            product.status == PublicationStatus::Published
                && product.published_revision.is_some()
                && request
                    .preferred_family
                    .map(|family| family == product.family)
                    .unwrap_or(true)
        })
        .collect();

    if eligible.is_empty() {
        return no_candidates("No published product record is available for the requested family.");
    }

    let priority = request.priority.unwrap_or(SelectorPriority::Headroom);
    let mut matches = Vec::new();
    let mut incomplete_evidence = false;
    for product in eligible {
        if request.motor_technology.as_ref().is_some_and(|required| {
            product
                .motor_technology
                .as_ref()
                .is_some_and(|actual| actual != required)
        }) {
            continue;
        }
        if let Some(maximum) = request.maximum_diameter_mm {
            match verified_numeric_spec(&product.specifications, "diameter", "mm") {
                Some(diameter) if diameter > maximum => continue,
                Some(_) => {}
                None => {
                    incomplete_evidence = true;
                    continue;
                }
            }
        }
        if request.motor_technology.is_some() && product.motor_technology.is_none() {
            incomplete_evidence = true;
            continue;
        }
        let usable_curve = product.performance_curves.iter().any(|curve| {
            curve.state == FactState::Verified
                && curve.airflow_unit == request.airflow_unit
                && curve.pressure_unit == request.pressure_unit
                && curve.points.len() >= 2
                && request.voltage.as_ref().is_none_or(|required| {
                    curve
                        .voltage
                        .as_ref()
                        .is_some_and(|actual| actual.trim().eq_ignore_ascii_case(required.trim()))
                })
        });
        if !usable_curve {
            incomplete_evidence = true;
            continue;
        }
        let best = product
            .performance_curves
            .iter()
            .enumerate()
            .filter(|(_, curve)| {
                curve.state == FactState::Verified
                    && curve.airflow_unit == request.airflow_unit
                    && curve.pressure_unit == request.pressure_unit
                    && request.voltage.as_ref().is_none_or(|required| {
                        curve.voltage.as_ref().is_some_and(|actual| {
                            actual.trim().eq_ignore_ascii_case(required.trim())
                        })
                    })
            })
            .filter_map(|(index, curve)| {
                pressure_at(&curve.points, request.airflow)
                    .filter(|pressure| *pressure >= request.pressure)
                    .map(|pressure| (index, pressure))
            })
            .max_by(|left, right| left.1.partial_cmp(&right.1).unwrap_or(Ordering::Equal));
        if let Some((curve_index, available_pressure)) = best {
            matches.push(DutyMatch {
                product,
                available_pressure,
                curve_index,
                ranking_value: 0.0,
            });
        }
    }

    if incomplete_evidence {
        return engineering_review("Published records lack verified curve conditions or dimensions; suitability cannot be determined automatically.".into());
    }
    if matches.is_empty() {
        return no_candidates(
            "No verified published curve covers the requested duty point and units.",
        );
    }

    let unevaluated_constraints = [
        request
            .ambient_temperature_c
            .is_some()
            .then_some("ambient temperature"),
        request.frequency_hz.is_some().then_some("frequency"),
        (!request.required_certifications.is_empty()).then_some("certification"),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    if !unevaluated_constraints.is_empty() {
        return engineering_review(format!(
                "Verified curves cover the duty point, but the current published selector contract cannot safely evaluate: {}.",
                unevaluated_constraints.join(", ")
            ));
    }

    if matches.iter().any(|matched| {
        matched.product.slug.trim().is_empty()
            || matched
                .product
                .seo
                .canonical_path
                .as_deref()
                .is_none_or(|path| !path.starts_with("/en/products/"))
    }) {
        return engineering_review(
            "A duty-point candidate is missing its canonical published product identity.".into(),
        );
    }

    for matched in &mut matches {
        matched.ranking_value = if let Some((key, unit, _)) = priority.specification() {
            match verified_numeric_spec(&matched.product.specifications, key, unit) {
                Some(value) => value,
                None => return engineering_review(format!("A candidate lacks verified {key} in {unit}; ranking requires engineering review.")),
            }
        } else {
            matched.available_pressure - request.pressure
        };
    }

    matches.sort_by(|left, right| {
        let order = if priority.prefers_higher() {
            right.ranking_value.partial_cmp(&left.ranking_value)
        } else {
            left.ranking_value.partial_cmp(&right.ranking_value)
        }
        .unwrap_or(Ordering::Equal);
        order.then_with(|| left.product.stable_id.cmp(&right.product.stable_id))
    });

    let candidates = matches
        .into_iter()
        .take(20)
        .enumerate()
        .map(|(index, matched)| {
            let curve = &matched.product.performance_curves[matched.curve_index];
            let mut matched_constraints = vec![format!(
                "A verified published curve covers {} {} at or above {} {}.",
                request.airflow,
                request.airflow_unit,
                request.pressure,
                request.pressure_unit
            )];
            if request.preferred_family.is_some() {
                matched_constraints.push("The published fan family matches the requested family.".into());
            }
            if request.motor_technology.is_some() {
                matched_constraints.push(
                    "The published motor technology matches the requested technology exactly."
                        .into(),
                );
            }
            if request.voltage.is_some() {
                matched_constraints.push("The curve voltage matches the requested voltage exactly.".into());
            }
            if request.maximum_diameter_mm.is_some() {
                matched_constraints.push(
                    "The verified diameter in mm is within the requested maximum.".into(),
                );
            }
            let ranking_explanation = match priority.specification() {
                Some((_, unit, label)) => format!(
                    "Ranked by verified {label} value {} {unit}.",
                    matched.ranking_value
                ),
                None => format!(
                    "Ranked by verified pressure headroom {} {} at the requested airflow.",
                    matched.available_pressure - request.pressure,
                    request.pressure_unit
                ),
            };
            matched_constraints.push(ranking_explanation);
            let mut warnings = Vec::new();
            if curve.test_method.as_deref().unwrap_or_default().is_empty() {
                warnings.push("The published curve has no test-method label; confirm conditions during engineering review.".into());
            }
            SelectorCandidate {
                product_id: matched.product.id,
                product_revision: matched
                    .product
                    .published_revision
                    .expect("eligible published product has a revision"),
                slug: matched.product.slug.clone(),
                family: matched.product.family,
                canonical_path: matched
                    .product
                    .seo
                    .canonical_path
                    .clone()
                    .expect("candidate canonical identity was validated"),
                title: matched.product.title.clone(),
                matched_constraints,
                warnings,
                rank: index + 1,
            }
        })
        .collect();

    SelectorResponse {
        outcome: SelectorOutcome::Matched,
        candidates,
        explanations: vec![
            "Candidates are based only on verified published PQ curves with exactly matching units."
                .into(),
            "Final suitability still depends on installation, controls, acoustics, certification and commercial review."
                .into(),
        ],
    }
}

fn verified_numeric_spec(specifications: &[SpecValue], key: &str, unit: &str) -> Option<f64> {
    specifications.iter().find_map(|specification| {
        if specification.key != key
            || specification.state != FactState::Verified
            || specification.unit.as_deref() != Some(unit)
        {
            return None;
        }
        let value = specification.value.as_ref()?;
        let number = match value {
            serde_json::Value::Number(value) => value.as_f64(),
            serde_json::Value::String(value) => {
                let trimmed = value.trim();
                (!trimmed.is_empty() && trimmed == value)
                    .then(|| trimmed.parse::<f64>().ok())
                    .flatten()
            }
            _ => None,
        }?;
        number.is_finite().then_some(number)
    })
}

fn engineering_review(reason: String) -> SelectorResponse {
    SelectorResponse {
        outcome: SelectorOutcome::EngineeringReviewRequired,
        candidates: vec![],
        explanations: vec![reason],
    }
}

fn no_candidates(reason: &str) -> SelectorResponse {
    SelectorResponse {
        outcome: SelectorOutcome::NoValidatedCandidates,
        candidates: vec![],
        explanations: vec![
            reason.into(),
            "Submit a Fan Selection RFQ for engineering review; no demo values or synthetic match scores are used."
                .into(),
        ],
    }
}

fn pressure_at(points: &[CurvePoint], airflow: f64) -> Option<f64> {
    if points.len() < 2 || !airflow.is_finite() {
        return None;
    }
    let mut points: Vec<_> = points
        .iter()
        .filter(|point| point.airflow.is_finite() && point.pressure.is_finite())
        .collect();
    points.sort_by(|left, right| {
        left.airflow
            .partial_cmp(&right.airflow)
            .unwrap_or(Ordering::Equal)
    });
    for pair in points.windows(2) {
        let left = pair[0];
        let right = pair[1];
        if airflow < left.airflow || airflow > right.airflow {
            continue;
        }
        let width = right.airflow - left.airflow;
        if width <= f64::EPSILON {
            continue;
        }
        let ratio = (airflow - left.airflow) / width;
        return Some(left.pressure + ratio * (right.pressure - left.pressure));
    }
    None
}

#[cfg(test)]
#[path = "selector_tests.rs"]
mod tests;
