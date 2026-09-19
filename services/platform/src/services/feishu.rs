#[path = "feishu/imports.rs"]
mod imports;
use imports::*;
use uuid::Uuid;
pub(super) const CONNECTOR_ID: Uuid = Uuid::from_u128(0x63e3d923632a4e47a31b16f3ec81690e);
#[path = "feishu/validation.rs"]
mod validation;
pub use validation::validate_staging_payload;
#[path = "feishu/specifications.rs"]
mod specifications;
use specifications::*;
#[path = "feishu/curves.rs"]
mod curves;
use curves::*;
#[path = "feishu/validation_support.rs"]
mod validation_support;
use validation_support::*;
#[path = "feishu/client.rs"]
mod client;
pub use client::{FeishuClient, FeishuField, FeishuRecord, FeishuRecordPage};
#[path = "feishu/credentials.rs"]
mod credentials;
pub use credentials::{credentials_configured, load_client};
#[path = "feishu/mapping.rs"]
mod mapping;
pub use mapping::*;
#[path = "feishu/settings.rs"]
mod settings;
pub use settings::*;
#[path = "feishu/normalization.rs"]
mod normalization;
#[path = "feishu/settings_validation.rs"]
mod settings_validation;
pub use normalization::*;
#[path = "feishu/assets.rs"]
mod assets;
pub use assets::*;
#[path = "feishu/asset_cleanup.rs"]
mod asset_cleanup;
pub use asset_cleanup::*;
#[path = "feishu/deletion.rs"]
mod deletion;
pub use deletion::*;
#[path = "feishu/promotion.rs"]
mod promotion;
pub use promotion::*;
#[path = "feishu/promotion_storage.rs"]
mod promotion_storage;
#[path = "feishu/staging.rs"]
mod staging;
pub use staging::*;
#[path = "feishu/cursor.rs"]
mod cursor;
pub use cursor::*;
#[path = "feishu/queue.rs"]
mod queue;
pub use queue::*;
#[path = "feishu/runner.rs"]
mod runner;
pub use runner::*;
#[path = "feishu/runner_support.rs"]
mod runner_support;
#[cfg(test)]
#[path = "feishu/tests.rs"]
mod tests;
