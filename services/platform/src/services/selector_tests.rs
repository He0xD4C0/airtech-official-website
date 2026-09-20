use chrono::Utc;
use uuid::Uuid;

use super::*;
use crate::models::{
    PerformanceCurve, ProductFamily, PublicationStatus, SelectorRequest, SpecValue,
};

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
        priority: Some(SelectorPriority::Headroom),
    }
}

fn spec(key: &str, value: serde_json::Value, unit: &str) -> SpecValue {
    SpecValue {
        key: key.into(),
        label: key.into(),
        value: Some(value),
        unit: Some(unit.into()),
        operating_condition: None,
        state: FactState::Verified,
        source_reference: Some("verified-source".into()),
    }
}

fn ranked_product(stable_id: &str, efficiency: f64, noise: f64, diameter: f64) -> Product {
    let mut value = product(
        FactState::Verified,
        vec![
            CurvePoint {
                airflow: 0.0,
                pressure: 320.0,
            },
            CurvePoint {
                airflow: 200.0,
                pressure: 180.0,
            },
        ],
    );
    value.stable_id = stable_id.into();
    value.specifications = vec![
        spec("efficiency", serde_json::json!(efficiency), "%"),
        spec("noise", serde_json::json!(noise.to_string()), "dB(A)"),
        spec("diameter", serde_json::json!(diameter), "mm"),
    ];
    value
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
        seo: crate::models::SeoMetadata {
            canonical_path: Some("/en/products/axial/verified-model".into()),
            indexable: true,
            ..Default::default()
        },
        sort_order: 0,
        related_content_ids: Vec::new(),
        media_gallery: Vec::new(),
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
    assert_eq!(response.candidates[0].slug, "verified-model");
    assert_eq!(response.candidates[0].family, ProductFamily::Axial);
    assert_eq!(
        response.candidates[0].canonical_path,
        "/en/products/axial/verified-model"
    );
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

#[test]
fn ranks_each_supported_priority_and_uses_stable_id_for_ties() {
    let products = vec![
        ranked_product("B", 80.0, 40.0, 300.0),
        ranked_product("A", 90.0, 50.0, 200.0),
        ranked_product("C", 90.0, 30.0, 400.0),
    ];
    for (priority, expected) in [
        (SelectorPriority::Efficiency, ["A", "C", "B"]),
        (SelectorPriority::Noise, ["C", "B", "A"]),
        (SelectorPriority::Size, ["A", "B", "C"]),
        (SelectorPriority::Headroom, ["A", "B", "C"]),
    ] {
        let mut request = request();
        request.priority = Some(priority);
        let response = evaluate(&request, &products);
        let ids = response
            .candidates
            .iter()
            .map(|candidate| {
                products
                    .iter()
                    .find(|product| product.id == candidate.product_id)
                    .unwrap()
                    .stable_id
                    .as_str()
            })
            .collect::<Vec<_>>();
        assert_eq!(ids, expected);
    }
}

#[test]
fn missing_unverified_or_wrong_unit_priority_requires_review() {
    for specifications in [
        vec![],
        vec![SpecValue {
            state: FactState::PendingVerification,
            ..spec("noise", serde_json::json!(40), "dB(A)")
        }],
        vec![spec("noise", serde_json::json!(40), "dB")],
        vec![spec("noise", serde_json::json!("40 dB"), "dB(A)")],
    ] {
        let mut value = ranked_product("A", 80.0, 40.0, 300.0);
        value.specifications = specifications;
        let mut request = request();
        request.priority = Some(SelectorPriority::Noise);
        assert_eq!(
            evaluate(&request, &[value]).outcome,
            SelectorOutcome::EngineeringReviewRequired
        );
    }
}
