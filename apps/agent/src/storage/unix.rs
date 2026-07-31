use std::{
    fs::{File, OpenOptions},
    io::{Read as _, Write as _},
    os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _},
    path::Path,
};

use crate::error::{AgentError, Result};

pub(super) const PRIVATE_MODE: u32 = 0o600;
pub(super) const DIRECTORY_MODE: u32 = 0o700;

pub(super) fn ensure_private_directory(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path).map_err(|_| AgentError::State)?;
    let metadata = path.symlink_metadata().map_err(|_| AgentError::State)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(AgentError::State);
    }
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(DIRECTORY_MODE))
        .map_err(|_| AgentError::State)
}

pub(super) fn read_private(path: &Path, maximum_bytes: u64) -> Result<Vec<u8>> {
    let metadata = path.symlink_metadata().map_err(|_| AgentError::State)?;
    if !metadata.is_file()
        || metadata.permissions().mode() & 0o077 != 0
        || metadata.len() > maximum_bytes
    {
        return Err(AgentError::State);
    }
    let mut file = File::open(path).map_err(|_| AgentError::State)?;
    let capacity = usize::try_from(metadata.len()).map_err(|_| AgentError::State)?;
    let mut bytes = Vec::with_capacity(capacity);
    file.read_to_end(&mut bytes)
        .map_err(|_| AgentError::State)?;
    if u64::try_from(bytes.len()) != Ok(metadata.len()) {
        return Err(AgentError::State);
    }
    Ok(bytes)
}

pub(super) fn write_private_atomic(path: &Path, temporary: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or(AgentError::State)?;
    ensure_private_directory(parent)?;
    if path
        .symlink_metadata()
        .is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        return Err(AgentError::State);
    }

    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(PRIVATE_MODE)
            .open(temporary)
            .map_err(|_| AgentError::State)?;
        file.write_all(bytes).map_err(|_| AgentError::State)?;
        file.sync_all().map_err(|_| AgentError::State)?;
        std::fs::rename(temporary, path).map_err(|_| AgentError::State)?;
        File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| AgentError::State)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    result
}
