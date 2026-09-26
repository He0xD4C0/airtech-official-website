#![forbid(unsafe_code)]

#[cfg(all(feature = "production", feature = "devtools"))]
compile_error!("the `production` and `devtools` features are mutually exclusive");

pub mod worker;
