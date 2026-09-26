#![forbid(unsafe_code)]

#[cfg(all(feature = "production", feature = "devtools"))]
compile_error!("the `production` and `devtools` features are mutually exclusive");

#[cfg(feature = "devtools")]
pub mod devtools;
pub mod openapi;
pub mod routes;
pub use routes::build_router;
