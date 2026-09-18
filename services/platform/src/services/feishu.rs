#[path = "feishu/imports.rs"]
mod imports;
use imports::*;
#[path = "feishu/diff.rs"]
mod diff;
pub use diff::*;
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
#[path = "feishu/flatten.rs"]
mod flatten;
use flatten::*;
#[path = "feishu/client.rs"]
mod client;
pub use client::{FeishuClient, FeishuField, FeishuRecord, FeishuRecordPage};
#[path = "feishu/mapping.rs"]
mod mapping;
pub use mapping::*;
#[path = "feishu/settings.rs"]
mod settings;
pub use settings::*;
#[path = "feishu/normalization.rs"]
mod normalization;
pub use normalization::*;
#[path = "feishu/assets.rs"]
mod assets;
pub use assets::*;
#[path = "feishu/asset_cleanup.rs"]
mod asset_cleanup;
pub use asset_cleanup::*;
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
#[path = "feishu/rollback.rs"]
mod rollback;
#[path = "feishu/runner_support.rs"]
mod runner_support;
pub use rollback::*;
#[cfg(test)]
#[path = "feishu/tests.rs"]
mod tests;
