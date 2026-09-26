use super::*;

#[path = "cms_v2/core.rs"]
mod core;
pub use core::*;
#[path = "cms_v2/blocks.rs"]
mod blocks;
pub use blocks::*;
#[path = "cms_v2/fields.rs"]
mod fields;
pub use fields::*;
#[path = "cms_v2/public.rs"]
mod public;
pub use public::*;

#[cfg(test)]
#[path = "cms_v2_tests.rs"]
mod cms_v2_tests;
