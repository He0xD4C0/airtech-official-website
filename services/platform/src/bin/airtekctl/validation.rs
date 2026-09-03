async fn validate(
    pool: &PgPool,
    target: ValidationTarget,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut report = ValidationReport::default();
    if matches!(target, ValidationTarget::All | ValidationTarget::Content) {
        validate_content(pool, &mut report).await?;
    }
    if matches!(target, ValidationTarget::All | ValidationTarget::Products) {
        validate_products(pool, &mut report).await?;
    }
    if matches!(target, ValidationTarget::All | ValidationTarget::Staging) {
        validate_staging(pool, &mut report).await?;
    }
    let passed = report.findings.is_empty();
    print_json(json!({
        "status": if passed { "valid" } else { "invalid" },
        "report": report,
    }))?;
    if !passed {
        return Err("validation failed; see the structured findings above".into());
    }
    Ok(())
}

async fn validate_content(pool: &PgPool, report: &mut ValidationReport) -> Result<(), sqlx::Error> {
    for row in sqlx::query("SELECT id, payload FROM content_entries ORDER BY id")
        .fetch_all(pool)
        .await?
    {
        let id: Uuid = row.try_get("id")?;
        let payload: Value = row.try_get("payload")?;
        report.records_checked += 1;
        match serde_json::from_value::<ContentEntry>(payload) {
            Ok(content) => {
                if content.body.schema_version != 1 {
                    report.push(finding(
                        "content",
                        id,
                        "body.schemaVersion",
                        "unsupportedSchema",
                    ));
                }
                if content.body.doc.get("type").and_then(Value::as_str) != Some("doc") {
                    report.push(finding("content", id, "body.doc.type", "docRequired"));
                }
                if content.slug.trim().is_empty() || content.locale.trim().is_empty() {
                    report.push(finding("content", id, "slug", "routeIdentityRequired"));
                }
            }
            Err(_) => report.push(finding("content", id, "$", "invalidStoredPayload")),
        }
    }
    Ok(())
}

async fn validate_products(
    pool: &PgPool,
    report: &mut ValidationReport,
) -> Result<(), sqlx::Error> {
    for row in sqlx::query(
        r#"SELECT product.id,product.payload,product.data_origin,
                  product.source_snapshot_id,product.source_revision,
                  product.product_import_run_id,
                  EXISTS (
                      SELECT 1 FROM product_import_runs AS import
                      WHERE import.id=product.product_import_run_id
                        AND import.data_origin='verifiedCsv'
                  ) AS verified_import_exists
           FROM products AS product ORDER BY product.id"#,
    )
    .fetch_all(pool)
    .await?
    {
        let id: Uuid = row.try_get("id")?;
        let payload: Value = row.try_get("payload")?;
        let source_identity = ProductSourceIdentity {
            data_origin: row.try_get("data_origin")?,
            source_snapshot_id: row.try_get("source_snapshot_id")?,
            source_revision: row.try_get("source_revision")?,
            product_import_run_id: row.try_get("product_import_run_id")?,
            verified_import_exists: row.try_get("verified_import_exists")?,
        };
        report.records_checked += 1;
        match serde_json::from_value::<Product>(payload) {
            Ok(product) => validate_product(id, &product, &source_identity, report),
            Err(_) => report.push(finding("product", id, "$", "invalidStoredPayload")),
        }
    }
    Ok(())
}

fn validate_product(
    id: Uuid,
    product: &Product,
    source: &ProductSourceIdentity,
    report: &mut ValidationReport,
) {
    let valid_source_identity = match source.data_origin.as_str() {
        "feishu" => {
            source
                .source_snapshot_id
                .is_some_and(|value| !value.is_nil())
                && !product.source_snapshot_id.is_nil()
                && !source.source_revision.trim().is_empty()
        }
        "verifiedCsv" => {
            source.source_snapshot_id.is_none()
                && product.source_snapshot_id.is_nil()
                && source.source_revision.starts_with("csv:")
                && product.source_revision == source.source_revision
                && source.product_import_run_id.is_some()
                && source.verified_import_exists
        }
        "developmentFixture" => {
            source.source_snapshot_id.is_none()
                && source.product_import_run_id.is_none()
                && !source.source_revision.trim().is_empty()
        }
        _ => false,
    };
    if product.stable_id.trim().is_empty() || !valid_source_identity {
        report.push(finding(
            "product",
            id,
            "sourceRevision",
            "sourceIdentityRequired",
        ));
    }
    for (index, specification) in product.specifications.iter().enumerate() {
        if specification.state == FactState::Verified
            && (specification.value.is_none()
                || specification
                    .source_reference
                    .as_deref()
                    .is_none_or(|value| value.trim().is_empty()))
        {
            report.push(finding(
                "product",
                id,
                &format!("specifications[{index}]"),
                "verifiedFactRequiresValueAndSource",
            ));
        }
    }
    for (curve_index, curve) in product.performance_curves.iter().enumerate() {
        if curve.points.len() < 2
            || curve.source_reference.trim().is_empty()
            || curve.points.iter().any(|point| {
                !point.airflow.is_finite()
                    || !point.pressure.is_finite()
                    || point.airflow < 0.0
                    || point.pressure < 0.0
            })
            || curve
                .points
                .windows(2)
                .any(|points| points[0].airflow >= points[1].airflow)
        {
            report.push(finding(
                "product",
                id,
                &format!("performanceCurves[{curve_index}]"),
                "invalidPerformanceCurve",
            ));
        }
        if curve.state == FactState::Verified
            && curve
                .test_method
                .as_deref()
                .is_none_or(|value| value.trim().is_empty())
        {
            report.push(finding(
                "product",
                id,
                &format!("performanceCurves[{curve_index}].testMethod"),
                "verifiedCurveRequiresTestMethod",
            ));
        }
    }
}

async fn validate_staging(pool: &PgPool, report: &mut ValidationReport) -> Result<(), sqlx::Error> {
    for row in sqlx::query(
        "SELECT source_record_id, source_payload FROM source_snapshots ORDER BY received_at DESC",
    )
    .fetch_all(pool)
    .await?
    {
        let source_record_id: String = row.try_get("source_record_id")?;
        let payload: Value = row.try_get("source_payload")?;
        report.records_checked += 1;
        for issue in validate_staging_payload(&payload) {
            report.push(ValidationFinding {
                entity_type: "sourceSnapshot",
                entity_id: source_record_id.clone(),
                field_path: issue.field_path,
                code: issue.code,
            });
        }
    }
    Ok(())
}

fn finding(
    entity_type: &'static str,
    entity_id: Uuid,
    field_path: &str,
    code: &str,
) -> ValidationFinding {
    ValidationFinding {
        entity_type,
        entity_id: entity_id.to_string(),
        field_path: field_path.into(),
        code: code.into(),
    }
}
