use std::collections::BTreeMap;

use serde_json::{json, Map, Value};

use super::*;

fn field(id: &str, name: &str) -> MappedFeishuField {
    MappedFeishuField {
        id: id.into(),
        name: name.into(),
        field_type: 1,
    }
}

fn mapping() -> FeishuTableMapping {
    FeishuTableMapping {
        fields: BTreeMap::from([
            ("model".into(), field("model-id", "型号")),
            ("noise".into(), field("noise-id", "噪声 dB(A)")),
            ("testCondition".into(), field("condition-id", "测试条件")),
            ("status".into(), field("status-id", "当前状态")),
        ]),
        attachments: vec![],
        private_fields: vec![field("price-id", "报价")],
    }
}

fn source_with_application(family: ProductFamily, application: Option<&str>) -> FeishuSource {
    FeishuSource {
        enabled: true,
        wiki_token: "wiki".into(),
        table_id: "table".into(),
        name: "table".into(),
        family,
        application: application.map(str::to_owned),
    }
}

fn source(family: ProductFamily) -> FeishuSource {
    source_with_application(family, None)
}

fn record(fields: Map<String, Value>) -> FeishuRecord {
    FeishuRecord {
        record_id: "record".into(),
        fields,
        created_time: None,
        last_modified_time: Some("1".into()),
    }
}

#[test]
fn only_cross_flow_uses_the_explicit_lifecycle_field() {
    let fields = Map::from_iter([
        ("型号".into(), json!("FAN-1")),
        ("当前状态".into(), json!("停产")),
    ]);
    let axial = normalize_record(
        &source(ProductFamily::Axial),
        &mapping(),
        &record(fields.clone()),
    );
    assert!(!axial.archived);
    assert!(axial.issues.is_empty());

    let cross_flow = normalize_record(
        &source(ProductFamily::CrossFlow),
        &mapping(),
        &record(fields),
    );
    assert!(cross_flow.archived);
    assert!(cross_flow.issues.is_empty());
}

#[test]
fn cross_flow_checkmark_is_an_explicit_active_status() {
    let normalized = normalize_record(
        &source(ProductFamily::CrossFlow),
        &mapping(),
        &record(Map::from_iter([
            ("型号".into(), json!("CF-ACTIVE")),
            ("当前状态".into(), json!("✅")),
        ])),
    );
    assert!(!normalized.archived);
    assert!(normalized.issues.is_empty());
}

#[test]
fn noise_requires_a_test_condition_and_keeps_unit_and_provenance() {
    let without_condition = normalize_record(
        &source(ProductFamily::Axial),
        &mapping(),
        &record(Map::from_iter([
            ("型号".into(), json!("AX-1")),
            ("噪声 dB(A)".into(), json!(65)),
            ("报价".into(), json!("private")),
        ])),
    );
    assert!(without_condition.normalized_payload["specifications"]
        .as_array()
        .unwrap()
        .iter()
        .all(|spec| spec["key"] != "noise"));
    assert_eq!(
        without_condition.confidential_payload["fieldsById"]["price-id"]["value"],
        "private"
    );
    assert!(!without_condition
        .normalized_payload
        .to_string()
        .contains("private"));

    let with_condition = normalize_record(
        &source(ProductFamily::Axial),
        &mapping(),
        &record(Map::from_iter([
            ("型号".into(), json!("AX-1")),
            ("噪声 dB(A)".into(), json!(65)),
            ("测试条件".into(), json!("1 m at rated voltage")),
        ])),
    );
    let noise = with_condition.normalized_payload["specifications"]
        .as_array()
        .unwrap()
        .iter()
        .find(|spec| spec["key"] == "noise")
        .unwrap();
    assert_eq!(noise["unit"], "dB(A)");
    assert_eq!(noise["operatingCondition"], "1 m at rated voltage");
    assert_eq!(
        noise["sourceReference"],
        format!("feishu:{}:table:record:noise-id", &token_hash("wiki")[..16])
    );
}

#[test]
fn agriculture_records_remain_axial_and_keep_application_classification() {
    let normalized = normalize_record(
        &source_with_application(ProductFamily::Axial, Some("agriculture-livestock")),
        &mapping(),
        &record(Map::from_iter([("型号".into(), json!("AG-1"))])),
    );
    assert_eq!(normalized.normalized_payload["family"], "axial");
    assert_eq!(
        normalized.normalized_payload["applicationCategories"][0],
        "agriculture-livestock"
    );
}

#[test]
fn same_table_and_record_in_different_wikis_have_distinct_source_ids() {
    let mut first_source = source(ProductFamily::Axial);
    let mut second_source = first_source.clone();
    first_source.wiki_token = "wiki-a".into();
    second_source.wiki_token = "wiki-b".into();
    let source_record = record(Map::from_iter([("型号".into(), json!("AX-1"))]));
    let first = normalize_record(&first_source, &mapping(), &source_record);
    let second = normalize_record(&second_source, &mapping(), &source_record);
    assert_ne!(first.source_record_id, second.source_record_id);
    assert_ne!(first.source_revision, second.source_revision);
}

#[test]
fn unknown_cross_flow_lifecycle_status_is_a_validation_error() {
    let normalized = normalize_record(
        &source(ProductFamily::CrossFlow),
        &mapping(),
        &record(Map::from_iter([
            ("型号".into(), json!("CF-1")),
            ("当前状态".into(), json!("maybe")),
        ])),
    );
    assert!(normalized
        .issues
        .iter()
        .any(|issue| issue.code == "unknownLifecycleStatus"));
}

#[test]
fn pricing_checksum_is_private_and_does_not_change_public_facts() {
    let first = normalize_record(
        &source(ProductFamily::Axial),
        &mapping(),
        &record(Map::from_iter([
            ("型号".into(), json!("AX-1")),
            ("报价".into(), json!("100")),
        ])),
    );
    let second = normalize_record(
        &source(ProductFamily::Axial),
        &mapping(),
        &record(Map::from_iter([
            ("型号".into(), json!("AX-1")),
            ("报价".into(), json!("125")),
        ])),
    );
    assert_eq!(first.source_checksum, second.source_checksum);
    assert_ne!(first.confidential_checksum, second.confidential_checksum);
    assert!(!first.normalized_payload.to_string().contains("100"));
}

#[test]
fn generic_numeric_field_does_not_invent_a_unit_or_condition() {
    let mut mapping = mapping();
    mapping
        .fields
        .insert("power".into(), field("power-id", "功率"));
    let normalized = normalize_record(
        &source(ProductFamily::Axial),
        &mapping,
        &record(Map::from_iter([
            ("型号".into(), json!("AX-1")),
            ("功率".into(), json!(120)),
        ])),
    );
    let power = normalized.normalized_payload["specifications"]
        .as_array()
        .unwrap()
        .iter()
        .find(|spec| spec["key"] == "power")
        .unwrap();
    assert!(power["unit"].is_null());
    assert!(power["operatingCondition"].is_null());
    assert_eq!(normalized.warnings.len(), 2);
}
