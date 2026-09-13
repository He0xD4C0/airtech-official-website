#[cfg(feature = "devtools")]
include!("airtekctl/imports.rs");
#[cfg(feature = "devtools")]
include!("airtekctl/cli.rs");
#[cfg(feature = "devtools")]
include!("airtekctl/main.rs");
#[cfg(feature = "devtools")]
include!("airtekctl/diagnostics_operations.rs");
#[cfg(feature = "devtools")]
include!("airtekctl/jobs.rs");
#[cfg(feature = "devtools")]
include!("airtekctl/validation.rs");
#[cfg(feature = "devtools")]
include!("airtekctl/support.rs");
#[cfg(feature = "devtools")]
include!("airtekctl/tests.rs");

#[cfg(not(feature = "devtools"))]
include!("airtekctl/operations.rs");
