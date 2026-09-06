use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::models::LinkTargetReference;

use super::conversion::IssueContext;

pub(super) fn link_target(value: &str) -> Option<LinkTargetReference> {
    if value.starts_with('/')
        && !value.starts_with("//")
        && !value.contains(['\\', '\r', '\n', '\0'])
    {
        return Some(LinkTargetReference::Route { path: value.into() });
    }
    if value.starts_with("https://")
        && !value[8..].contains('@')
        && !value.chars().any(char::is_control)
    {
        return Some(LinkTargetReference::External { url: value.into() });
    }
    None
}

pub(super) fn stable_id(context: IssueContext, path: &str) -> Uuid {
    let mut digest = Sha256::new();
    digest.update(b"airtek-cms-v2\0");
    digest.update(context.entity_id.as_bytes());
    digest.update(context.revision.to_be_bytes());
    digest.update(path.as_bytes());
    let hash = digest.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&hash[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x80;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}
