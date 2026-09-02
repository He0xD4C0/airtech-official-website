use std::cmp::Ordering;

use crate::models::{
    CurvePoint, FactState, Product, PublicationStatus, SelectorCandidate, SelectorOutcome,
    SelectorPriority, SelectorRequest, SelectorResponse,
};

struct DutyMatch<'a> {
    product: &'a Product,
    available_pressure: f64,
    curve_index: usize,
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
                && request
                    .motor_technology
                    .as_ref()
                    .map(|technology| product.motor_technology.as_ref() == Some(technology))
                    .unwrap_or(true)
        })
        .collect();

    if eligible.is_empty() {
        return no_candidates("No published product record is available for the requested family.");
    }

    let mut matches = Vec::new();
    for product in eligible {
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
            });
        }
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
        request
            .maximum_diameter_mm
            .is_some()
            .then_some("maximum diameter"),
        request.frequency_hz.is_some().then_some("frequency"),
        (!request.required_certifications.is_empty()).then_some("certification"),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    if !unevaluated_constraints.is_empty() {
        return SelectorResponse {
            outcome: SelectorOutcome::EngineeringReviewRequired,
            candidates: vec![],
            explanations: vec![format!(
                "Verified curves cover the duty point, but the current published selector contract cannot safely evaluate: {}.",
                unevaluated_constraints.join(", ")
            )],
        };
    }

    let prefer_headroom = request.priority == Some(SelectorPriority::Headroom);
    matches.sort_by(|left, right| {
        let left_margin = left.available_pressure - request.pressure;
        let right_margin = right.available_pressure - request.pressure;
        if prefer_headroom {
            right_margin
                .partial_cmp(&left_margin)
                .unwrap_or(Ordering::Equal)
        } else {
            left_margin
                .partial_cmp(&right_margin)
                .unwrap_or(Ordering::Equal)
        }
        .then_with(|| left.product.stable_id.cmp(&right.product.stable_id))
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
            let mut warnings = Vec::new();
            if request.priority.is_some() && !prefer_headroom {
                warnings.push(
                    "Rank uses verified duty-point pressure margin because the requested efficiency, noise or size metric is not normalized in the selector contract."
                        .into(),
                );
            }
            if curve.test_method.as_deref().unwrap_or_default().is_empty() {
                warnings.push("The published curve has no test-method label; confirm conditions during engineering review.".into());
            }
            SelectorCandidate {
                product_id: matched.product.id,
                product_revision: matched
                    .product
                    .published_revision
                    .expect("eligible published product has a revision"),
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
mod tests {
    use chrono::Utc;
    use uuid::Uuid;

    use super::*;
    use crate::models::{PerformanceCurve, ProductFamily, PublicationStatus, SelectorRequest};

    fn request() -> SelectorRequest {
        SelectorRequest {
            airflow: 100.0,
            airflow_unit: "m3/h".into(),
            pressure: 200.0,
            pressure_unit: "Pa".into(),
            ambient_temperature_c: None,
            maximum_diameter_mm: None,
            voltage: None,
            frequency_hz: None,
            required_certifications: vec![],
            preferred_family: Some(ProductFamily::Axial),
            motor_technology: None,
            priority: Some(SelectorPriority::Efficiency),
        }
    }

    fn product(state: FactState, points: Vec<CurvePoint>) -> Product {
        Product {
            id: Uuid::new_v4(),
            stable_id: "AT-VERIFIED-001".into(),
            model: Some("Verified model".into()),
            slug: "verified-model".into(),
            locale: "en".into(),
            family: ProductFamily::Axial,
            subtype: None,
            motor_technology: None,
            title: "Published verified product".into(),
            summary: None,
            seo: Default::default(),
            sort_order: 0,
            related_content_ids: Vec::new(),
            specifications: vec![],
            performance_curves: vec![PerformanceCurve {
                airflow_unit: "m3/h".into(),
                pressure_unit: "Pa".into(),
                speed_rpm: None,
                density_kg_m3: None,
                voltage: None,
                test_method: Some("Controlled test".into()),
                source_reference: "verified-source".into(),
                state,
                points,
            }],
            source_snapshot_id: Uuid::new_v4(),
            source_revision: "source-1".into(),
            current_revision: 1,
            published_revision: Some(1),
            status: PublicationStatus::Published,
            indexable: true,
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn interpolates_only_verified_published_curves() {
        let response = evaluate(
            &request(),
            &[product(
                FactState::Verified,
                vec![
                    CurvePoint {
                        airflow: 0.0,
                        pressure: 300.0,
                    },
                    CurvePoint {
                        airflow: 200.0,
                        pressure: 180.0,
                    },
                ],
            )],
        );
        assert_eq!(response.outcome, SelectorOutcome::Matched);
        assert_eq!(response.candidates.len(), 1);
        assert_eq!(response.candidates[0].rank, 1);
    }

    #[test]
    fn returns_no_candidate_when_the_verified_curve_misses_the_point() {
        let response = evaluate(
            &request(),
            &[product(
                FactState::Verified,
                vec![
                    CurvePoint {
                        airflow: 0.0,
                        pressure: 150.0,
                    },
                    CurvePoint {
                        airflow: 200.0,
                        pressure: 50.0,
                    },
                ],
            )],
        );
        assert_eq!(response.outcome, SelectorOutcome::NoValidatedCandidates);
        assert!(response.candidates.is_empty());
    }

    #[test]
    fn fails_to_engineering_review_for_unmodeled_hard_constraints() {
        let mut constrained = request();
        constrained.required_certifications = vec!["Owner-approved requirement".into()];
        let response = evaluate(
            &constrained,
            &[product(
                FactState::Verified,
                vec![
                    CurvePoint {
                        airflow: 0.0,
                        pressure: 300.0,
                    },
                    CurvePoint {
                        airflow: 200.0,
                        pressure: 180.0,
                    },
                ],
            )],
        );
        assert_eq!(response.outcome, SelectorOutcome::EngineeringReviewRequired);
        assert!(response.candidates.is_empty());
    }

    #[test]
    fn does_not_use_pending_curves() {
        let response = evaluate(
            &request(),
            &[product(
                FactState::PendingVerification,
                vec![
                    CurvePoint {
                        airflow: 0.0,
                        pressure: 300.0,
                    },
                    CurvePoint {
                        airflow: 200.0,
                        pressure: 180.0,
                    },
                ],
            )],
        );
        assert_eq!(response.outcome, SelectorOutcome::NoValidatedCandidates);
    }

    #[test]
    fn interpolation_rejects_duplicate_or_out_of_range_segments() {
        assert_eq!(
            pressure_at(
                &[
                    CurvePoint {
                        airflow: 10.0,
                        pressure: 20.0,
                    },
                    CurvePoint {
                        airflow: 10.0,
                        pressure: 30.0,
                    },
                ],
                10.0
            ),
            None
        );
        assert_eq!(
            pressure_at(
                &[
                    CurvePoint {
                        airflow: 0.0,
                        pressure: 300.0,
                    },
                    CurvePoint {
                        airflow: 200.0,
                        pressure: 100.0,
                    },
                ],
                100.0
            ),
            Some(200.0)
        );
    }
}
