use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::models::{FeishuSource, ProductFamily, ValidationIssue};

use super::{validate_staging_payload, FeishuRecord, FeishuTableMapping, MappedFeishuField};

#[path = "normalization_units.rs"]
mod normalization_units;
use normalization_units::{declared_unit, optional_warning, specification_definitions};

#[derive(Clone, Debug)]
pub struct SourceAttachment {
    pub file_token: String,
    pub table_id: String,
    pub source_record_id: String,
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
    pub wiki_token: String,
    pub table_id: String,
    pub record_id: String,
    pub source_record_id: String,
    pub source_revision: String,
    /// Checksum of public facts and source-attachment identities only.
    pub source_checksum: String,
    /// Checksum of the encrypted private projection, including mapped pricing.
    pub confidential_checksum: String,
    pub snapshot_payload: Value,
    pub confidential_payload: Value,
    pub normalized_payload: Value,
    pub attachments: Vec<SourceAttachment>,
    pub archived: bool,
    pub issues: Vec<ValidationIssue>,
    pub warnings: Vec<ValidationIssue>,
}

pub fn normalize_record(
    source: &FeishuSource,
    mapping: &FeishuTableMapping,
    record: &FeishuRecord,
) -> NormalizedFeishuRecord {
    let source_scope = &token_hash(&source.wiki_token)[..16];
    let source_revision = record
        .last_modified_time
        .as_deref()
        .or(record.created_time.as_deref())
        .map(|value| format!("{source_scope}:{}:{value}", source.table_id))
        .unwrap_or_else(|| format!("{source_scope}:{}:unversioned", source.table_id));
    let stable_id = format!(
        "fs.{}",
        checksum(&json!([
            source.wiki_token,
            source.table_id,
            record.record_id
        ]))
    );
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
            "feishu:{source_scope}:{}:{}:{}",
            source.table_id, record.record_id, field.id
        )
    };
    let mut warnings = Vec::new();
    let mut specifications = Vec::new();
    for definition in specification_definitions() {
        let Some(field) = mapping.fields.get(definition.key) else {
            continue;
        };
        let Some(value) = record.fields.get(&field.name).and_then(public_value) else {
            continue;
        };
        let unit = declared_unit(definition.key, &field.name);
        if definition.unit_expected && unit.is_none() {
            warnings.push(optional_warning(
                definition.key,
                "unitNotDeclared",
                format!(
                    "`{}` does not explicitly declare a supported unit.",
                    field.name
                ),
            ));
        }
        if definition.condition_required && test_condition.is_none() {
            warnings.push(optional_warning(
                definition.key,
                "operatingConditionMissing",
                format!(
                    "`{}` has no explicit operating or test condition.",
                    field.name
                ),
            ));
            if definition.key == "noise" {
                continue;
            }
        }
        specifications.push(json!({
            "key": definition.key,
            "label": definition.label,
            "value": value,
            "unit": unit,
            "operatingCondition": definition.condition_required.then(|| test_condition.clone()).flatten(),
            "state": "verified",
            "sourceReference": source_reference(field)
        }));
    }
    let attachments = extract_attachments(source, mapping, record, &source_revision);
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
        "wikiToken": source.wiki_token,
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
    let confidential_payload = private_projection(mapping, &record.fields);
    let mut confidential_checksum_payload = confidential_payload.clone();
    remove_volatile_attachment_urls(&mut confidential_checksum_payload);
    let confidential_checksum = checksum(&confidential_checksum_payload);
    let mut public_checksum_payload = payload.clone();
    public_checksum_payload
        .as_object_mut()
        .map(|object| object.remove("sourceRevision"));
    if let Some(object) = public_checksum_payload.as_object_mut() {
        object.insert(
            "sourceAttachments".into(),
            snapshot_payload["attachments"].clone(),
        );
    }
    let source_checksum = checksum(&public_checksum_payload);
    if let Some(object) = payload.as_object_mut() {
        object.insert("sourceChecksum".into(), json!(source_checksum));
    }
    NormalizedFeishuRecord {
        wiki_token: source.wiki_token.clone(),
        table_id: source.table_id.clone(),
        record_id: record.record_id.clone(),
        source_record_id,
        source_revision,
        source_checksum,
        confidential_checksum,
        snapshot_payload,
        confidential_payload,
        normalized_payload: payload,
        attachments,
        archived,
        issues,
        warnings,
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
        || [
            "active",
            "启用",
            "在产",
            "正常",
            "有效",
            "上架",
            "销售中",
            "✅",
        ]
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

fn private_projection(mapping: &FeishuTableMapping, fields: &Map<String, Value>) -> Value {
    let mut fields_by_id = Map::new();
    for field in mapping.fields.values().chain(mapping.private_fields.iter()) {
        if fields_by_id.contains_key(&field.id) {
            continue;
        }
        let Some(value) = fields.get(&field.name) else {
            continue;
        };
        fields_by_id.insert(
            field.id.clone(),
            json!({"name": field.name, "value": value}),
        );
    }
    json!({
        "schema": "feishu-private-v1",
        "fieldsById": fields_by_id,
        "pricingFieldIds": mapping.private_fields.iter().map(|field| field.id.clone()).collect::<Vec<_>>()
    })
}

fn extract_attachments(
    source: &FeishuSource,
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
                table_id: source.table_id.clone(),
                source_record_id: record.record_id.clone(),
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

fn remove_volatile_attachment_urls(value: &mut Value) {
    match value {
        Value::Array(values) => values.iter_mut().for_each(remove_volatile_attachment_urls),
        Value::Object(object) => {
            let attachment = object.contains_key("file_token")
                || object.contains_key("fileToken")
                || (object.contains_key("token") && object.contains_key("name"));
            if attachment {
                object.remove("url");
                object.remove("tmp_url");
                object.remove("tmpUrl");
            }
            object
                .values_mut()
                .for_each(remove_volatile_attachment_urls);
        }
        _ => {}
    }
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
#[path = "normalization_more_tests.rs"]
mod more_tests;
