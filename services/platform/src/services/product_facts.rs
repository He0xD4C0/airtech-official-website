use serde_json::Value;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::error::ApiError;

/// Rebuild the relational projections for one immutable Product facts
/// revision. CSV and Feishu acceptance both call this function inside the same
/// transaction that inserts the revision, so no reader can observe a revision
/// without its specifications, conditions, curves, and current asset set.
pub async fn project_product_facts(
    transaction: &mut Transaction<'_, Postgres>,
    product_id: Uuid,
    product_revision: i64,
    payload: &Value,
    fallback_source_reference: &str,
) -> Result<(), ApiError> {
    let specifications = optional_array(payload, "specifications")?;
    let operating_conditions = optional_array(payload, "operatingConditions")?;
    let performance_curves = optional_array(payload, "performanceCurves")?;
    let assets = optional_present_array(payload, "assets")?;

    sqlx::query("DELETE FROM product_specs WHERE product_id=$1 AND product_revision=$2")
        .bind(product_id)
        .bind(product_revision)
        .execute(&mut **transaction)
        .await?;
    for specification in specifications {
        let value = specification.get("value").filter(|value| !value.is_null());
        let numeric_value = value.and_then(json_number_text);
        let text_value = if numeric_value.is_some() {
            None
        } else {
            value.map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| value.to_string())
            })
        };
        sqlx::query(
            r#"INSERT INTO product_specs
               (product_id,product_revision,key,label,numeric_value,text_value,unit,
                operating_condition,fact_state,source_reference)
               VALUES ($1,$2,$3,$4,$5::numeric,$6,$7,$8,$9,$10)"#,
        )
        .bind(product_id)
        .bind(product_revision)
        .bind(required_text(specification, "key")?)
        .bind(required_text(specification, "label")?)
        .bind(numeric_value)
        .bind(text_value)
        .bind(optional_text(specification, "unit")?)
        .bind(optional_text(specification, "operatingCondition")?)
        .bind(required_text(specification, "state")?)
        .bind(
            optional_text(specification, "sourceReference")?
                .unwrap_or_else(|| fallback_source_reference.to_owned()),
        )
        .execute(&mut **transaction)
        .await?;
    }

    sqlx::query(
        "DELETE FROM product_operating_conditions WHERE product_id=$1 AND product_revision=$2",
    )
    .bind(product_id)
    .bind(product_revision)
    .execute(&mut **transaction)
    .await?;
    for condition in operating_conditions {
        let frequency_hz = optional_number_text(condition, "frequencyHz")?;
        let state = optional_text(condition, "state")?.unwrap_or_else(|| "verified".into());
        sqlx::query(
            r#"INSERT INTO product_operating_conditions
               (product_id,product_revision,key,label,frequency_hz,voltage,fact_state,
                source_reference,payload)
               VALUES ($1,$2,$3,$4,$5::numeric,$6,$7,$8,$9)"#,
        )
        .bind(product_id)
        .bind(product_revision)
        .bind(required_text(condition, "key")?)
        .bind(required_text(condition, "label")?)
        .bind(frequency_hz)
        .bind(optional_text(condition, "voltage")?)
        .bind(state)
        .bind(
            optional_text(condition, "sourceReference")?
                .unwrap_or_else(|| fallback_source_reference.to_owned()),
        )
        .bind(condition)
        .execute(&mut **transaction)
        .await?;
    }

    sqlx::query("DELETE FROM performance_curves WHERE product_id=$1 AND product_revision=$2")
        .bind(product_id)
        .bind(product_revision)
        .execute(&mut **transaction)
        .await?;
    for curve in performance_curves {
        let speed_rpm = optional_positive_i32(curve, "speedRpm")?;
        let density_kg_m3 = optional_number_text(curve, "densityKgM3")?;
        let points = match curve.get("points") {
            None | Some(Value::Null) => Value::Array(Vec::new()),
            Some(Value::Array(points)) => Value::Array(points.clone()),
            Some(_) => {
                return Err(ApiError::conflict(
                    "Accepted Product facts `points` must be an array.",
                ));
            }
        };
        sqlx::query(
            r#"INSERT INTO performance_curves
               (id,product_id,product_revision,airflow_unit,pressure_unit,speed_rpm,
                density_kg_m3,voltage,test_method,source_reference,fact_state,points)
               VALUES ($1,$2,$3,$4,$5,$6,$7::numeric,$8,$9,$10,$11,$12)"#,
        )
        .bind(Uuid::new_v4())
        .bind(product_id)
        .bind(product_revision)
        .bind(required_text(curve, "airflowUnit")?)
        .bind(required_text(curve, "pressureUnit")?)
        .bind(speed_rpm)
        .bind(density_kg_m3)
        .bind(optional_text(curve, "voltage")?)
        .bind(optional_text(curve, "testMethod")?)
        .bind(
            optional_text(curve, "sourceReference")?
                .unwrap_or_else(|| fallback_source_reference.to_owned()),
        )
        .bind(required_text(curve, "state")?)
        .bind(points)
        .execute(&mut **transaction)
        .await?;
    }

    // When the accepted source carries an explicit asset set it replaces the
    // current storage projection. Historical source references remain in
    // immutable revision payloads; filename-only CSV references omit `assets`,
    // stay in the missing-assets report, and never become public downloads.
    if let Some(assets) = assets {
        sqlx::query("DELETE FROM product_assets WHERE product_id=$1")
            .bind(product_id)
            .execute(&mut **transaction)
            .await?;
        for asset in assets {
            let id = asset
                .get("id")
                .and_then(Value::as_str)
                .and_then(|value| value.parse::<Uuid>().ok())
                .unwrap_or_else(Uuid::new_v4);
            sqlx::query(
                r#"INSERT INTO product_assets
                   (id,product_id,asset_type,locale,revision,storage_key,checksum,
                    source_reference)
                   VALUES ($1,$2,$3,$4,$5,$6,$7,$8)"#,
            )
            .bind(id)
            .bind(product_id)
            .bind(required_text(asset, "assetType")?)
            .bind(optional_text(asset, "locale")?)
            .bind(required_text(asset, "revision")?)
            .bind(required_text(asset, "storageKey")?)
            .bind(required_text(asset, "checksum")?)
            .bind(
                optional_text(asset, "sourceReference")?
                    .unwrap_or_else(|| fallback_source_reference.to_owned()),
            )
            .execute(&mut **transaction)
            .await?;
        }
    }
    Ok(())
}

fn optional_present_array<'a>(
    payload: &'a Value,
    key: &str,
) -> Result<Option<&'a [Value]>, ApiError> {
    match payload.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(values)) => Ok(Some(values)),
        Some(_) => Err(ApiError::conflict(format!(
            "Accepted Product facts `{key}` must be an array."
        ))),
    }
}

fn optional_array<'a>(payload: &'a Value, key: &str) -> Result<&'a [Value], ApiError> {
    match payload.get(key) {
        None | Some(Value::Null) => Ok(&[]),
        Some(Value::Array(values)) => Ok(values),
        Some(_) => Err(ApiError::conflict(format!(
            "Accepted Product facts `{key}` must be an array."
        ))),
    }
}

fn required_text(payload: &Value, key: &str) -> Result<String, ApiError> {
    optional_text(payload, key)?.ok_or_else(|| {
        ApiError::conflict(format!(
            "Accepted Product facts `{key}` must be a non-empty string."
        ))
    })
}

fn optional_text(payload: &Value, key: &str) -> Result<Option<String>, ApiError> {
    match payload.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if !value.trim().is_empty() => Ok(Some(value.trim().to_owned())),
        Some(Value::String(_)) => Err(ApiError::conflict(format!(
            "Accepted Product facts `{key}` must not be empty."
        ))),
        Some(_) => Err(ApiError::conflict(format!(
            "Accepted Product facts `{key}` must be a string."
        ))),
    }
}

fn json_number_text(value: &Value) -> Option<String> {
    match value {
        Value::Number(number) => Some(number.to_string()),
        Value::String(value) if value.trim().parse::<f64>().ok().is_some_and(f64::is_finite) => {
            Some(value.trim().to_owned())
        }
        _ => None,
    }
}

fn optional_number_text(payload: &Value, key: &str) -> Result<Option<String>, ApiError> {
    match payload.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => json_number_text(value).map(Some).ok_or_else(|| {
            ApiError::conflict(format!(
                "Accepted Product facts `{key}` must be a finite numeric value."
            ))
        }),
    }
}

fn optional_positive_i32(payload: &Value, key: &str) -> Result<Option<i32>, ApiError> {
    let Some(value) = payload.get(key).filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    let parsed = value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
        .or_else(|| value.as_str().and_then(|value| value.parse::<i64>().ok()))
        .filter(|value| *value > 0)
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(|| {
            ApiError::conflict(format!(
                "Accepted Product facts `{key}` must be a positive 32-bit integer."
            ))
        })?;
    Ok(Some(parsed))
}
