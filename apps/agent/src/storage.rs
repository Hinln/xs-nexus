use std::path::{Path, PathBuf};

use ed25519_dalek::SigningKey;
use getrandom::fill;
use serde::{Serialize, de::DeserializeOwned};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::error::{AgentError, Result};

#[cfg(unix)]
mod unix;
#[cfg(unix)]
use unix as platform;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as platform;

const MAX_STATE_BYTES: u64 = 512 * 1024;

pub struct Identity {
    signing_key: SigningKey,
}

impl Identity {
    /// Loads an existing restricted Ed25519 seed file without creating state.
    ///
    /// # Errors
    ///
    /// Returns [`AgentError::State`] when the file is absent, unsafe, or invalid.
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = Zeroizing::new(platform::read_private(path, 32)?);
        let seed: &[u8; 32] = bytes.as_slice().try_into().map_err(|_| AgentError::State)?;
        Ok(Self {
            signing_key: SigningKey::from_bytes(seed),
        })
    }

    /// Loads a restricted Ed25519 seed file or atomically creates one.
    ///
    /// # Errors
    ///
    /// Returns [`AgentError::State`] when paths, permissions, randomness, or I/O are unsafe.
    pub fn load_or_create(path: &Path) -> Result<Self> {
        let parent = path.parent().ok_or(AgentError::State)?;
        ensure_private_directory(parent)?;
        match path.symlink_metadata() {
            Ok(_) => Self::load(path),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let mut seed = Zeroizing::new([0_u8; 32]);
                fill(seed.as_mut()).map_err(|_| AgentError::State)?;
                write_atomic(path, seed.as_ref())?;
                Ok(Self {
                    signing_key: SigningKey::from_bytes(&seed),
                })
            }
            Err(_) => Err(AgentError::State),
        }
    }

    #[must_use]
    pub const fn signing_key(&self) -> &SigningKey {
        &self.signing_key
    }

    #[must_use]
    pub fn public_key(&self) -> [u8; 32] {
        self.signing_key.verifying_key().to_bytes()
    }
}

/// Creates or restricts a private state directory.
///
/// # Errors
///
/// Returns [`AgentError::State`] when the path is not a real directory or permissions fail.
pub fn ensure_private_directory(path: &Path) -> Result<()> {
    platform::ensure_private_directory(path)
}

/// Serializes and atomically replaces a bounded private JSON state file.
///
/// # Errors
///
/// Returns [`AgentError::State`] when serialization, size validation, or persistence fails.
pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec(value).map_err(|_| AgentError::State)?;
    if bytes.len() > usize::try_from(MAX_STATE_BYTES).map_err(|_| AgentError::State)? {
        return Err(AgentError::State);
    }
    write_atomic(path, &bytes)
}

/// Reads a bounded private JSON state file.
///
/// # Errors
///
/// Returns [`AgentError::State`] when the file is unsafe, oversized, unreadable, or invalid.
pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let bytes = platform::read_private(path, MAX_STATE_BYTES)?;
    serde_json::from_slice(&bytes).map_err(|_| AgentError::State)
}

/// Reads a bounded enrollment token from a restricted regular file.
///
/// # Errors
///
/// Returns [`AgentError::State`] when permissions, size, encoding, or token syntax are invalid.
pub fn read_token(path: &Path) -> Result<Zeroizing<String>> {
    let bytes = platform::read_private(path, 512)?;
    if bytes.len() < 16 {
        return Err(AgentError::State);
    }
    let token = Zeroizing::new(String::from_utf8(bytes).map_err(|_| AgentError::State)?);
    let trimmed = token.trim();
    if trimmed.len() < 16 || trimmed.len() > 256 || trimmed.chars().any(char::is_whitespace) {
        return Err(AgentError::State);
    }
    Ok(Zeroizing::new(trimmed.to_owned()))
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or(AgentError::State)?;
    ensure_private_directory(parent)?;
    let temporary = temporary_path(path);
    platform::write_private_atomic(path, &temporary, bytes)
}

fn temporary_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("state");
    path.with_file_name(format!(".{file_name}.{}.tmp", Uuid::new_v4()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt as _;

    #[test]
    fn identity_is_stable_and_restricted() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("state/identity.key");
        let first = Identity::load_or_create(&path).expect("create identity");
        let second = Identity::load_or_create(&path).expect("load identity");
        assert_eq!(first.public_key(), second.public_key());
        #[cfg(unix)]
        {
            let mode = path.metadata().expect("metadata").permissions().mode();
            assert_eq!(mode & 0o777, unix::PRIVATE_MODE);
        }
    }

    #[cfg(unix)]
    #[test]
    fn identity_rejects_group_readable_file() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("identity.key");
        std::fs::write(&path, [7_u8; 32]).expect("write key");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640))
            .expect("set permissions");
        assert!(Identity::load_or_create(&path).is_err());
    }

    #[test]
    fn strict_identity_load_never_creates_missing_state() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("missing/identity.key");

        assert!(Identity::load(&path).is_err());
        assert!(!path.exists());
        assert!(!path.parent().expect("parent").exists());
    }
}
