use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

use ed25519_dalek::SigningKey;
use getrandom::fill;
use serde::{Serialize, de::DeserializeOwned};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::error::{AgentError, Result};

const MAX_STATE_BYTES: u64 = 512 * 1024;
const PRIVATE_MODE: u32 = 0o600;
const DIRECTORY_MODE: u32 = 0o700;

pub struct Identity {
    signing_key: SigningKey,
}

impl Identity {
    /// Loads a restricted Ed25519 seed file or atomically creates one.
    ///
    /// # Errors
    ///
    /// Returns [`AgentError::State`] when paths, permissions, randomness, or I/O are unsafe.
    pub fn load_or_create(path: &Path) -> Result<Self> {
        let parent = path.parent().ok_or(AgentError::State)?;
        ensure_private_directory(parent)?;
        match path.symlink_metadata() {
            Ok(metadata) => {
                if !metadata.is_file() || metadata.permissions().mode() & 0o077 != 0 {
                    return Err(AgentError::State);
                }
                let bytes = Zeroizing::new(std::fs::read(path).map_err(|_| AgentError::State)?);
                let seed: &[u8; 32] = bytes.as_slice().try_into().map_err(|_| AgentError::State)?;
                Ok(Self {
                    signing_key: SigningKey::from_bytes(seed),
                })
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let mut seed = Zeroizing::new([0_u8; 32]);
                fill(seed.as_mut()).map_err(|_| AgentError::State)?;
                write_atomic(path, seed.as_ref(), PRIVATE_MODE)?;
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
    std::fs::create_dir_all(path).map_err(|_| AgentError::State)?;
    let metadata = path.symlink_metadata().map_err(|_| AgentError::State)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(AgentError::State);
    }
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(DIRECTORY_MODE))
        .map_err(|_| AgentError::State)
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
    write_atomic(path, &bytes, PRIVATE_MODE)
}

/// Reads a bounded private JSON state file.
///
/// # Errors
///
/// Returns [`AgentError::State`] when the file is unsafe, oversized, unreadable, or invalid.
pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let metadata = path.symlink_metadata().map_err(|_| AgentError::State)?;
    if !metadata.is_file()
        || metadata.permissions().mode() & 0o077 != 0
        || metadata.len() > MAX_STATE_BYTES
    {
        return Err(AgentError::State);
    }
    let mut file = File::open(path).map_err(|_| AgentError::State)?;
    let capacity = usize::try_from(metadata.len()).map_err(|_| AgentError::State)?;
    let mut bytes = Vec::with_capacity(capacity);
    file.read_to_end(&mut bytes)
        .map_err(|_| AgentError::State)?;
    serde_json::from_slice(&bytes).map_err(|_| AgentError::State)
}

/// Reads a bounded enrollment token from a restricted regular file.
///
/// # Errors
///
/// Returns [`AgentError::State`] when permissions, size, encoding, or token syntax are invalid.
pub fn read_token(path: &Path) -> Result<Zeroizing<String>> {
    let metadata = path.symlink_metadata().map_err(|_| AgentError::State)?;
    if !metadata.is_file()
        || metadata.permissions().mode() & 0o077 != 0
        || !(16..=512).contains(&metadata.len())
    {
        return Err(AgentError::State);
    }
    let token = Zeroizing::new(std::fs::read_to_string(path).map_err(|_| AgentError::State)?);
    let trimmed = token.trim();
    if trimmed.len() < 16 || trimmed.len() > 256 || trimmed.chars().any(char::is_whitespace) {
        return Err(AgentError::State);
    }
    Ok(Zeroizing::new(trimmed.to_owned()))
}

fn write_atomic(path: &Path, bytes: &[u8], mode: u32) -> Result<()> {
    let parent = path.parent().ok_or(AgentError::State)?;
    ensure_private_directory(parent)?;
    if path
        .symlink_metadata()
        .is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        return Err(AgentError::State);
    }

    let temporary = temporary_path(path);
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(mode)
            .open(&temporary)
            .map_err(|_| AgentError::State)?;
        file.write_all(bytes).map_err(|_| AgentError::State)?;
        file.sync_all().map_err(|_| AgentError::State)?;
        std::fs::rename(&temporary, path).map_err(|_| AgentError::State)?;
        File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| AgentError::State)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
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

    #[test]
    fn identity_is_stable_and_restricted() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("state/identity.key");
        let first = Identity::load_or_create(&path).expect("create identity");
        let second = Identity::load_or_create(&path).expect("load identity");
        assert_eq!(first.public_key(), second.public_key());
        let mode = path.metadata().expect("metadata").permissions().mode();
        assert_eq!(mode & 0o777, PRIVATE_MODE);
    }

    #[test]
    fn identity_rejects_group_readable_file() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("identity.key");
        std::fs::write(&path, [7_u8; 32]).expect("write key");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640))
            .expect("set permissions");
        assert!(Identity::load_or_create(&path).is_err());
    }
}
