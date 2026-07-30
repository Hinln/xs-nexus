use std::{env, net::SocketAddr, path::Path};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::{SigningKey, VerifyingKey};
use thiserror::Error;
use xs_protocol::{RELAY_MAX_FRAME_LENGTH, RELAY_MAX_LEASE_SECONDS};
use zeroize::Zeroizing;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

pub struct RelayConfig {
    pub listen: SocketAddr,
    pub health_listen: SocketAddr,
    pub relay_id: [u8; 16],
    pub controller_credential_key: VerifyingKey,
    pub identity_key: SigningKey,
    pub lease_ttl_seconds: u64,
    pub idle_timeout_seconds: u64,
    pub max_leases: usize,
    pub registration_requests_per_minute: u16,
    pub packets_per_lease_per_second: u32,
    pub bytes_per_lease_per_second: u64,
    pub queue_packets_per_node: usize,
    pub queue_bytes_per_node: usize,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("missing required environment variable {0}")]
    Missing(&'static str),
    #[error("invalid Relay listener")]
    Listen,
    #[error("invalid Relay ID")]
    RelayId,
    #[error("unable to inspect key file")]
    KeyMetadata,
    #[error("key path must be a regular non-symlink file")]
    KeyType,
    #[error("private key file permissions must not grant group or other access")]
    PrivateKeyPermissions,
    #[error("public key file must not be writable by group or other")]
    PublicKeyPermissions,
    #[error("unable to read key file")]
    KeyRead,
    #[error("key file must contain exactly 32 raw bytes")]
    KeyLength,
    #[error("invalid Relay resource limit {0}")]
    Limit(&'static str),
}

impl RelayConfig {
    /// Loads strict Relay listeners, trust roots, identity, and resource limits.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] when a required value, key file, permission, or bound is invalid.
    pub fn from_env() -> Result<Self, ConfigError> {
        let listen = socket("RELAY_LISTEN")?;
        let health_listen = socket("RELAY_HEALTH_LISTEN")?;
        let relay_id = decode_relay_id(&required("RELAY_ID_BASE64")?)?;
        let controller_credential_key = load_verifying_key(Path::new(&required(
            "CONTROLLER_CREDENTIAL_PUBLIC_KEY_PATH",
        )?))?;
        let identity_key = load_signing_key(Path::new(&required("RELAY_IDENTITY_KEY_PATH")?))?;
        let lease_ttl_seconds =
            bounded_u64("RELAY_LEASE_TTL_SECONDS", 120, 30, RELAY_MAX_LEASE_SECONDS)?;
        let idle_timeout_seconds =
            bounded_u64("RELAY_IDLE_TIMEOUT_SECONDS", 60, 15, lease_ttl_seconds)?;
        let max_leases = bounded_usize("RELAY_MAX_LEASES", 4096, 1, 65_536)?;
        let registration_requests_per_minute = u16::try_from(bounded_u64(
            "RELAY_REGISTRATIONS_PER_SOURCE_PER_MINUTE",
            30,
            1,
            600,
        )?)
        .map_err(|_| ConfigError::Limit("RELAY_REGISTRATIONS_PER_SOURCE_PER_MINUTE"))?;
        let packets_per_lease_per_second = u32::try_from(bounded_u64(
            "RELAY_PACKETS_PER_LEASE_PER_SECOND",
            2_000,
            1,
            1_000_000,
        )?)
        .map_err(|_| ConfigError::Limit("RELAY_PACKETS_PER_LEASE_PER_SECOND"))?;
        let bytes_per_lease_per_second = bounded_u64(
            "RELAY_BYTES_PER_LEASE_PER_SECOND",
            16 * 1024 * 1024,
            1_500,
            10 * 1024 * 1024 * 1024,
        )?;
        let queue_packets_per_node = bounded_usize("RELAY_QUEUE_PACKETS_PER_NODE", 64, 1, 1024)?;
        let queue_bytes_per_node = bounded_usize(
            "RELAY_QUEUE_BYTES_PER_NODE",
            128 * 1024,
            RELAY_MAX_FRAME_LENGTH,
            64 * 1024 * 1024,
        )?;
        if queue_bytes_per_node < queue_packets_per_node {
            return Err(ConfigError::Limit("RELAY_QUEUE_BYTES_PER_NODE"));
        }
        Ok(Self {
            listen,
            health_listen,
            relay_id,
            controller_credential_key,
            identity_key,
            lease_ttl_seconds,
            idle_timeout_seconds,
            max_leases,
            registration_requests_per_minute,
            packets_per_lease_per_second,
            bytes_per_lease_per_second,
            queue_packets_per_node,
            queue_bytes_per_node,
        })
    }
}

fn required(name: &'static str) -> Result<String, ConfigError> {
    env::var(name).map_err(|_| ConfigError::Missing(name))
}

fn socket(name: &'static str) -> Result<SocketAddr, ConfigError> {
    let address = required(name)?
        .parse::<SocketAddr>()
        .map_err(|_| ConfigError::Listen)?;
    if address.port() == 0 {
        return Err(ConfigError::Listen);
    }
    Ok(address)
}

fn decode_relay_id(encoded: &str) -> Result<[u8; 16], ConfigError> {
    let decoded = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| ConfigError::RelayId)?;
    let relay_id: [u8; 16] = decoded.try_into().map_err(|_| ConfigError::RelayId)?;
    if relay_id == [0_u8; 16] {
        return Err(ConfigError::RelayId);
    }
    Ok(relay_id)
}

fn bounded_u64(
    name: &'static str,
    default: u64,
    minimum: u64,
    maximum: u64,
) -> Result<u64, ConfigError> {
    let value = env::var(name)
        .unwrap_or_else(|_| default.to_string())
        .parse::<u64>()
        .map_err(|_| ConfigError::Limit(name))?;
    if (minimum..=maximum).contains(&value) {
        Ok(value)
    } else {
        Err(ConfigError::Limit(name))
    }
}

fn bounded_usize(
    name: &'static str,
    default: usize,
    minimum: usize,
    maximum: usize,
) -> Result<usize, ConfigError> {
    let value = env::var(name)
        .unwrap_or_else(|_| default.to_string())
        .parse::<usize>()
        .map_err(|_| ConfigError::Limit(name))?;
    if (minimum..=maximum).contains(&value) {
        Ok(value)
    } else {
        Err(ConfigError::Limit(name))
    }
}

fn load_signing_key(path: &Path) -> Result<SigningKey, ConfigError> {
    validate_key_file(path, true)?;
    let bytes = Zeroizing::new(std::fs::read(path).map_err(|_| ConfigError::KeyRead)?);
    let seed: &[u8; 32] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| ConfigError::KeyLength)?;
    Ok(SigningKey::from_bytes(seed))
}

fn load_verifying_key(path: &Path) -> Result<VerifyingKey, ConfigError> {
    validate_key_file(path, false)?;
    let bytes = std::fs::read(path).map_err(|_| ConfigError::KeyRead)?;
    let encoded: &[u8; 32] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| ConfigError::KeyLength)?;
    VerifyingKey::from_bytes(encoded).map_err(|_| ConfigError::KeyLength)
}

fn validate_key_file(path: &Path, private: bool) -> Result<(), ConfigError> {
    let metadata = path
        .symlink_metadata()
        .map_err(|_| ConfigError::KeyMetadata)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(ConfigError::KeyType);
    }
    #[cfg(unix)]
    {
        let mode = metadata.permissions().mode();
        if private && mode & 0o077 != 0 {
            return Err(ConfigError::PrivateKeyPermissions);
        }
        if !private && mode & 0o022 != 0 {
            return Err(ConfigError::PublicKeyPermissions);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relay_id_is_fixed_nonzero_base64url() {
        assert_eq!(
            decode_relay_id("AQEBAQEBAQEBAQEBAQEBAQ").expect("valid Relay ID"),
            [1_u8; 16]
        );
        assert!(decode_relay_id("AAAAAAAAAAAAAAAAAAAAAA").is_err());
        assert!(decode_relay_id("not-base64").is_err());
    }
}
