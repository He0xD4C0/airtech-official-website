use super::*;

fn field(id: &str, name: &str, field_type: i32, primary: bool) -> FeishuField {
    FeishuField {
        field_id: id.into(),
        field_name: name.into(),
        field_type,
        is_primary: primary,
    }
}

#[test]
fn maps_model_attachments_and_private_pricing_by_stable_id() {
    let fields = vec![
        field("fld-model", "产品型号(Model）", 1, true),
        field("fld-file", "PQ曲线源文件", 17, false),
        field("fld-price", "样品报价(Sample)", 1, false),
    ];
    let table = discover_table(&fields).unwrap();
    assert_eq!(table.fields["model"].id, "fld-model");
    assert_eq!(table.attachments[0].id, "fld-file");
    assert_eq!(table.private_fields[0].id, "fld-price");
}

#[test]
fn type_change_is_schema_drift_even_after_a_display_rename() {
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
                source_fields: vec![],
                attachments: vec![],
                private_fields: vec![],
            },
        )]),
    };
    let sources = vec![DiscoveredSource {
        source: FeishuSource {
            enabled: true,
            wiki_token: "wiki".into(),
            table_id: "tbl".into(),
            name: "Table".into(),
            family: airtek_domain::models::ProductFamily::Axial,
            application: None,
        },
        app_token: "app".into(),
        fields: vec![field("fld", "新型号名", 2, true)],
    }];
    assert!(validate_mapping(mapping, &sources).is_err());
}

#[test]
fn assigns_each_field_to_its_most_specific_fact() {
    let fields = vec![
        field("fld-model", "型号", 1, true),
        field("fld-package", "包装尺寸(mm)", 1, false),
        field("fld-drawing", "产品尺寸图纸", 17, false),
    ];
    let table = discover_table(&fields).unwrap();
    assert_eq!(table.fields["packageDimensions"].id, "fld-package");
    assert!(!table.fields.contains_key("productDimensions"));
    assert_eq!(table.attachments[0].id, "fld-drawing");
}

#[test]
fn equal_matches_are_rejected_instead_of_guessed() {
    let fields = vec![
        field("fld-model", "型号", 1, true),
        field("fld-voltage-a", "额定电压", 1, false),
        field("fld-voltage-b", "额定电压", 1, false),
    ];
    assert!(discover_table(&fields).is_err());
}

#[test]
fn mapping_identity_keeps_same_table_id_in_different_wikis_distinct() {
    let discovered = ["wiki-a", "wiki-b"].map(|wiki| DiscoveredSource {
        source: FeishuSource {
            enabled: true,
            wiki_token: wiki.into(),
            table_id: "tbl-shared".into(),
            name: wiki.into(),
            family: airtek_domain::models::ProductFamily::Axial,
            application: None,
        },
        app_token: format!("app-{wiki}"),
        fields: vec![field("fld-model", "型号", 1, true)],
    });
    let mapping = discover_mapping("v1", &discovered).unwrap();
    assert_eq!(mapping.tables.len(), 2);
    assert!(mapping
        .tables
        .contains_key(&source_identity_key(&discovered[0].source)));
    assert!(mapping
        .tables
        .contains_key(&source_identity_key(&discovered[1].source)));
}

#[test]
fn maps_all_four_archived_product_schemas_without_guessing_between_related_fields() {
    let schemas: BTreeMap<String, Vec<FeishuField>> =
        serde_json::from_str(include_str!("archive_fields.json")).unwrap();
    for (name, fields) in schemas {
        let table = discover_archived_table(&name, &fields)
            .unwrap_or_else(|error| panic!("{name}: {error}"))
            .unwrap_or_else(|| panic!("{name}: explicit archived mapping is missing"));
        assert!(table.fields.contains_key("model"));
        assert!(table.fields.contains_key("category"));
        assert!(table.fields.contains_key("primaryCategory"));
        assert_eq!(
            table
                .fields
                .get("ambientTemperature")
                .map(|field| field.name.as_str()),
            Some("Amb.Temp"),
            "{name}: ambientTemperature"
        );
        assert_eq!(
            table.fields.get("airflow").map(|field| field.name.as_str()),
            Some("风量(Air Flow)"),
            "{name}: airflow"
        );
        if name != "tblOtUU5MaxEnZI7" {
            assert_eq!(
                table
                    .fields
                    .get("pressure")
                    .map(|field| field.name.as_str()),
                Some("风压(AIr Pressure)"),
                "{name}: pressure"
            );
        }
        let private_names: Vec<_> = table
            .private_fields
            .iter()
            .map(|field| field.name.as_str())
            .collect();
        for field in fields.iter().filter(|field| {
            field.field_type != 17
                && (field.field_name.contains("sample") || field.field_name.contains("批"))
        }) {
            assert!(
                private_names.contains(&field.field_name.as_str()),
                "{}",
                field.field_name
            );
        }
    }
}

#[test]
fn archived_tables_fail_closed_without_their_exact_stable_field_ids() {
    let schemas: BTreeMap<String, Vec<FeishuField>> =
        serde_json::from_str(include_str!("archive_fields.json")).unwrap();
    for (name, fields) in schemas {
        let mut renamed = fields.clone();
        let model = renamed
            .iter_mut()
            .find(|field| field.field_id == "fldjq4VB5w" || field.field_id == "fldjAaBY2N")
            .expect("archived model field exists");
        model.field_id = "renamed-model-field".into();
        assert!(
            discover_archived_table(&name, &renamed).is_err(),
            "{name} must not fall back when an exact field id is missing"
        );
    }
}
