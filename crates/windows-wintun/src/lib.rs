#![deny(unsafe_code)]

//! Minimal, audited binding to the official Wintun user-mode API.
//!
//! This crate never implements a VPN protocol or a Windows driver. It dynamically loads a
//! caller-pinned copy of the vendor-signed `wintun.dll`, creates an ephemeral layer-three
//! adapter, and copies bounded IPv4 packets across its session ring. XS Nexus owns all control,
//! authentication, encryption, routing policy, and packet validation above this boundary.

use std::{
    fs::File,
    io::{self, Read},
    path::Path,
};

use sha2::{Digest as _, Sha256};
use thiserror::Error;

pub const MIN_RING_CAPACITY: u32 = 0x20000;
pub const MAX_RING_CAPACITY: u32 = 0x0400_0000;
pub const DEFAULT_RING_CAPACITY: u32 = 0x0040_0000;
pub const MAX_PACKET_SIZE: usize = 65_535;
const MAX_LIBRARY_SIZE: u64 = 32 * 1024 * 1024;

#[derive(Debug, Error, Eq, PartialEq)]
pub enum WintunError {
    #[error("Wintun is only available on Windows")]
    UnsupportedPlatform,
    #[error("Wintun library path is not an absolute regular file")]
    InvalidLibraryPath,
    #[error("Wintun library exceeds the maximum accepted size")]
    LibraryTooLarge,
    #[error("Wintun library hash does not match the pinned release manifest")]
    LibraryHashMismatch,
    #[error("Wintun adapter name or tunnel type is invalid")]
    InvalidAdapterName,
    #[error("Wintun ring capacity is invalid")]
    InvalidRingCapacity,
    #[error("Wintun packet is invalid")]
    InvalidPacket,
    #[error("Wintun API call failed with Win32 status {0}")]
    Win32(u32),
}

/// Verifies that a checked-in release manifest may refer to this exact library.
///
/// # Errors
///
/// Rejects relative paths, symlinks, oversized files, I/O failures, and any non-matching hash.
pub fn verify_library(path: &Path, expected_sha256: &[u8; 32]) -> Result<(), WintunError> {
    if !path.is_absolute() {
        return Err(WintunError::InvalidLibraryPath);
    }
    let metadata = std::fs::symlink_metadata(path).map_err(|_| WintunError::InvalidLibraryPath)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(WintunError::InvalidLibraryPath);
    }
    if metadata.len() > MAX_LIBRARY_SIZE {
        return Err(WintunError::LibraryTooLarge);
    }
    let actual = file_sha256(path).map_err(|_| WintunError::InvalidLibraryPath)?;
    if actual != *expected_sha256 {
        return Err(WintunError::LibraryHashMismatch);
    }
    Ok(())
}

/// Parses exactly one canonical lowercase SHA-256 value from a release configuration.
///
/// # Errors
///
/// Rejects non-hexadecimal, upper-case, or incorrectly sized values.
pub fn parse_sha256(value: &str) -> Result<[u8; 32], WintunError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(WintunError::LibraryHashMismatch);
    }
    let mut hash = [0_u8; 32];
    for (index, destination) in hash.iter_mut().enumerate() {
        let offset = index * 2;
        let high = hex_value(value.as_bytes()[offset]).ok_or(WintunError::LibraryHashMismatch)?;
        let low =
            hex_value(value.as_bytes()[offset + 1]).ok_or(WintunError::LibraryHashMismatch)?;
        *destination = (high << 4) | low;
    }
    Ok(hash)
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    }
}

fn file_sha256(path: &Path) -> io::Result<[u8; 32]> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
    }
    Ok(hash.finalize().into())
}

#[cfg(windows)]
#[allow(unsafe_code)]
mod platform;

#[cfg(windows)]
pub use platform::WintunSession;

#[cfg(not(windows))]
#[derive(Debug)]
pub struct WintunSession;

#[cfg(not(windows))]
impl WintunSession {
    /// Rejects Wintun session creation on non-Windows platforms.
    ///
    /// # Errors
    ///
    /// Always returns [`WintunError::UnsupportedPlatform`].
    pub fn create(
        _library_path: &Path,
        _expected_sha256: &[u8; 32],
        _adapter_name: &str,
        _tunnel_type: &str,
        _ring_capacity: u32,
    ) -> Result<Self, WintunError> {
        Err(WintunError::UnsupportedPlatform)
    }

    #[must_use]
    pub const fn interface_luid(&self) -> u64 {
        0
    }

    #[must_use]
    pub const fn interface_index(&self) -> u32 {
        0
    }

    /// # Errors
    ///
    /// Always returns [`WintunError::UnsupportedPlatform`].
    pub fn try_receive(&self, _buffer: &mut [u8]) -> Result<Option<usize>, WintunError> {
        Err(WintunError::UnsupportedPlatform)
    }

    /// # Errors
    ///
    /// Always returns [`WintunError::UnsupportedPlatform`].
    pub fn send(&self, _packet: &[u8]) -> Result<(), WintunError> {
        Err(WintunError::UnsupportedPlatform)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_RING_CAPACITY, MAX_RING_CAPACITY, MIN_RING_CAPACITY, WintunError, parse_sha256,
    };

    #[test]
    fn sha256_parser_requires_canonical_lowercase_hex() {
        assert_eq!(parse_sha256("00".repeat(32).as_str()), Ok([0_u8; 32]));
        assert_eq!(
            parse_sha256(&format!("{}A", "00".repeat(31))),
            Err(WintunError::LibraryHashMismatch)
        );
        assert_eq!(
            parse_sha256("0".repeat(63).as_str()),
            Err(WintunError::LibraryHashMismatch)
        );
    }

    #[test]
    fn default_ring_capacity_is_a_supported_power_of_two() {
        assert!((MIN_RING_CAPACITY..=MAX_RING_CAPACITY).contains(&DEFAULT_RING_CAPACITY));
        assert!(DEFAULT_RING_CAPACITY.is_power_of_two());
    }
}
