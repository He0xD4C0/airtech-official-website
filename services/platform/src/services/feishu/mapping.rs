use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use crate::{error::ApiError, models::FeishuSource, state::AppState};

use super::{FeishuClient, FeishuField};

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
    pub attachments: Vec<MappedFeishuField>,
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
        let table = discover_table(&source.fields)?;
        tables.insert(source.source.table_id.clone(), table);
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
        let stored = mapping
            .tables
            .get_mut(&source.source.table_id)
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

fn discover_table(fields: &[FeishuField]) -> Result<FeishuTableMapping, ApiError> {
    let primary = fields.iter().find(|field| field.is_primary);
    let definitions = definitions();
    let mut candidates = HashMap::<&'static str, (usize, &FeishuField)>::new();
    for field in fields.iter().filter(|field| field.field_type != 17) {
        let mut best: Option<(&Definition, usize)> = None;
        for definition in &definitions {
            let Some(score) = definition.match_score(&field.field_name) else {
                continue;
            };
            if best.is_none_or(|(_, best_score)| score > best_score) {
                best = Some((definition, score));
            }
        }
        if let Some((definition, score)) = best {
            let replace = candidates
                .get(definition.key)
                .is_none_or(|(current, _)| score > *current);
            if replace {
                candidates.insert(definition.key, (score, field));
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
    Ok(FeishuTableMapping {
        fields: mapped,
        attachments,
    })
}

fn mapped_field(field: &FeishuField) -> MappedFeishuField {
    MappedFeishuField {
        id: field.field_id.clone(),
        name: field.field_name.clone(),
        field_type: field.field_type,
    }
}

struct Definition {
    key: &'static str,
    required: bool,
    aliases: &'static [&'static str],
}

impl Definition {
    fn match_score(&self, candidate: &str) -> Option<usize> {
        let candidate = normalized_name(candidate);
        self.aliases
            .iter()
            .map(|alias| normalized_name(alias))
            .filter_map(|alias| {
                let length = alias.chars().count();
                if candidate == alias {
                    Some(10_000 + length)
                } else if length >= 3 && candidate.contains(&alias) {
                    Some(length)
                } else {
                    None
                }
            })
            .max()
    }
}

fn definitions() -> Vec<Definition> {
    vec![
        Definition {
            key: "model",
            required: true,
            aliases: &[
                "产品型号(Model）",
                "产品型号(Model)",
                "AIRTEK型号",
                "产品型号",
                "型号",
            ],
        },
        Definition {
            key: "category",
            required: false,
            aliases: &[
                "产品分类（大类）",
                "产品分类",
                "产品类别",
                "类别",
                "分类",
                "系列",
            ],
        },
        Definition {
            key: "voltage",
            required: false,
            aliases: &["额定电压", "电压(V)", "电压"],
        },
        Definition {
            key: "frequency",
            required: false,
            aliases: &["额定频率", "频率(Hz)", "频率"],
        },
        Definition {
            key: "speed",
            required: false,
            aliases: &["转速(rpm)", "转速", "额定转速"],
        },
        Definition {
            key: "current",
            required: false,
            aliases: &["电流(A)", "电流", "额定电流"],
        },
        Definition {
            key: "power",
            required: false,
            aliases: &["功率(W)", "输入功率", "额定功率", "功率"],
        },
        Definition {
            key: "airflow",
            required: false,
            aliases: &["风量(m³/h)", "风量(m3/h)", "最大风量", "风量"],
        },
        Definition {
            key: "pressure",
            required: false,
            aliases: &["风压(Pa)", "静压", "最大风压", "风压"],
        },
        Definition {
            key: "diameter",
            required: false,
            aliases: &["直径(mm)", "叶轮直径", "直径"],
        },
        Definition {
            key: "material",
            required: false,
            aliases: &["材质", "材料"],
        },
        Definition {
            key: "protection",
            required: false,
            aliases: &["防护等级", "IP等级", "防护"],
        },
        Definition {
            key: "insulation",
            required: false,
            aliases: &["绝缘等级", "绝缘"],
        },
        Definition {
            key: "ambientTemperature",
            required: false,
            aliases: &["环境温度", "工作温度", "使用温度"],
        },
        Definition {
            key: "productDimensions",
            required: false,
            aliases: &["产品尺寸", "外形尺寸", "尺寸"],
        },
        Definition {
            key: "packageDimensions",
            required: false,
            aliases: &["包装尺寸", "包装规格"],
        },
        Definition {
            key: "status",
            required: false,
            aliases: &["当前状态", "产品状态", "状态"],
        },
        Definition {
            key: "noise",
            required: false,
            aliases: &["噪声", "噪音", "声压级"],
        },
        Definition {
            key: "testCondition",
            required: false,
            aliases: &["测试条件", "测试工况", "工况"],
        },
    ]
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
mod tests {
    use super::*;

    #[test]
    fn maps_model_alias_and_every_attachment_by_stable_field_id() {
        let fields = vec![
            FeishuField {
                field_id: "fld-model".into(),
                field_name: "产品型号(Model）".into(),
                field_type: 1,
                is_primary: true,
            },
            FeishuField {
                field_id: "fld-file".into(),
                field_name: "PQ曲线源文件".into(),
                field_type: 17,
                is_primary: false,
            },
        ];
        let table = discover_table(&fields).unwrap();
        assert_eq!(table.fields["model"].id, "fld-model");
        assert_eq!(table.attachments[0].id, "fld-file");
    }

    #[test]
    fn type_change_is_schema_drift_even_when_the_display_name_is_unchanged() {
        let mapping = VersionedFeishuMapping {
            version: "v1".into(),
            tables: BTreeMap::from([(
                "tbl".into(),
                FeishuTableMapping {
                    fields: BTreeMap::from([(
                        "model".into(),
                        MappedFeishuField {
                            id: "fld".into(),
                            name: "型号".into(),
                            field_type: 1,
                        },
                    )]),
                    attachments: vec![],
                },
            )]),
        };
        let sources = vec![DiscoveredSource {
            source: FeishuSource {
                wiki_token: "wiki".into(),
                table_id: "tbl".into(),
                name: "Table".into(),
                family: crate::models::ProductFamily::Axial,
                application: None,
            },
            app_token: "app".into(),
            fields: vec![FeishuField {
                field_id: "fld".into(),
                field_name: "型号".into(),
                field_type: 2,
                is_primary: true,
            }],
        }];
        assert!(validate_mapping(mapping, &sources).is_err());
    }

    #[test]
    fn assigns_each_source_field_to_its_most_specific_fact() {
        let fields = vec![
            FeishuField {
                field_id: "fld-model".into(),
                field_name: "型号".into(),
                field_type: 1,
                is_primary: true,
            },
            FeishuField {
                field_id: "fld-package".into(),
                field_name: "包装尺寸(mm)".into(),
                field_type: 1,
                is_primary: false,
            },
            FeishuField {
                field_id: "fld-drawing".into(),
                field_name: "产品尺寸图纸".into(),
                field_type: 17,
                is_primary: false,
            },
        ];
        let table = discover_table(&fields).unwrap();
        assert_eq!(table.fields["packageDimensions"].id, "fld-package");
        assert!(!table.fields.contains_key("productDimensions"));
        assert_eq!(table.attachments[0].id, "fld-drawing");
    }
}
