use std::collections::BTreeMap;

use crate::error::ApiError;

use super::{FeishuField, FeishuTableMapping, MappedFeishuField};

const PRICING_FIELD_IDS: &[&str] = &[
    "fldEdnmi7l",
    "fldKA3ze3L",
    "fldjXJ5MVU",
    "fld24z51k1",
    "fldD7yItvY",
];

const CENTRIFUGAL_FIELDS: &[(&str, &str)] = &[
    ("model", "fldjq4VB5w"),
    ("category", "fldhSvt3EJ"),
    ("primaryCategory", "fldpreKbHa"),
    ("voltage", "fldXCwyCB7"),
    ("frequency", "flde8oQBqK"),
    ("diameter", "fldGU3Hnx2"),
    ("current", "fldvziN31h"),
    ("speed", "fldz2khJh8"),
    ("airflow", "flds2Oot4x"),
    ("pressure", "fld69b7ihP"),
    ("power", "fldXSelRhk"),
    ("noise", "fld3Nrhkoz"),
    ("material", "fldvU1Ttf0"),
    ("protection", "flddNT0ESK"),
    ("insulation", "fldRKAMmq9"),
    ("ambientTemperature", "fldHi3eaGJ"),
    ("productDimensions", "fldGINWKpE"),
    ("packageDimensions", "fld0eQIRzc"),
    ("weight", "fldRUs0UjP"),
];

const AXIAL_FIELDS: &[(&str, &str)] = &[
    ("model", "fldjq4VB5w"),
    ("category", "fldhSvt3EJ"),
    ("primaryCategory", "fldpreKbHa"),
    ("voltage", "fldXCwyCB7"),
    ("frequency", "flde8oQBqK"),
    ("diameter", "fldfLW5xYR"),
    ("current", "fldgdOnjgd"),
    ("speed", "fldz2khJh8"),
    ("airflow", "flds2Oot4x"),
    ("pressure", "fld69b7ihP"),
    ("power", "fldXSelRhk"),
    ("noise", "fld3Nrhkoz"),
    ("material", "fldvU1Ttf0"),
    ("protection", "flddNT0ESK"),
    ("insulation", "fldRKAMmq9"),
    ("ambientTemperature", "fldHi3eaGJ"),
    ("productDimensions", "fldGINWKpE"),
    ("packageDimensions", "fld0eQIRzc"),
    ("weight", "fldFkx6x0Q"),
];

const AGRICULTURE_FIELDS: &[(&str, &str)] = &[
    ("model", "fldjq4VB5w"),
    ("category", "fldhSvt3EJ"),
    ("primaryCategory", "fldpreKbHa"),
    ("voltage", "fldXCwyCB7"),
    ("frequency", "flde8oQBqK"),
    ("diameter", "fldfLW5xYR"),
    ("speed", "fldz2khJh8"),
    ("airflow", "flds2Oot4x"),
    ("power", "fldXSelRhk"),
    ("noise", "fld3Nrhkoz"),
    ("protection", "flddNT0ESK"),
    ("insulation", "fldRKAMmq9"),
    ("ambientTemperature", "fldHi3eaGJ"),
    ("productDimensions", "fldGINWKpE"),
    ("packageDimensions", "fld0eQIRzc"),
    ("weight", "fldFkx6x0Q"),
];

const CROSS_FLOW_FIELDS: &[(&str, &str)] = &[
    ("model", "fldjAaBY2N"),
    ("category", "fldhSvt3EJ"),
    ("primaryCategory", "fldpreKbHa"),
    ("voltage", "fldXCwyCB7"),
    ("current", "fldNng05qD"),
    ("speed", "fldz2khJh8"),
    ("airflow", "flds2Oot4x"),
    ("pressure", "fld69b7ihP"),
    ("power", "fldXSelRhk"),
    ("noise", "fld3Nrhkoz"),
    ("material", "fldvU1Ttf0"),
    ("protection", "flddNT0ESK"),
    ("insulation", "fldRKAMmq9"),
    ("ambientTemperature", "fldHi3eaGJ"),
    ("diameter", "fldbMRrA1O"),
    ("impellerLength", "fldkxsyMge"),
    ("dimensionA", "fldua3cSCb"),
    ("dimensionB", "fldi78S812"),
    ("dimensionC", "fldF3GTEEh"),
    ("status", "fldPTZM4uB"),
];

pub(super) fn discover_archived_table(
    table_id: &str,
    fields: &[FeishuField],
) -> Result<Option<FeishuTableMapping>, ApiError> {
    let Some(field_specs) = field_specs(table_id) else {
        return Ok(None);
    };
    let by_id = fields
        .iter()
        .map(|field| (field.field_id.as_str(), field))
        .collect::<BTreeMap<_, _>>();
    let mut mapped = BTreeMap::new();
    for (key, field_id) in field_specs {
        let field = by_id.get(field_id).ok_or_else(|| {
            ApiError::conflict(format!(
                "Archived product table `{table_id}` is missing explicit field `{field_id}` for `{key}`."
            ))
        })?;
        if field.field_type == 17 {
            return Err(ApiError::conflict(format!(
                "Archived product table `{table_id}` maps `{key}` to an attachment field."
            )));
        }
        mapped.insert(key.to_string(), mapped_field(field));
    }
    let primary = fields
        .iter()
        .find(|field| field.is_primary)
        .ok_or_else(|| {
            ApiError::conflict(format!(
                "Archived product table `{table_id}` has no stable primary field."
            ))
        })?;
    mapped.insert("stableId".into(), mapped_field(primary));
    let private_fields = PRICING_FIELD_IDS
        .iter()
        .map(|field_id| {
            by_id
                .get(field_id)
                .copied()
                .map(mapped_field)
                .ok_or_else(|| {
                    ApiError::conflict(format!(
                        "Archived product table `{table_id}` is missing private field `{field_id}`."
                    ))
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let attachments = fields
        .iter()
        .filter(|field| field.field_type == 17)
        .map(mapped_field)
        .collect();
    Ok(Some(FeishuTableMapping {
        fields: mapped,
        source_fields: fields.iter().map(mapped_field).collect(),
        attachments,
        private_fields,
    }))
}

fn field_specs(table_id: &str) -> Option<&'static [(&'static str, &'static str)]> {
    match table_id {
        "tblzksABQBk6rdB6" => Some(CENTRIFUGAL_FIELDS),
        "tblJjxOgBFL0FD0N" => Some(AXIAL_FIELDS),
        "tblOtUU5MaxEnZI7" => Some(AGRICULTURE_FIELDS),
        "tbl3hCDkvVIs2ZSi" => Some(CROSS_FLOW_FIELDS),
        _ => None,
    }
}

fn mapped_field(field: &FeishuField) -> MappedFeishuField {
    MappedFeishuField {
        id: field.field_id.clone(),
        name: field.field_name.clone(),
        field_type: field.field_type,
    }
}
