use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;
use airtek_domain::models::FeishuSource;

use super::{FeishuClient, FeishuField};

#[path = "mapping_definitions.rs"]
mod mapping_definitions;
use mapping_definitions::{definitions, Definition};
#[path = "mapping_archived.rs"]
mod mapping_archived;
use mapping_archived::discover_archived_table;

#[derive(Clone, Debug)]
pub struct DiscoveredSource {
    pub source: FeishuSource,
    pub app_token: String,
    pub fields: Vec<FeishuField>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionedFeishuMapping {
    pub version: String,
    pub tables: BTreeMap<String, FeishuTableMapping>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeishuTableMapping {
    pub fields: BTreeMap<String, MappedFeishuField>,
    #[serde(default)]
    pub source_fields: Vec<MappedFeishuField>,
    pub attachments: Vec<MappedFeishuField>,
    #[serde(default)]
    pub private_fields: Vec<MappedFeishuField>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MappedFeishuField {
    pub id: String,
    pub name: String,
    pub field_type: i32,
}

pub async fn discover_sources(
    client: &FeishuClient,
    sources: &[FeishuSource],
) -> Result<Vec<DiscoveredSource>, ApiError> {
    let mut discovered = Vec::with_capacity(sources.len());
    for source in sources {
        let app_token = client.resolve_app_token(&source.wiki_token).await?;
        let fields = client.list_fields(&app_token, &source.table_id).await?;
        discovered.push(DiscoveredSource {
            source: source.clone(),
            app_token,
            fields,
        });
    }
    Ok(discovered)
}

pub async fn ensure_mapping(
    state: &AppState,
    connector_id: Uuid,
    version: &str,
    sources: &[DiscoveredSource],
) -> Result<VersionedFeishuMapping, ApiError> {
    let existing = sqlx::query(
        r#"SELECT id,mapping FROM sync_mappings
           WHERE connector_id=$1 AND version=$2"#,
    )
    .bind(connector_id)
    .bind(version)
    .fetch_optional(&state.pool)
    .await?;
    if let Some(row) = existing {
        let stored: VersionedFeishuMapping = serde_json::from_value(row.try_get("mapping")?)
            .map_err(|error| {
                tracing::error!(%error, "stored Feishu mapping is invalid");
                ApiError::service_unavailable("The stored Feishu mapping is invalid.")
            })?;
        return validate_mapping(stored, sources);
    }

    let mapping = discover_mapping(version, sources)?;
    let payload = serde_json::to_value(&mapping)
        .map_err(|_| ApiError::internal("Feishu mapping serialization failed."))?;
    let mut transaction = state.pool.begin().await?;
    sqlx::query("UPDATE sync_mappings SET active=false WHERE connector_id=$1 AND active=true")
        .bind(connector_id)
        .execute(&mut *transaction)
        .await?;
    sqlx::query(
        r#"INSERT INTO sync_mappings
           (id,connector_id,version,mapping,schema_version,active,created_at)
           VALUES ($1,$2,$3,$4,1,true,now())
           ON CONFLICT (connector_id,version) DO NOTHING"#,
    )
    .bind(Uuid::new_v4())
    .bind(connector_id)
    .bind(version)
    .bind(payload)
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(mapping)
}

pub async fn load_mapping(
    state: &AppState,
    connector_id: Uuid,
    version: &str,
) -> Result<Option<VersionedFeishuMapping>, ApiError> {
    let value = sqlx::query_scalar::<_, serde_json::Value>(
        "SELECT mapping FROM sync_mappings WHERE connector_id=$1 AND version=$2",
    )
    .bind(connector_id)
    .bind(version)
    .fetch_optional(&state.pool)
    .await?;
    value
        .map(|value| {
            serde_json::from_value(value).map_err(|error| {
                tracing::error!(%error, "stored Feishu mapping is invalid");
                ApiError::service_unavailable("The stored Feishu mapping is invalid.")
            })
        })
        .transpose()
}

pub fn discover_mapping(
    version: &str,
    sources: &[DiscoveredSource],
) -> Result<VersionedFeishuMapping, ApiError> {
    let mut tables = BTreeMap::new();
    for source in sources {
        let table = discover_archived_table(&source.source.table_id, &source.fields)?
            .map(Ok)
            .unwrap_or_else(|| discover_table(&source.fields))?;
        tables.insert(source_identity_key(&source.source), table);
    }
    Ok(VersionedFeishuMapping {
        version: version.to_owned(),
        tables,
    })
}

pub fn validate_mapping(
    mut mapping: VersionedFeishuMapping,
    sources: &[DiscoveredSource],
) -> Result<VersionedFeishuMapping, ApiError> {
    for source in sources {
        let identity_key = source_identity_key(&source.source);
        let mapping_key = if mapping.tables.contains_key(&identity_key) {
            identity_key
        } else {
            source.source.table_id.clone()
        };
        let stored = mapping
            .tables
            .get_mut(&mapping_key)
            .ok_or_else(|| drift_error(&source.source.table_id, "the mapped table is missing"))?;
        let by_id = source
            .fields
            .iter()
            .map(|field| (field.field_id.as_str(), field))
            .collect::<HashMap<_, _>>();
        for field in stored
            .fields
            .values_mut()
            .chain(stored.attachments.iter_mut())
            .chain(stored.private_fields.iter_mut())
        {
            let current = by_id.get(field.id.as_str()).ok_or_else(|| {
                drift_error(
                    &source.source.table_id,
                    &format!("field {} was deleted or its id changed", field.id),
                )
            })?;
            if current.field_type != field.field_type {
                return Err(drift_error(
                    &source.source.table_id,
                    &format!("field {} changed type", field.id),
                ));
            }
            // Display names are descriptive only. The stable id remains the
            // authority, while the runtime name follows harmless renames.
            field.name.clone_from(&current.field_name);
        }
    }
    Ok(mapping)
}

pub fn source_identity_key(source: &FeishuSource) -> String {
    serde_json::to_string(&(source.wiki_token.trim(), source.table_id.trim()))
        .expect("Feishu source identity is serializable")
}

fn discover_table(fields: &[FeishuField]) -> Result<FeishuTableMapping, ApiError> {
    let primary = fields.iter().find(|field| field.is_primary);
    let definitions = definitions();
    let mut candidates = HashMap::<&'static str, (usize, &FeishuField)>::new();
    for field in fields.iter().filter(|field| field.field_type != 17) {
        let mut best = Vec::<(&Definition, usize)>::new();
        for definition in &definitions {
            let Some(score) = definition.match_score(&field.field_name) else {
                continue;
            };
            match best.first().map(|(_, best_score)| *best_score) {
                None => best.push((definition, score)),
                Some(best_score) if score > best_score => {
                    best.clear();
                    best.push((definition, score));
                }
                Some(best_score) if score == best_score => best.push((definition, score)),
                _ => {}
            }
        }
        if best.len() > 1 {
            return Err(ApiError::conflict(format!(
                "Feishu field `{}` ambiguously matches multiple product facts.",
                field.field_name
            )));
        }
        if let Some((definition, score)) = best.pop() {
            match candidates.get(definition.key) {
                Some((current, other)) if score == *current && other.field_id != field.field_id => {
                    return Err(ApiError::conflict(format!(
                        "Feishu product fact `{}` ambiguously matches fields `{}` and `{}`.",
                        definition.key, other.field_name, field.field_name
                    )));
                }
                Some((current, _)) if score <= *current => {}
                _ => {
                    candidates.insert(definition.key, (score, field));
                }
            }
        }
    }
    let mut mapped = BTreeMap::new();
    for definition in definitions {
        let found = candidates
            .get(definition.key)
            .map(|(_, field)| *field)
            .or_else(|| (definition.key == "model").then_some(primary).flatten());
        if let Some(field) = found {
            mapped.insert(definition.key.into(), mapped_field(field));
        } else if definition.required {
            return Err(ApiError::conflict(format!(
                "Feishu field discovery could not map required field `{}`.",
                definition.key
            )));
        }
    }
    if let Some(field) = primary {
        mapped
            .entry("stableId".into())
            .or_insert_with(|| mapped_field(field));
    }
    let attachments = fields
        .iter()
        .filter(|field| field.field_type == 17)
        .map(mapped_field)
        .collect();
    let private_fields = fields
        .iter()
        .filter(|field| field.field_type != 17 && is_pricing_field(&field.field_name))
        .map(mapped_field)
        .collect();
    Ok(FeishuTableMapping {
        fields: mapped,
        source_fields: fields.iter().map(mapped_field).collect(),
        attachments,
        private_fields,
    })
}

fn is_pricing_field(name: &str) -> bool {
    let normalized = normalized_name(name);
    matches!(
        normalized.as_str(),
        "样品报价sample"
            | "样品报价"
            | "报价"
            | "单价"
            | "199pcs"
            | "100499pcs"
            | "500999pcs"
            | "10004999pcs"
            | "5000pcs"
            | "100500pcs"
            | "5001000pcs"
            | "10005000pcs"
            | "样品13pcssample"
            | "批5000pcs"
            | "批量100500件"
            | "批量5001000件"
            | "批量10005000件"
            | "批量5000"
    )
}

fn mapped_field(field: &FeishuField) -> MappedFeishuField {
    MappedFeishuField {
        id: field.field_id.clone(),
        name: field.field_name.clone(),
        field_type: field.field_type,
    }
}

fn normalized_name(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn drift_error(table_id: &str, detail: &str) -> ApiError {
    ApiError::conflict(format!(
        "Feishu schema drift for table `{table_id}`: {detail}. Create a reviewed mapping version before resuming this table."
    ))
}

#[cfg(test)]
#[path = "mapping_tests.rs"]
mod tests;
