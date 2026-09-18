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
            ("noise".into(), field("noise-id", "噪声")),
            ("testCondition".into(), field("condition-id", "测试条件")),
            ("status".into(), field("status-id", "当前状态")),
        ]),
        attachments: vec![],
    }
}

fn source(family: ProductFamily) -> FeishuSource {
    FeishuSource {
        wiki_token: "wiki".into(),
        table_id: "table".into(),
        name: "table".into(),
        family,
        application: None,
    }
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
fn noise_requires_a_test_condition_and_keeps_unit_and_provenance() {
    let without_condition = normalize_record(
        &source(ProductFamily::Axial),
        &mapping(),
        &record(Map::from_iter([
            ("型号".into(), json!("AX-1")),
            ("噪声".into(), json!(65)),
            ("报价".into(), json!("private")),
        ])),
    );
    assert!(without_condition.normalized_payload["specifications"]
        .as_array()
        .unwrap()
        .iter()
        .all(|spec| spec["key"] != "noise"));
    assert_eq!(without_condition.confidential_payload["报价"], "private");
    assert!(!without_condition
        .normalized_payload
        .to_string()
        .contains("private"));

    let with_condition = normalize_record(
        &source(ProductFamily::Axial),
        &mapping(),
        &record(Map::from_iter([
            ("型号".into(), json!("AX-1")),
            ("噪声".into(), json!(65)),
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
    assert_eq!(noise["sourceReference"], "feishu:table:record:noise-id");
}
