#[cfg(test)]
mod cases {
    use super::super::*;
    use crate::config::{ApprovedProductMaster, Config};

    pub(super) fn authority_for(parsed: &ParsedProductImport) -> ApprovedProductMaster {
        ApprovedProductMaster {
            sha256: parsed.result.checksum.clone(),
            mapping_version: parsed.result.mapping_version.clone(),
            expected_valid_rows: parsed.result.valid_rows,
            expected_error_rows: parsed.result.malformed_rows,
        }
    }

    #[test]
    pub(super) fn import_keeps_valid_rows_reports_bad_rows_and_excludes_noise_and_price() {
        let csv = concat!(
            "stable_id,model,family,title,price,currency,noise_db,image\n",
            "p-1,B23E280H128-102-B0,Centrifugal,Verified model,12.50,USD,55,missing.jpg\n",
            "p-2,,unknown,Bad row,,,,\n",
            "p-3,M3,Axial,Another verified model,,,,\n"
        );
        let config = Config::for_test();
        let parsed = parse_product_master(
            csv,
            "airtek-basic-v1",
            config.product_staging_encryption_key.as_ref(),
        )
        .expect("valid import");
        assert_eq!(parsed.result.total_rows, 3);
        assert_eq!(parsed.result.valid_rows, 2);
        assert_eq!(parsed.result.malformed_rows, 1);
        assert_eq!(parsed.result.missing_assets.len(), 1);
        assert!(parsed.rows[0].confidential_payload.is_some());
        let serialized = parsed.rows[0].normalized_payload.to_string();
        assert!(!serialized.contains("12.50"));
        assert!(!serialized.to_lowercase().contains("noise"));
        assert!(!serialized.contains("missing.jpg"));
        assert_eq!(parsed.rows[0].normalized_payload["family"], "centrifugal");
    }

    #[test]
    pub(super) fn production_rejects_unapproved_modified_truncated_and_wrong_mapping_sources() {
        let config = Config::for_test();
        let key = config.product_staging_encryption_key.as_ref();
        let approved_csv = concat!(
            "stable_id,model,family,title\n",
            "p-1,M1,Centrifugal,Approved model\n"
        );
        let approved = parse_product_master(approved_csv, "airtek-basic-v1", key).unwrap();
        let authority = authority_for(&approved);
        assert_eq!(
            verify_product_master_authority(
                "production",
                Some(&authority),
                &approved.result.checksum,
                &approved.result.mapping_version,
                approved.result.valid_rows,
                approved.result.malformed_rows,
            )
            .unwrap(),
            ProductMasterAuthorityDecision::ProductionApproved
        );

        for (label, csv, mapping) in [
            (
                "arbitrary",
                "stable_id,model,family,title\np-2,X1,Axial,Other model\n",
                "airtek-basic-v1",
            ),
            (
                "single-byte modification",
                "stable_id,model,family,title\np-1,N1,Centrifugal,Approved model\n",
                "airtek-basic-v1",
            ),
            (
                "truncation",
                "stable_id,model,family,title\np-1,M1,Centrifugal,Approved mod",
                "airtek-basic-v1",
            ),
            ("wrong mapping", approved_csv, "airtek-basic-v2"),
        ] {
            let parsed = parse_product_master(csv, mapping, key).unwrap();
            assert!(
                verify_product_master_authority(
                    "production",
                    Some(&authority),
                    &parsed.result.checksum,
                    &parsed.result.mapping_version,
                    parsed.result.valid_rows,
                    parsed.result.malformed_rows,
                )
                .is_err(),
                "{label} must not enter production staging"
            );
        }
        assert!(verify_product_master_authority(
            "production",
            None,
            &approved.result.checksum,
            &approved.result.mapping_version,
            approved.result.valid_rows,
            approved.result.malformed_rows,
        )
        .is_err());
    }

    #[test]
    pub(super) fn development_recognizes_the_registered_source_without_requiring_it() {
        let config = Config::for_test();
        let parsed = parse_product_master(
            "stable_id,model,family\np-1,M1,Centrifugal\n",
            "airtek-basic-v1",
            config.product_staging_encryption_key.as_ref(),
        )
        .unwrap();
        let authority = authority_for(&parsed);
        assert_eq!(
            verify_product_master_authority(
                "development",
                Some(&authority),
                &parsed.result.checksum,
                &parsed.result.mapping_version,
                parsed.result.valid_rows,
                parsed.result.malformed_rows,
            )
            .unwrap(),
            ProductMasterAuthorityDecision::ConfiguredApprovedDevelopment
        );
        assert!(verify_product_master_authority(
            "development",
            Some(&authority),
            &"0".repeat(64),
            &parsed.result.mapping_version,
            parsed.result.valid_rows,
            parsed.result.malformed_rows,
        )
        .is_err());
    }

    #[test]
    pub(super) fn confidential_source_round_trips_only_with_row_bound_aad() {
        let csv = concat!(
            "stable_id,model,family,price,price_notes,noise_db\n",
            "p-1,M1,Centrifugal,12.50,never-expose-this-note,55\n"
        );
        let config = Config::for_test();
        let key = config.product_staging_encryption_key.as_ref().unwrap();
        let parsed = parse_product_master(csv, "v1", Some(key)).unwrap();
        let row = &parsed.rows[0];
        let sealed = row.confidential_payload.as_ref().unwrap();
        let nonce = &sealed[..12];
        let ciphertext = &sealed[12..sealed.len() - 16];
        let tag = &sealed[sealed.len() - 16..];
        let pricing = decrypt_private_pricing(
            key,
            PrivatePricingEnvelope {
                mapping_version: "v1",
                checksum: &parsed.result.checksum,
                source_row_number: row.row_number,
                stable_id: &row.stable_id,
                nonce,
                ciphertext,
                authentication_tag: tag,
            },
        )
        .unwrap();
        assert_eq!(pricing.get("price").map(String::as_str), Some("12.50"));
        assert!(!pricing.contains_key("pricenotes"));
        assert!(!pricing.contains_key("noisedb"));
        assert!(decrypt_private_pricing(
            key,
            PrivatePricingEnvelope {
                mapping_version: "v1",
                checksum: &parsed.result.checksum,
                source_row_number: 999,
                stable_id: &row.stable_id,
                nonce,
                ciphertext,
                authentication_tag: tag,
            },
        )
        .is_err());
    }

    #[test]
    pub(super) fn durable_job_payload_contains_only_the_import_run_reference() {
        let id = Uuid::new_v4();
        let payload = product_import_job_payload(id);
        assert_eq!(payload, json!({"importRunId": id}));
        let serialized = payload.to_string().to_ascii_lowercase();
        assert!(!serialized.contains("csv"));
        assert!(!serialized.contains("price"));
        assert!(!serialized.contains("ciphertext"));
    }

    #[test]
    pub(super) fn confirmed_master_pricing_headers_are_private_and_allowlisted_on_read() {
        let csv = concat!(
            "文本,一级分类_Product #1,样品报价（sample）,100～500 pcs,500～1000pcs,1000～5000pcs,≥5000pcs,噪声(Noise Level)\n",
            "M1,离心风机 Centrifugal fans,sample,band-1,band-2,band-3,band-4,55\n"
        );
        let config = Config::for_test();
        let key = config.product_staging_encryption_key.as_ref().unwrap();
        let parsed = parse_product_master(csv, "airtek-basic-v1", Some(key)).unwrap();
        let row = &parsed.rows[0];
        let sealed = row.confidential_payload.as_ref().unwrap();
        let pricing = decrypt_private_pricing(
            key,
            PrivatePricingEnvelope {
                mapping_version: "airtek-basic-v1",
                checksum: &parsed.result.checksum,
                source_row_number: row.row_number,
                stable_id: &row.stable_id,
                nonce: &sealed[..12],
                ciphertext: &sealed[12..sealed.len() - 16],
                authentication_tag: &sealed[sealed.len() - 16..],
            },
        )
        .unwrap();
        assert_eq!(pricing.len(), 5);
        assert_eq!(pricing["样品报价sample"], "sample");
        assert_eq!(pricing["100500pcs"], "band-1");
        assert_eq!(pricing["5001000pcs"], "band-2");
        assert_eq!(pricing["10005000pcs"], "band-3");
        assert_eq!(pricing["5000pcs"], "band-4");
        assert!(pricing.values().all(|value| value != "55"));
    }

    #[test]
    pub(super) fn feishu_pricing_is_selected_by_saved_field_ids() {
        let config = Config::for_test();
        let key = config.product_staging_encryption_key.as_ref().unwrap();
        let mapping_version = "feishu-product-v1-r4";
        let checksum = "feishu-run:pricing-test";
        let row_number = 2;
        let stable_id = "fs.pricing-test";
        let private = json!({
            "schema": "feishu-private-v1",
            "fieldsById": {
                "fld-model": {"name": "型号", "value": "AX-100"},
                "fld-price": {"name": "更新后的报价名称", "value": "125"}
            },
            "pricingFieldIds": ["fld-price"]
        });
        let aad = format!("{mapping_version}:{checksum}:{row_number}:{stable_id}");
        let sealed = encrypt_confidential(key, aad.as_bytes(), private.to_string().as_bytes())
            .expect("private Feishu payload encrypts");
        let pricing = decrypt_private_pricing(
            key,
            PrivatePricingEnvelope {
                mapping_version,
                checksum,
                source_row_number: row_number,
                stable_id,
                nonce: &sealed[..12],
                ciphertext: &sealed[12..sealed.len() - 16],
                authentication_tag: &sealed[sealed.len() - 16..],
            },
        )
        .expect("saved pricing ids decrypt");
        assert_eq!(pricing.len(), 1);
        assert_eq!(pricing["更新后的报价名称"], "125");
        assert!(!pricing.contains_key("型号"));
    }

    #[test]
    pub(super) fn csv_parser_supports_commas_quotes_and_newlines() {
        let rows =
            parse_csv("a,b\n1,\"two, three\"\n2,\"line one\nline two\"\n").expect("valid CSV");
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1].1[1], "two, three");
        assert_eq!(rows[2].1[1], "line one\nline two");
    }

    #[test]
    pub(super) fn duplicate_stable_ids_are_malformed() {
        let config = Config::for_test();
        let parsed = parse_product_master(
            "stableId,family\np-1,axial\np-1,axial\n",
            "v1",
            config.product_staging_encryption_key.as_ref(),
        )
        .unwrap();
        assert_eq!(parsed.result.valid_rows, 1);
        assert_eq!(parsed.result.malformed_rows, 1);
        assert_eq!(parsed.result.errors[0].code, "duplicateStableId");
    }

    #[test]
    pub(super) fn verified_product_master_contract_is_370_valid_and_5_malformed_when_fixture_is_provided(
    ) {
        let Ok(path) = std::env::var("AIRTEK_PRODUCT_MASTER_TEST_CSV") else {
            return;
        };
        let csv = std::fs::read_to_string(path).expect("verified Product Master CSV is readable");
        let config = Config::for_test();
        let parsed = parse_product_master(
            &csv,
            "airtek-basic-v1",
            config.product_staging_encryption_key.as_ref(),
        )
        .expect("verified Product Master is accepted");
        assert_eq!(
            parsed.result.checksum,
            "e3b944d5d979c72d963ba353416ef452f9dac0bf63182bb09fc6b1201c043800"
        );
        assert_eq!(parsed.result.total_rows, 375);
        assert_eq!(parsed.result.valid_rows, 370);
        assert_eq!(parsed.result.malformed_rows, 5);
        assert_eq!(
            parsed
                .result
                .errors
                .iter()
                .filter(|error| error.severity == "error")
                .count(),
            5
        );
        assert!(parsed.rows.iter().all(|row| {
            let payload = row.normalized_payload.to_string().to_lowercase();
            !payload.contains("noise") && !payload.contains("sample") && !payload.contains("price")
        }));
    }
}
