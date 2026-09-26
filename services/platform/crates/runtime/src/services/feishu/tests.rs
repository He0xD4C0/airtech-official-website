#[cfg(test)]
mod cases {
    use serde_json::json;

    use super::super::validate_staging_payload;

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
