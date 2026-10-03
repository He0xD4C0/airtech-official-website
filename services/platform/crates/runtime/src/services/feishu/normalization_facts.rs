use serde_json::Value;

use airtek_domain::models::{FactState, SourceFact};

use super::super::{FeishuRecord, FeishuTableMapping, MappedFeishuField};
use super::value_text;

pub(super) fn source_facts(
    mapping: &FeishuTableMapping,
    record: &FeishuRecord,
    source_reference: &impl Fn(&MappedFeishuField) -> String,
) -> Vec<Value> {
    let private_ids = mapping
        .private_fields
        .iter()
        .map(|field| field.id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let primary_id = mapping
        .fields
        .get("stableId")
        .map(|field| field.id.as_str());
    mapping
        .source_fields
        .iter()
        .filter(|field| field.field_type != 17)
        .filter(|field| Some(field.id.as_str()) != primary_id)
        .filter(|field| !private_ids.contains(field.id.as_str()))
        .filter(|field| !is_management_only_field(&field.name))
        .filter(|field| {
            !matches!(
                field.name.as_str(),
                "产品型号(Model）" | "产品型号(Model)" | "AIRTEK型号" | "产品型号" | "型号"
            )
        })
        .filter_map(|field| {
            let raw = record.fields.get(&field.name)?;
            let raw_value = value_text(raw)?;
            serde_json::to_value(SourceFact {
                field_name: field.name.clone(),
                raw_value,
                source_reference: source_reference(field),
                state: FactState::Verified,
                unit: None,
                operating_condition: None,
            })
            .ok()
        })
        .collect()
}

fn is_management_only_field(name: &str) -> bool {
    let normalized = name
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    matches!(
        normalized.as_str(),
        "供应商型号" | "供应商名称" | "品牌名称" | "品牌属性"
    )
}
