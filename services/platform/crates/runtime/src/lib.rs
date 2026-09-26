#![forbid(unsafe_code)]

#[cfg(all(feature = "production", feature = "devtools"))]
compile_error!("the `production` and `devtools` features are mutually exclusive");

pub mod auth;
pub mod config;
pub mod error;
pub mod flyway;
pub mod idempotency;
pub mod pagination;
pub mod rate_limit;
pub mod second_factor;
pub mod services;
pub mod state;
pub use config::Config;
pub use error::{ApiError, ProblemDetails};
pub use state::AppState;
