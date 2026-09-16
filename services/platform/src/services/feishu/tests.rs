#[cfg(test)]
mod cases {
    use std::collections::BTreeSet;

    use serde_json::json;

    use super::super::{conflicting_diffs, three_way_diff, validate_staging_payload};

    #[test]
    pub(super) fn detects_source_owned_three_way_conflict() {
        let base = json!({"model": "A", "site": {"seoTitle": "Old"}});
        let local = json!({"model": "LOCAL", "site": {"seoTitle": "New"}});
        let incoming = json!({"model": "FEISHU", "site": {"seoTitle": "Old"}});
        let owned = BTreeSet::from(["model".to_owned()]);
        let diffs = three_way_diff(&base, &local, &incoming, &owned);
        let conflicts = conflicting_diffs(&diffs);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].field_path, "model");
    }

    #[test]
    pub(super) fn verified_values_require_product_master_provenance() {
        let issues = validate_staging_payload(&json!({
            "stableId": "source-record-id",
            "sourceRevision": "revision-id",
            "family": "axial",
            "specifications": [{"key": "airflow", "state": "verified", "value": 1}]
        }));
        assert!(issues
            .iter()
            .any(|issue| issue.code == "provenanceRequired"));
    }

    #[test]
    pub(super) fn validates_verified_pq_units_provenance_and_monotonic_points() {
        let issues = validate_staging_payload(&json!({
            "stableId": "AT_VALIDATED-001",
            "model": "VALIDATED-MODEL",
            "sourceRevision": "revision-id",
            "family": "axial",
            "performanceCurves": [{
                "airflowUnit": "litres/minute",
                "pressureUnit": "Pa",
                "state": "verified",
                "sourceReference": "",
                "densityKgM3": -1,
                "points": [
                    {"airflow": 100, "pressure": 200},
                    {"airflow": 100, "pressure": 180}
                ]
            }]
        }));
        for code in [
            "unsupportedUnit",
            "provenanceRequired",
            "positiveNumberRequired",
            "nonIncreasingAirflow",
        ] {
            assert!(issues.iter().any(|issue| issue.code == code), "{code}");
        }
    }

    #[test]
    pub(super) fn accepts_a_traceable_normalized_verified_product_payload() {
        let issues = validate_staging_payload(&json!({
            "stableId": "AT_VALIDATED-001",
            "model": "VALIDATED-MODEL",
            "sourceRevision": "revision-id",
            "family": "centrifugal",
            "specifications": [{
                "key": "ratedVoltage",
                "label": "Rated voltage",
                "value": 230,
                "unit": "V",
                "state": "verified",
                "sourceReference": "feishu:record:field"
            }],
            "performanceCurves": [{
                "airflowUnit": "m3/h",
                "pressureUnit": "Pa",
                "state": "verified",
                "sourceReference": "feishu:curve:1",
                "testMethod": "Controlled Product Master test",
                "speedRpm": 1200,
                "densityKgM3": 1.2,
                "points": [
                    {"airflow": 0, "pressure": 300},
                    {"airflow": 100, "pressure": 200}
                ]
            }]
        }));
        assert!(issues.is_empty(), "{issues:#?}");
    }

    #[test]
    pub(super) fn rejects_duplicate_specification_keys_and_unverified_values_without_state() {
        let issues = validate_staging_payload(&json!({
            "stableId": "AT_VALIDATED-001",
            "sourceRevision": "revision-id",
            "family": "motors",
            "specifications": [
                {"key": "power", "label": "Power", "state": "missing"},
                {"key": "power", "label": "Power", "value": 1}
            ]
        }));
        assert!(issues.iter().any(|issue| issue.code == "duplicateKey"));
        assert!(issues.iter().any(|issue| issue.code == "factStateRequired"));
    }
}
