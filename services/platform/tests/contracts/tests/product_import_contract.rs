use airtek_domain::models::{
    CurvePoint, FactState, PerformanceCurve, Product, ProductFamily, PublicationStatus,
    SeoMetadata, SpecValue,
};
use airtek_jobs::worker::run_one_pending_job;
use airtek_runtime::services::product_import::{
    decrypt_private_pricing, parse_product_master, promote_staged_product_import,
    reset_staged_product_import_for_retry, stage_and_queue_product_import, PrivatePricingEnvelope,
};
use airtek_runtime::AppState;
use airtek_runtime::Config;
use serde_json::Value;
use sqlx::{postgres::PgPoolOptions, Row};
use uuid::Uuid;

mod support;

struct SyntheticProductMaster {
    csv: String,
    first_stable_id: String,
    first_private_price: String,
    private_price_prefix: String,
    missing_asset: String,
}

fn synthetic_product_master(prefix: &str) -> SyntheticProductMaster {
    let mut csv =
        String::from("stable_id,model,family,motor_technology,title,locale,price,currency,image\n");
    let private_price_prefix = format!("ci-private-{prefix}-");
    let missing_asset = format!("ci-missing-{prefix}.svg");
    let mut first_stable_id = String::new();
    let mut first_private_price = String::new();

    for index in 0..370 {
        let stable_id = format!("CI-{prefix}-{index:03}");
        let model = format!("CI-MODEL-{prefix}-{index:03}");
        let (family, motor_technology) = if index % 2 == 0 {
            ("Centrifugal", "EC")
        } else {
            ("Inline Duct", "AC")
        };
        let private_price = format!("{private_price_prefix}{index:03}");
        let image = if index == 0 {
            missing_asset.as_str()
        } else {
            ""
        };
        csv.push_str(&format!(
            "{stable_id},{model},{family},{motor_technology},CI synthetic product {index:03},en,{private_price},TEST,{image}\n"
        ));
        if index == 0 {
            first_stable_id = stable_id;
            first_private_price = private_price;
        }
    }

    // Each malformed source row has exactly one deliberate fatal error. These
    // rows exercise reporting without borrowing any value from the controlled
    // commercial Product Master acceptance fixture above.
    csv.push_str(&format!(
        ",CI-INVALID-EMPTY-{prefix},Centrifugal,EC,CI invalid empty stable id,en,ci-invalid,TEST,\n"
    ));
    csv.push_str(&format!(
        "{first_stable_id},CI-INVALID-DUP-{prefix},Centrifugal,EC,CI invalid duplicate stable id,en,ci-invalid,TEST,\n"
    ));
    csv.push_str(&format!(
        "CI-{prefix}-BAD-FAMILY,CI-INVALID-FAMILY-{prefix},Unsupported,EC,CI invalid family,en,ci-invalid,TEST,\n"
    ));
    csv.push_str(&format!(
        "CI-{prefix}-BAD-LOCALE,CI-INVALID-LOCALE-{prefix},Centrifugal,EC,CI invalid locale,en_US,ci-invalid,TEST,\n"
    ));
    csv.push_str(&format!(
        "CI-{prefix}-NO-FAMILY,CI-INVALID-NO-FAMILY-{prefix},,AC,CI invalid empty family,en,ci-invalid,TEST,\n"
    ));

    SyntheticProductMaster {
        csv,
        first_stable_id,
        first_private_price,
        private_price_prefix,
        missing_asset,
    }
}

#[path = "product_import_contract/feishu_takeover_and_expiry.rs"]
mod feishu_takeover_and_expiry;
#[path = "product_import_contract/synthetic_master.rs"]
mod synthetic_master;
#[path = "product_import_contract/verified_master.rs"]
mod verified_master;
