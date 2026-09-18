use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::models::{FeishuSource, ProductFamily, ValidationIssue};

use super::{validate_staging_payload, FeishuRecord, FeishuTableMapping, MappedFeishuField};

#[derive(Clone, Debug)]
pub struct SourceAttachment {
    pub file_token: String,
    pub original_name: String,
    pub declared_size: Option<u64>,
    pub declared_media_type: Option<String>,
    pub source_field_id: String,
    pub source_field_name: String,
    pub usage: String,
    pub source_revision: String,
}

#[derive(Clone, Debug)]
pub struct NormalizedFeishuRecord {
    pub source_record_id: String,
    pub source_revision: String,
    pub source_checksum: String,
    pub snapshot_payload: Value,
    pub confidential_payload: Value,
    pub normalized_payload: Value,
    pub attachments: Vec<SourceAttachment>,
    pub archived: bool,
    pub issues: Vec<ValidationIssue>,
}

pub fn normalize_record(
    source: &FeishuSource,
    mapping: &FeishuTableMapping,
    record: &FeishuRecord,
) -> NormalizedFeishuRecord {
    let source_revision = record
        .last_modified_time
        .as_deref()
        .or(record.created_time.as_deref())
        .map(|value| format!("{}:{value}", source.table_id))
        .unwrap_or_else(|| format!("{}:unversioned", source.table_id));
    let stable_id = format!("fs.{}.{}", source.table_id, record.record_id);
    let source_record_id = stable_id.clone();
    let model = mapped_text(mapping, &record.fields, "model").unwrap_or_default();
    let subtype = mapped_text(mapping, &record.fields, "category");
    let test_condition = mapped_text(mapping, &record.fields, "testCondition");
    let lifecycle = if source.family == ProductFamily::CrossFlow {
        lifecycle(mapping, &record.fields)
    } else {
        Ok(false)
    };
    let archived = lifecycle.as_ref().copied().unwrap_or(false);
    let slug = slug_for(&model, &stable_id);
    let source_reference = |field: &MappedFeishuField| {
        format!(
            "feishu:{}:{}:{}",
            source.table_id, record.record_id, field.id
        )
    };
    let specifications = specification_definitions()
        .into_iter()
        .filter_map(|definition| {
            let field = mapping.fields.get(definition.key)?;
            let value = record.fields.get(&field.name).and_then(public_value)?;
            if definition.key == "noise" && test_condition.is_none() {
                return None;
            }
            let operating_condition = definition.condition_required.then(|| {
                test_condition.clone().unwrap_or_else(|| {
                    format!(
                        "Source-stated rating in Feishu field `{}`; no separate test condition was supplied.",
                        field.name
                    )
                })
            });
            Some(json!({
                "key": definition.key,
                "label": definition.label,
                "value": value,
                "unit": definition.unit,
                "operatingCondition": operating_condition,
                "state": "verified",
                "sourceReference": source_reference(field)
            }))
        })
        .collect::<Vec<_>>();
    let attachments = extract_attachments(mapping, record, &source_revision);
    let family = family_label(source.family);
    let mut payload = json!({
        "stableId": stable_id,
        "model": model,
        "slug": slug,
        "locale": "en",
        "family": family,
        "subtype": subtype,
        "motorTechnology": null,
        "title": model,
        "summary": null,
        "seo": {
            "title": null,
            "description": null,
            "canonicalPath": canonical_path(source.family, &slug),
            "indexable": true
        },
        "sortOrder": 0,
        "relatedContentIds": [],
        "mediaGallery": [],
        "specifications": specifications,
        "performanceCurves": [],
        "sourceRevision": source_revision,
        "applicationCategories": source.application.iter().cloned().collect::<Vec<_>>(),
        "sourceRecordId": record.record_id
    });
    let mut issues = validate_staging_payload(&payload);
    if let Err(issue) = lifecycle {
        issues.push(issue);
    }
    let mapped_snapshot = mapping
        .fields
        .iter()
        .filter(|(key, _)| !matches!(key.as_str(), "noise"))
        .filter_map(|(key, field)| {
            record
                .fields
                .get(&field.name)
                .cloned()
                .map(|value| (key.clone(), value))
        })
        .collect::<Map<_, _>>();
    let snapshot_payload = json!({
        "recordId": record.record_id,
        "tableId": source.table_id,
        "sourceRevision": source_revision,
        "mappedFields": mapped_snapshot,
        "attachments": attachments.iter().map(|attachment| json!({
            "tokenSha256": token_hash(&attachment.file_token),
            "name": attachment.original_name,
            "fieldId": attachment.source_field_id,
            "fieldName": attachment.source_field_name,
            "usage": attachment.usage
        })).collect::<Vec<_>>()
    });
    let confidential_payload = Value::Object(record.fields.clone());
    let source_checksum = checksum(&confidential_payload);
    if let Some(object) = payload.as_object_mut() {
        object.insert("sourceChecksum".into(), json!(source_checksum));
    }
    NormalizedFeishuRecord {
        source_record_id,
        source_revision,
        source_checksum,
        snapshot_payload,
        confidential_payload,
        normalized_payload: payload,
        attachments,
        archived,
        issues,
    }
}

fn mapped_text(
    mapping: &FeishuTableMapping,
    fields: &Map<String, Value>,
    key: &str,
) -> Option<String> {
    let field = mapping.fields.get(key)?;
    fields.get(&field.name).and_then(value_text)
}

fn lifecycle(
    mapping: &FeishuTableMapping,
    fields: &Map<String, Value>,
) -> Result<bool, ValidationIssue> {
    let Some(value) = mapped_text(mapping, fields, "status") else {
        return Ok(false);
    };
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty()
        || ["active", "启用", "在产", "正常", "有效", "上架", "销售中"]
            .contains(&normalized.as_str())
    {
        Ok(false)
    } else if ["inactive", "停产", "下架", "归档", "禁用"].contains(&normalized.as_str()) {
        Ok(true)
    } else {
        Err(ValidationIssue {
            field_path: "status".into(),
            code: "unknownLifecycleStatus".into(),
            detail: format!("Unsupported Feishu lifecycle status `{value}`."),
        })
    }
}

struct SpecificationDefinition {
    key: &'static str,
    label: &'static str,
    unit: Option<&'static str>,
    condition_required: bool,
}

fn specification_definitions() -> Vec<SpecificationDefinition> {
    vec![
        spec("voltage", "Rated voltage", Some("V"), false),
        spec("frequency", "Frequency", Some("Hz"), false),
        spec("speed", "Speed", Some("rpm"), true),
        spec("current", "Current", Some("A"), true),
        spec("power", "Input power", Some("W"), true),
        spec("airflow", "Airflow", Some("m3/h"), true),
        spec("pressure", "Pressure", Some("Pa"), true),
        spec("diameter", "Diameter", Some("mm"), false),
        spec("material", "Material", None, false),
        spec("protection", "Protection class", None, false),
        spec("insulation", "Insulation class", None, false),
        spec(
            "ambientTemperature",
            "Ambient temperature",
            Some("°C"),
            false,
        ),
        spec("productDimensions", "Product dimensions", Some("mm"), false),
        spec("packageDimensions", "Package dimensions", Some("mm"), false),
        spec("noise", "Noise", Some("dB(A)"), true),
    ]
}

fn spec(
    key: &'static str,
    label: &'static str,
    unit: Option<&'static str>,
    condition_required: bool,
) -> SpecificationDefinition {
    SpecificationDefinition {
        key,
        label,
        unit,
        condition_required,
    }
}

fn extract_attachments(
    mapping: &FeishuTableMapping,
    record: &FeishuRecord,
    source_revision: &str,
) -> Vec<SourceAttachment> {
    let mut output = Vec::new();
    for field in &mapping.attachments {
        let Some(items) = record.fields.get(&field.name).and_then(Value::as_array) else {
            continue;
        };
        for item in items {
            let Some(object) = item.as_object() else {
                continue;
            };
            let token = text_property(object, &["file_token", "fileToken", "token"]);
            let Some(file_token) = token else { continue };
            let original_name = text_property(object, &["name", "file_name", "fileName"])
                .unwrap_or_else(|| "feishu-attachment".into());
            output.push(SourceAttachment {
                file_token,
                original_name,
                declared_size: object.get("size").and_then(Value::as_u64),
                declared_media_type: text_property(object, &["type", "mime_type", "mimeType"]),
                source_field_id: field.id.clone(),
                source_field_name: field.name.clone(),
                usage: attachment_usage(&field.name).into(),
                source_revision: source_revision.into(),
            });
        }
    }
    output
}

fn attachment_usage(name: &str) -> &'static str {
    let lower = name.to_ascii_lowercase();
    if name.contains("曲线") || lower.contains("pq") {
        "curve"
    } else if name.contains("图纸") || lower.contains("drawing") || lower.contains("2d") {
        "drawing"
    } else if lower.contains("cad")
        || lower.contains("3d")
        || lower.contains("step")
        || lower.contains("dwg")
    {
        "cad"
    } else if name.contains("规格") || name.contains("说明书") || lower.contains("datasheet") {
        "datasheet"
    } else {
        "technicalDocument"
    }
}

fn public_value(value: &Value) -> Option<Value> {
    match value {
        Value::Null => None,
        Value::String(text) => (!text.trim().is_empty()).then(|| json!(text.trim())),
        Value::Number(_) | Value::Bool(_) => Some(value.clone()),
        _ => value_text(value).map(Value::String),
    }
}

fn value_text(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.trim().to_owned()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Array(values) => {
            let joined = values
                .iter()
                .filter_map(value_text)
                .collect::<Vec<_>>()
                .join(", ");
            (!joined.is_empty()).then_some(joined)
        }
        Value::Object(object) => text_property(object, &["text", "name", "value"]),
        Value::Null => None,
    }
    .filter(|value| !value.trim().is_empty())
}

fn text_property(object: &Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| object.get(*key).and_then(value_text))
}

fn slug_for(model: &str, stable_id: &str) -> String {
    let base = model
        .chars()
        .flat_map(char::to_lowercase)
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let suffix = &checksum(&json!(stable_id))[..8];
    format!(
        "{}-{suffix}",
        if base.is_empty() { "product" } else { &base }
    )
}

fn canonical_path(family: ProductFamily, slug: &str) -> String {
    let family = match family {
        ProductFamily::Centrifugal => "centrifugal",
        ProductFamily::Axial => "axial",
        ProductFamily::CrossFlow => "cross-flow",
        ProductFamily::InlineDuct => "inline-duct",
        ProductFamily::Motors => "motors",
    };
    format!("/en/products/{family}/{slug}")
}

fn family_label(family: ProductFamily) -> &'static str {
    match family {
        ProductFamily::Centrifugal => "centrifugal",
        ProductFamily::Axial => "axial",
        ProductFamily::CrossFlow => "crossFlow",
        ProductFamily::InlineDuct => "inlineDuct",
        ProductFamily::Motors => "motors",
    }
}

pub fn token_hash(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

fn checksum(value: &Value) -> String {
    let bytes = serde_json::to_vec(value).unwrap_or_default();
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn source(family: ProductFamily, application: Option<&str>) -> FeishuSource {
        FeishuSource {
            wiki_token: "wiki".into(),
            table_id: "tbl".into(),
            name: "table".into(),
            family,
            application: application.map(str::to_owned),
        }
    }

    fn mapping() -> FeishuTableMapping {
        FeishuTableMapping {
            fields: BTreeMap::from([
                (
                    "model".into(),
                    MappedFeishuField {
                        id: "model-id".into(),
                        name: "型号".into(),
                        field_type: 1,
                    },
                ),
                (
                    "status".into(),
                    MappedFeishuField {
                        id: "status-id".into(),
                        name: "当前状态".into(),
                        field_type: 3,
                    },
                ),
            ]),
            attachments: vec![],
        }
    }

    #[test]
    fn agriculture_records_remain_axial_and_keep_application_classification() {
        let record = FeishuRecord {
            record_id: "rec1".into(),
            fields: Map::from_iter([("型号".into(), json!("AG-1"))]),
            created_time: None,
            last_modified_time: Some("1".into()),
        };
        let normalized = normalize_record(
            &source(ProductFamily::Axial, Some("agriculture-livestock")),
            &mapping(),
            &record,
        );
        assert_eq!(normalized.normalized_payload["family"], "axial");
        assert_eq!(
            normalized.normalized_payload["applicationCategories"][0],
            "agriculture-livestock"
        );
    }

    #[test]
    fn unknown_lifecycle_status_is_a_validation_error() {
        let record = FeishuRecord {
            record_id: "rec1".into(),
            fields: Map::from_iter([
                ("型号".into(), json!("CF-1")),
                ("当前状态".into(), json!("maybe")),
            ]),
            created_time: None,
            last_modified_time: Some("1".into()),
        };
        let normalized =
            normalize_record(&source(ProductFamily::CrossFlow, None), &mapping(), &record);
        assert!(normalized
            .issues
            .iter()
            .any(|issue| issue.code == "unknownLifecycleStatus"));
    }
}

#[cfg(test)]
#[path = "normalization_more_tests.rs"]
mod more_tests;
