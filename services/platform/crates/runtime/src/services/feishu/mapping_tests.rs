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
