//! OpenAPI path assembly, ordered to mirror the runtime router surface.

mod admin_analytics;
mod admin_auth;
mod admin_catalog;
mod admin_content;
mod admin_identity;
mod admin_operations;
#[cfg(feature = "devtools")]
mod devtools;
mod media;
mod public;

use serde_json::{Map, Value};

pub(super) fn add_all(paths: &mut Map<String, Value>) {
    public::add_core(paths);
    public::add_data(paths);

    admin_auth::add_paths(paths);
    admin_content::add_paths(paths);
    media::add_paths(paths);
    admin_catalog::add_publication_and_sync(paths);
    admin_analytics::add_overview(paths);

    admin_catalog::add_management(paths);
    admin_analytics::add_reports(paths);
    admin_identity::add_paths(paths);

    admin_operations::add_paths(paths);

    #[cfg(feature = "devtools")]
    devtools::add_paths(paths);
}
