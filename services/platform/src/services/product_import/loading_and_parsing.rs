use super::*;

pub(super) fn required_payload_text<'a>(
    payload: &'a Value,
    key: &str,
) -> Result<&'a str, ApiError> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| ApiError::internal(format!("Normalized product `{key}` is missing.")))
}

pub async fn load_product_import_result(
    pool: &PgPool,
    id: Uuid,
) -> Result<Option<ProductImportResult>, ApiError> {
    let row = sqlx::query(
        r#"SELECT id,source_checksum,mapping_version,status,records_received,records_valid,
                  created_at FROM product_import_runs WHERE id=$1"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let error_rows = sqlx::query(
        r#"SELECT source_record_id,severity,error_code,field_path,message,details
           FROM product_import_errors WHERE import_run_id=$1
           ORDER BY created_at,id"#,
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    let errors = error_rows
        .into_iter()
        .map(|row| {
            let details: Value = row.try_get("details")?;
            Ok(ProductImportError {
                row_number: details
                    .get("rowNumber")
                    .and_then(Value::as_i64)
                    .unwrap_or(0) as i32,
                stable_id: row.try_get("source_record_id")?,
                field_name: row.try_get("field_path")?,
                severity: row.try_get("severity")?,
                code: row.try_get("error_code")?,
                detail: row.try_get("message")?,
            })
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    let missing_rows = sqlx::query(
        r#"SELECT source_record_id,asset_type,source_reference
           FROM product_import_missing_assets
           WHERE import_run_id=$1 AND resolution_status='missing' ORDER BY created_at,id"#,
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    let missing_assets = missing_rows
        .into_iter()
        .map(|row| {
            Ok(MissingAssetReference {
                stable_id: row.try_get("source_record_id")?,
                asset_type: row.try_get("asset_type")?,
                source_reference: row.try_get("source_reference")?,
            })
        })
        .collect::<Result<Vec<_>, ApiError>>()?;
    let total_rows: i64 = row.try_get("records_received")?;
    let valid_rows: i64 = row.try_get("records_valid")?;
    Ok(Some(ProductImportResult {
        id: row.try_get("id")?,
        checksum: row.try_get("source_checksum")?,
        mapping_version: row.try_get("mapping_version")?,
        status: row.try_get("status")?,
        total_rows,
        valid_rows,
        malformed_rows: total_rows.saturating_sub(valid_rows),
        errors,
        missing_assets,
        reused: false,
        created_at: row.try_get("created_at")?,
    }))
}

pub fn parse_product_master(
    csv: &str,
    mapping_version: &str,
    key: Option<&ProductStagingEncryptionKey>,
) -> Result<ParsedProductImport, ApiError> {
    if csv.is_empty() || csv.len() > MAX_CSV_BYTES {
        return Err(ApiError::bad_request(
            "csv must contain between 1 byte and 16 MiB.",
        ));
    }
    if mapping_version.trim().is_empty() || mapping_version.len() > 100 {
        return Err(ApiError::bad_request(
            "mappingVersion must contain 1 to 100 characters.",
        ));
    }

    let checksum = format!("{:x}", Sha256::digest(csv.as_bytes()));
    let records = parse_csv(csv)?;
    if records.len() < 2 {
        return Err(ApiError::bad_request(
            "csv must contain a header row and at least one product row.",
        ));
    }
    if records.len() > MAX_ROWS + 1 {
        return Err(ApiError::bad_request("csv contains too many rows."));
    }

    let header = records[0]
        .1
        .iter()
        .map(|value| normalize_header(value))
        .collect::<Vec<_>>();
    let header_index = header
        .iter()
        .enumerate()
        .filter(|(_, value)| !value.is_empty())
        .map(|(index, value)| (value.clone(), index))
        .collect::<HashMap<_, _>>();
    if header_index.is_empty() {
        return Err(ApiError::bad_request("csv header row is empty."));
    }

    let stable_index = find_header(
        &header_index,
        &["stableid", "productid", "sku", "id", "文本"],
    )
    .ok_or_else(|| ApiError::bad_request("csv is missing the stableId column."))?;
    let family_index = find_header(
        &header_index,
        &[
            "family",
            "productfamily",
            "category",
            "taxonomy",
            "一级分类product1",
            "一级分类",
        ],
    )
    .ok_or_else(|| ApiError::bad_request("csv is missing the family column."))?;
    let model_index = find_header(
        &header_index,
        &["model", "modelnumber", "partnumber", "产品型号model"],
    );
    let title_index = find_header(&header_index, &["title", "name", "productname"]);
    let slug_index = find_header(&header_index, &["slug", "urlslug"]);
    let locale_index = find_header(&header_index, &["locale", "language"]);
    let subtype_index = find_header(
        &header_index,
        &["subtype", "type", "二级类product2", "二级类"],
    );
    let motor_index = find_header(&header_index, &["motortechnology", "motortype", "motor"]);
    let summary_index = find_header(&header_index, &["summary", "description"]);
    let asset_indexes = header
        .iter()
        .enumerate()
        .filter(|(_, name)| is_asset_header(name))
        .map(|(index, name)| (index, asset_type(name).to_owned()))
        .collect::<Vec<_>>();

    let mut errors = Vec::new();
    let mut rows = Vec::new();
    let mut missing_assets = Vec::new();
    let mut seen = HashSet::new();
    for (row_number, record) in records.into_iter().skip(1) {
        // A physical blank line is harmless, but an explicitly present CSV
        // row made of delimiters is a malformed source record and must remain
        // visible in the import report (the verified master has one such row).
        if record.len() == 1 && record[0].trim().is_empty() {
            continue;
        }
        let stable_id = field(&record, Some(stable_index)).trim().to_owned();
        let mut row_errors = Vec::new();
        if stable_id.is_empty() || stable_id.len() > 200 {
            row_errors.push(import_error(
                row_number,
                none_if_empty(&stable_id),
                "stableId",
                "invalidStableId",
                "stableId must contain 1 to 200 characters.",
            ));
        } else if !seen.insert(stable_id.to_lowercase()) {
            row_errors.push(import_error(
                row_number,
                Some(&stable_id),
                "stableId",
                "duplicateStableId",
                "stableId is duplicated in this import.",
            ));
        }
        if !row_errors.is_empty() {
            errors.extend(row_errors);
            continue;
        }
        let raw_family = field(&record, Some(family_index));
        let family = normalize_family(raw_family);
        if family.is_none() {
            row_errors.push(import_error(
                row_number,
                none_if_empty(&stable_id),
                "family",
                "invalidFamily",
                "family must map to Centrifugal, Axial, Cross-flow, Inline Duct, or Motors.",
            ));
        }
        if !row_errors.is_empty() {
            errors.extend(row_errors);
            continue;
        }

        let model = non_empty(field(&record, model_index));
        let title = non_empty(field(&record, title_index))
            .or_else(|| model.clone())
            .unwrap_or_else(|| stable_id.clone());
        let supplied_slug = non_empty(field(&record, slug_index));
        let slug = supplied_slug
            .as_deref()
            .map(slugify)
            .filter(|slug| !slug.is_empty())
            .unwrap_or_else(|| slugify(model.as_deref().unwrap_or(&stable_id)));
        if slug.is_empty() {
            errors.push(import_error(
                row_number,
                Some(&stable_id),
                "slug",
                "invalidSlug",
                "A URL-safe slug could not be derived.",
            ));
            continue;
        }

        let locale = non_empty(field(&record, locale_index)).unwrap_or_else(|| "en".into());
        if !valid_locale(&locale) {
            errors.push(import_error(
                row_number,
                Some(&stable_id),
                "locale",
                "invalidLocale",
                "locale must be a simple BCP 47 language tag.",
            ));
            continue;
        }

        for (index, kind) in &asset_indexes {
            for reference in split_asset_references(field(&record, Some(*index))) {
                missing_assets.push(MissingAssetReference {
                    stable_id: stable_id.clone(),
                    asset_type: kind.clone(),
                    source_reference: reference,
                });
            }
        }

        let key = key.ok_or_else(|| {
            ApiError::service_unavailable(
                "Product staging encryption is not configured; source rows cannot be imported.",
            )
        })?;
        let source_row = header
            .iter()
            .enumerate()
            .map(|(index, name)| {
                (
                    name.clone(),
                    Value::String(field(&record, Some(index)).to_owned()),
                )
            })
            .collect::<Map<_, _>>();
        let confidential_payload = Some(encrypt_confidential(
            key,
            format!("{mapping_version}:{checksum}:{row_number}:{stable_id}").as_bytes(),
            &serde_json::to_vec(&source_row)
                .map_err(|_| ApiError::internal("Private product staging serialization failed."))?,
        )?);

        // This projection is intentionally allow-listed. Price and every
        // noise-related column remain absent even when present in the source.
        let mut source_fields = Map::new();
        for (index, name) in header.iter().enumerate() {
            if is_safe_optional_header(name) {
                let value = field(&record, Some(index)).trim();
                if !value.is_empty() {
                    source_fields.insert(name.to_owned(), Value::String(value.to_owned()));
                }
            }
        }
        let (specifications, warnings) =
            normalized_specifications(&header_index, &record, &checksum, row_number, &stable_id);
        errors.extend(warnings);
        let operating_conditions = normalized_operating_conditions(&header_index, &record);
        let subtype = non_empty(field(&record, subtype_index));
        let motor_technology = normalize_motor_technology(field(&record, motor_index))
            .or_else(|| normalize_motor_technology(subtype.as_deref().unwrap_or_default()));
        let normalized_payload = json!({
            "stableId": stable_id,
            "model": model,
            "slug": slug,
            "locale": locale,
            "family": family.expect("family was validated"),
            "subtype": subtype,
            "motorTechnology": motor_technology,
            "title": title,
            "summary": non_empty(field(&record, summary_index)),
            "specifications": specifications,
            "operatingConditions": operating_conditions,
            "performanceCurves": [],
            "sourceRevision": format!("csv:{checksum}"),
            "mappingVersion": mapping_version,
            "sourceFields": source_fields,
        });
        rows.push(ImportedProductRow {
            row_number,
            stable_id,
            normalized_payload,
            confidential_payload,
        });
    }

    let fatal_rows = errors
        .iter()
        .filter(|error| error.severity == "error")
        .map(|error| error.row_number)
        .collect::<HashSet<_>>();
    let total_rows = rows.len() as i64 + fatal_rows.len() as i64;
    let malformed_rows = fatal_rows.len() as i64;
    let status = if rows.is_empty() {
        "failed"
    } else if errors.is_empty() {
        "validated"
    } else {
        "validatedWithErrors"
    };
    let created_at = Utc::now();
    Ok(ParsedProductImport {
        result: ProductImportResult {
            id: Uuid::new_v4(),
            checksum,
            mapping_version: mapping_version.to_owned(),
            status: status.into(),
            total_rows,
            valid_rows: rows.len() as i64,
            malformed_rows,
            errors,
            missing_assets,
            reused: false,
            created_at,
        },
        rows,
    })
}
