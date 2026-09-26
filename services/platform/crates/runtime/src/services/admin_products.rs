#[path = "admin_products/lists.rs"]
mod lists;
#[path = "admin_products/loading.rs"]
mod loading;
#[path = "admin_products/presentation.rs"]
mod presentation;

pub use lists::*;
pub use loading::*;
pub use presentation::*;
