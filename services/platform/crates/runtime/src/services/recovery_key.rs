//! Administrator recovery-key file handling.
//!
//! The file is the single plaintext source of truth. PostgreSQL stores only an
//! Argon2id hash, so a database dump alone cannot recover the account.

use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;

use data_encoding::BASE32_NOPAD;
use ring::rand::{SecureRandom, SystemRandom};

use crate::config::AdminRecoveryKeyMode;
use crate::error::ApiError;

const FILE_NAME: &str = "recovery-key";
const KEY_BYTES: usize = 32;

pub const GENERATED_ORIGIN: &str = "generated";
pub const PROVIDED_ORIGIN: &str = "provided";

#[derive(Debug, Clone)]
pub struct ResolvedRecoveryKey {
    pub key: String,
    pub origin: &'static str,
}

pub fn recovery_key_path(dir: &str) -> PathBuf {
    PathBuf::from(dir).join(FILE_NAME)
}

fn generate() -> Result<String, ApiError> {
    let mut raw = [0_u8; KEY_BYTES];
    SystemRandom::new()
        .fill(&mut raw)
        .map_err(|_| ApiError::internal("Recovery key generation failed."))?;
    Ok(BASE32_NOPAD.encode(&raw))
}

fn validate(value: &str) -> Result<String, ApiError> {
    let trimmed = value.trim();
    let decoded = BASE32_NOPAD.decode(trimmed.as_bytes()).map_err(|_| {
        ApiError::service_unavailable("The administrator recovery key file is invalid.")
    })?;
    if decoded.len() != KEY_BYTES || decoded.iter().all(|byte| *byte == 0) {
        return Err(ApiError::service_unavailable(
            "The administrator recovery key file is invalid.",
        ));
    }
    Ok(trimmed.to_owned())
}

/// Reads the key file, returning `None` when it is absent or empty. Any other
/// read failure or invalid content is fatal: the deployment must never start
/// with a recovery key it cannot verify.
pub fn read_existing(dir: &str) -> Result<Option<String>, ApiError> {
    match fs::read_to_string(recovery_key_path(dir)) {
        Ok(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                validate(trimmed).map(Some)
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(ApiError::service_unavailable(
            "The administrator recovery key file could not be read.",
        )),
    }
}

fn write(dir: &str, key: &str) -> Result<(), ApiError> {
    fs::create_dir_all(dir).map_err(|_| {
        ApiError::service_unavailable("The administrator recovery key directory is not writable.")
    })?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(0o600)
        .open(recovery_key_path(dir))
        .map_err(|_| {
            ApiError::service_unavailable(
                "The administrator recovery key file is not writable by the runtime user.",
            )
        })?;
    writeln!(file, "{key}").map_err(|_| {
        ApiError::service_unavailable("The administrator recovery key file could not be written.")
    })?;
    Ok(())
}

pub fn resolve(dir: &str, mode: AdminRecoveryKeyMode) -> Result<ResolvedRecoveryKey, ApiError> {
    let existing = read_existing(dir)?;
    match (mode, existing) {
        (AdminRecoveryKeyMode::Load, Some(key)) => Ok(ResolvedRecoveryKey {
            key,
            origin: PROVIDED_ORIGIN,
        }),
        (AdminRecoveryKeyMode::Load, None) => Err(ApiError::service_unavailable(
            "AIRTEK_ADMIN_RECOVERY_KEY_MODE=load requires an existing, valid recovery key file.",
        )),
        (AdminRecoveryKeyMode::Auto, Some(key)) => Ok(ResolvedRecoveryKey {
            key,
            origin: PROVIDED_ORIGIN,
        }),
        (AdminRecoveryKeyMode::Auto | AdminRecoveryKeyMode::Generate, _) => {
            let key = generate()?;
            write(dir, &key)?;
            Ok(ResolvedRecoveryKey {
                key,
                origin: GENERATED_ORIGIN,
            })
        }
    }
}

/// Explicit rotation used after a recovery-key login and by the maintenance
/// command. The new key is always written to the file and reported as
/// `generated` so the GUI shows it once for physical storage.
pub fn rotate(dir: &str) -> Result<ResolvedRecoveryKey, ApiError> {
    resolve(dir, AdminRecoveryKeyMode::Generate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn scratch(label: &str) -> PathBuf {
        env::temp_dir().join(format!("airtek-recovery-{label}-{}", std::process::id()))
    }

    #[test]
    fn auto_generates_once_then_reuses_the_file() {
        let dir = scratch("auto");
        let _ = fs::remove_dir_all(&dir);
        let path = dir.to_str().expect("utf8 temp path");
        let first = resolve(path, AdminRecoveryKeyMode::Auto).unwrap();
        assert_eq!(first.origin, GENERATED_ORIGIN);
        let second = resolve(path, AdminRecoveryKeyMode::Auto).unwrap();
        assert_eq!(second.key, first.key);
        assert_eq!(second.origin, PROVIDED_ORIGIN);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn load_requires_an_existing_valid_file() {
        let dir = scratch("load");
        let _ = fs::remove_dir_all(&dir);
        assert!(resolve(dir.to_str().unwrap(), AdminRecoveryKeyMode::Load).is_err());
    }

    #[test]
    fn invalid_content_is_rejected() {
        let dir = scratch("invalid");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(recovery_key_path(dir.to_str().unwrap()), "not-a-key\n").unwrap();
        assert!(read_existing(dir.to_str().unwrap()).is_err());
        fs::remove_dir_all(&dir).unwrap();
    }
}
