include!("cms_v2/core.rs");
include!("cms_v2/blocks.rs");
include!("cms_v2/fields.rs");
include!("cms_v2/migration.rs");
include!("cms_v2/public.rs");

#[cfg(test)]
mod cms_v2_tests {
    include!("cms_v2_tests.rs");
}
