use std::path::Path;

use crate::error::{AgentError, Result};

pub(super) fn ensure_private_directory(path: &Path) -> Result<()> {
    xs_windows_private_storage::ensure_private_directory(path).map_err(|_| AgentError::State)
}

pub(super) fn read_private(path: &Path, maximum_bytes: u64) -> Result<Vec<u8>> {
    xs_windows_private_storage::read_private(path, maximum_bytes).map_err(|_| AgentError::State)
}

pub(super) fn write_private_atomic(path: &Path, temporary: &Path, bytes: &[u8]) -> Result<()> {
    xs_windows_private_storage::write_private_atomic(path, temporary, bytes)
        .map_err(|_| AgentError::State)
}
