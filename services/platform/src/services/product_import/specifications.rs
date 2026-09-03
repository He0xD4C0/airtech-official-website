fn normalized_specifications(
    header_index: &HashMap<String, usize>,
    record: &[String],
    checksum: &str,
    row_number: i32,
    stable_id: &str,
) -> (Vec<Value>, Vec<ProductImportError>) {
    let frequency = value_for(
        header_index,
        record,
        &["frequency", "频率frequency", "频率"],
    );
    let source_reference = format!("verified-csv:{checksum}:row-{row_number}:{stable_id}");
    type SpecificationDefinition = (
        &'static str,
        &'static str,
        &'static [&'static str],
        Option<&'static str>,
        bool,
    );
    let definitions: &[SpecificationDefinition] = &[
        (
            "voltage",
            "Voltage",
            &["voltage", "电压voltage", "电压"],
            Some("V"),
            true,
        ),
        (
            "frequency",
            "Frequency",
            &["frequency", "频率frequency", "频率"],
            Some("Hz"),
            false,
        ),
        (
            "diameter",
            "Diameter",
            &["diameter", "直径diameter", "直径"],
            Some("mm"),
            false,
        ),
        (
            "current",
            "Current",
            &["current", "电流current", "电流"],
            Some("A"),
            true,
        ),
        (
            "speed",
            "Speed",
            &["speed", "转速speed", "转速"],
            Some("rpm"),
            true,
        ),
        (
            "airflow",
            "Air Flow",
            &["airflow", "风量airflow", "风量"],
            Some("m³/h"),
            true,
        ),
        (
            "pressure",
            "Air Pressure",
            &["airpressure", "风压airpressure", "风压"],
            Some("Pa"),
            true,
        ),
        (
            "power",
            "Power",
            &["power", "功率power", "功率"],
            Some("W"),
            true,
        ),
        (
            "material",
            "Material",
            &["material", "风轮材质material", "风轮材质"],
            None,
            false,
        ),
        (
            "dimension",
            "Product Dimension",
            &["dimension", "产品尺寸dimension", "产品尺寸"],
            None,
            false,
        ),
        (
            "packageData",
            "Package Data",
            &["packagedata", "包装数据packagedata", "包装数据"],
            None,
            false,
        ),
        (
            "protectionClass",
            "Protection Class",
            &["protectionclass", "防护等级protectionclass", "防护等级"],
            None,
            false,
        ),
        (
            "insulationClass",
            "Insulation Class",
            &["insulationclass", "绝缘等级insulationclass", "绝缘等级"],
            None,
            false,
        ),
    ];
    let mut specifications = Vec::new();
    let mut warnings = Vec::new();
    for (key, label, aliases, unit, frequency_scoped) in definitions {
        let Some(raw) = value_for(header_index, record, aliases) else {
            continue;
        };
        let values = split_paired(&raw);
        let frequencies = frequency.as_deref().map(split_paired).unwrap_or_default();
        if *frequency_scoped && values.len() > 1 && values.len() == frequencies.len() {
            for (index, value) in values.into_iter().enumerate() {
                specifications.push(json!({
                    "key": format!("{key}@{}Hz", frequencies[index]),
                    "label": label,
                    "value": value,
                    "unit": unit,
                    "operatingCondition": format!("{} Hz", frequencies[index]),
                    "state": "verified",
                    "sourceReference": source_reference,
                }));
            }
        } else {
            let pending =
                *frequency_scoped && values.len() > 1 && values.len() != frequencies.len();
            if pending {
                warnings.push(ProductImportError {
                    row_number,
                    stable_id: Some(stable_id.to_owned()),
                    field_name: Some((*key).to_owned()),
                    severity: "warning".into(),
                    code: "unpairedOperatingCondition".into(),
                    detail: "Multiple values do not map one-to-one to frequency values; the raw value is retained pending verification.".into(),
                });
            }
            specifications.push(json!({
                "key": key,
                "label": label,
                "value": raw,
                "unit": unit,
                "operatingCondition": if *frequency_scoped {
                    frequency.as_ref().map(|value| format!("Frequency: {value} Hz"))
                } else { None },
                "state": if pending { "pendingVerification" } else { "verified" },
                "sourceReference": source_reference,
            }));
        }
    }
    (specifications, warnings)
}
