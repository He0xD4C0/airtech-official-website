#[path = "auth/imports.rs"]
mod imports;
use imports::*;
#[path = "auth/types.rs"]
mod types;
use types::*;
pub use types::*;
#[path = "auth/bootstrap.rs"]
mod bootstrap;
pub use bootstrap::{accept_invitation, setup};
#[path = "auth/login_sessions.rs"]
mod login_sessions;
pub use login_sessions::*;
#[path = "auth/password_reset.rs"]
mod password_reset;
pub use password_reset::*;
#[path = "auth/authentication.rs"]
mod authentication;
use authentication::*;
pub use authentication::*;
#[path = "auth/second_factor.rs"]
mod second_factor;
pub use second_factor::verify_totp_reauthentication;
use second_factor::*;
#[path = "auth/identity.rs"]
mod identity;
pub use identity::verify_csrf;
use identity::*;
#[path = "auth/session_store.rs"]
mod session_store;
use session_store::*;
#[path = "auth/rate_limits.rs"]
mod rate_limits;
use rate_limits::*;
#[path = "auth/permissions.rs"]
mod permissions;
use permissions::*;
pub use permissions::*;
/// Public wrappers so account provisioning and recovery-key handling can reuse
/// the same Argon2id parameters without duplicating password hashing.
pub fn hash_secret(value: &str) -> Result<String, ApiError> {
    hash_password(value)
}
pub fn verify_secret(encoded: &str, value: &str) -> bool {
    verify_password(encoded, value)
}
pub fn validate_password_strength(value: &str) -> Result<(), ApiError> {
    validate_strong_password(value)
}
#[cfg(feature = "devtools")]
pub fn development_hash_password(password: &str) -> Result<String, ApiError> {
    hash_password(password)
}
#[cfg(feature = "devtools")]
pub fn development_validate_password(password: &str) -> Result<(), ApiError> {
    validate_strong_password(password)
}
#[cfg(test)]
#[path = "auth/tests.rs"]
mod tests;
