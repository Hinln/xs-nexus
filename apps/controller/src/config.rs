use std::{
    collections::HashSet,
    env,
    net::{Ipv4Addr, SocketAddr},
    path::Path,
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};
use thiserror::Error;
use xs_core::ConfigurationRelay;
use zeroize::Zeroizing;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

pub struct ControllerConfig {
    pub listen: SocketAddr,
    pub discovery_listen: Option<SocketAddr>,
    pub discovery_public_endpoint: Option<SocketAddr>,
    pub database_url: String,
    pub database_schema: String,
    pub admin_token_hash: [u8; 32],
    pub credential_signing_key: SigningKey,
    pub config_signing_key: SigningKey,
    pub credential_ttl_seconds: u64,
    pub relays: Vec<ConfigurationRelay>,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("missing required environment variable {0}")]
    Missing(&'static str),
    #[error("invalid CONTROLLER_LISTEN")]
    Listen,
    #[error("DISCOVERY_LISTEN and DISCOVERY_PUBLIC_ENDPOINT must be valid and set together")]
    Discovery,
    #[error("invalid DATABASE_SCHEMA")]
    Schema,
    #[error("ADMIN_API_TOKEN must contain at least 32 characters")]
    AdminToken,
    #[error("invalid NODE_CREDENTIAL_TTL_SECONDS")]
    CredentialTtl,
    #[error("unable to inspect signing key file")]
    KeyMetadata,
    #[error("signing key file permissions must not grant group or other access")]
    KeyPermissions,
    #[error("unable to read signing key file")]
    KeyRead,
    #[error("signing key file must contain exactly 32 raw bytes")]
    KeyLength,
    #[error("credential and configuration signing keys must be different")]
    KeyReuse,
    #[error("invalid RELAY_CATALOG_PATH")]
    RelayCatalog,
}

impl ControllerConfig {
    /// Loads controller configuration and signing keys from the environment.
    ///
    /// # Errors
    ///
    /// Returns `ConfigError` when a required value, key, or permission is invalid.
    pub fn from_env() -> Result<Self, ConfigError> {
        let listen = required("CONTROLLER_LISTEN")?
            .parse()
            .map_err(|_| ConfigError::Listen)?;
        let discovery_listen = optional_socket("DISCOVERY_LISTEN")?;
        let discovery_public_endpoint = optional_socket("DISCOVERY_PUBLIC_ENDPOINT")?;
        if discovery_listen.is_some() != discovery_public_endpoint.is_some()
            || discovery_listen.is_some_and(|endpoint| endpoint.port() == 0)
            || discovery_public_endpoint.is_some_and(|endpoint| {
                endpoint.port() == 0
                    || endpoint.ip().is_unspecified()
                    || endpoint.ip().is_multicast()
            })
        {
            return Err(ConfigError::Discovery);
        }
        let database_url = required("DATABASE_URL")?;
        let database_schema = env::var("DATABASE_SCHEMA").unwrap_or_else(|_| "xs_nexus".to_owned());
        validate_schema(&database_schema)?;

        let admin_token = Zeroizing::new(required("ADMIN_API_TOKEN")?);
        if admin_token.chars().count() < 32 {
            return Err(ConfigError::AdminToken);
        }
        let admin_token_hash = Sha256::digest(admin_token.as_bytes()).into();
        drop(admin_token);

        let credential_signing_key =
            load_signing_key(Path::new(&required("CREDENTIAL_SIGNING_KEY_PATH")?))?;
        let config_signing_key =
            load_signing_key(Path::new(&required("CONFIG_SIGNING_KEY_PATH")?))?;
        if credential_signing_key.to_bytes() == config_signing_key.to_bytes() {
            return Err(ConfigError::KeyReuse);
        }

        let credential_ttl_seconds = env::var("NODE_CREDENTIAL_TTL_SECONDS")
            .unwrap_or_else(|_| "2592000".to_owned())
            .parse::<u64>()
            .map_err(|_| ConfigError::CredentialTtl)?;
        if !(3600..=31_536_000).contains(&credential_ttl_seconds) {
            return Err(ConfigError::CredentialTtl);
        }
        let relays = env::var("RELAY_CATALOG_PATH")
            .ok()
            .map(|path| load_relay_catalog(Path::new(&path)))
            .transpose()?
            .unwrap_or_default();

        Ok(Self {
            listen,
            discovery_listen,
            discovery_public_endpoint,
            database_url,
            database_schema,
            admin_token_hash,
            credential_signing_key,
            config_signing_key,
            credential_ttl_seconds,
            relays,
        })
    }
}

fn required(name: &'static str) -> Result<String, ConfigError> {
    env::var(name).map_err(|_| ConfigError::Missing(name))
}

fn optional_socket(name: &'static str) -> Result<Option<SocketAddr>, ConfigError> {
    env::var(name)
        .ok()
        .map(|value| value.parse().map_err(|_| ConfigError::Discovery))
        .transpose()
}

/// Validates a `PostgreSQL` schema identifier before it is quoted into SQL.
///
/// # Errors
///
/// Returns `ConfigError::Schema` for identifiers outside the accepted grammar.
pub fn validate_schema(schema: &str) -> Result<(), ConfigError> {
    let valid_length = !schema.is_empty() && schema.len() <= 63;
    let mut characters = schema.bytes();
    let valid_first = characters
        .next()
        .is_some_and(|byte| byte.is_ascii_lowercase() || byte == b'_');
    let valid_rest =
        characters.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');

    if valid_length && valid_first && valid_rest {
        Ok(())
    } else {
        Err(ConfigError::Schema)
    }
}

fn load_signing_key(path: &Path) -> Result<SigningKey, ConfigError> {
    let metadata = path.metadata().map_err(|_| ConfigError::KeyMetadata)?;
    #[cfg(unix)]
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err(ConfigError::KeyPermissions);
    }

    let bytes = Zeroizing::new(std::fs::read(path).map_err(|_| ConfigError::KeyRead)?);
    let seed: &[u8; 32] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| ConfigError::KeyLength)?;
    Ok(SigningKey::from_bytes(seed))
}

fn load_relay_catalog(path: &Path) -> Result<Vec<ConfigurationRelay>, ConfigError> {
    let metadata = path
        .symlink_metadata()
        .map_err(|_| ConfigError::RelayCatalog)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > 65_536
        || metadata.len() == 0
    {
        return Err(ConfigError::RelayCatalog);
    }
    #[cfg(unix)]
    if metadata.permissions().mode() & 0o022 != 0 {
        return Err(ConfigError::RelayCatalog);
    }
    let bytes = std::fs::read(path).map_err(|_| ConfigError::RelayCatalog)?;
    let mut relays: Vec<ConfigurationRelay> =
        serde_json::from_slice(&bytes).map_err(|_| ConfigError::RelayCatalog)?;
    if relays.len() > 16 {
        return Err(ConfigError::RelayCatalog);
    }
    let now = Utc::now();
    let mut relay_ids = HashSet::new();
    let mut endpoints = HashSet::new();
    let mut public_keys = HashSet::new();
    for relay in &relays {
        let relay_id = decode_array::<16>(&relay.relay_id_base64)?;
        let public_key = decode_array::<32>(&relay.identity_public_key_base64)?;
        if relay_id == [0_u8; 16]
            || ed25519_dalek::VerifyingKey::from_bytes(&public_key).is_err()
            || !valid_service_endpoint(relay.endpoint)
            || relay.priority == 0
            || relay.expires_at <= now
            || !relay_ids.insert(relay_id)
            || !endpoints.insert(relay.endpoint)
            || !public_keys.insert(public_key)
        {
            return Err(ConfigError::RelayCatalog);
        }
    }
    relays.sort_by(|left, right| {
        right
            .priority
            .cmp(&left.priority)
            .then_with(|| left.relay_id_base64.cmp(&right.relay_id_base64))
    });
    Ok(relays)
}

fn decode_array<const LENGTH: usize>(encoded: &str) -> Result<[u8; LENGTH], ConfigError> {
    URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| ConfigError::RelayCatalog)?
        .try_into()
        .map_err(|_| ConfigError::RelayCatalog)
}

fn valid_service_endpoint(endpoint: SocketAddr) -> bool {
    if endpoint.port() == 0 || endpoint.ip().is_unspecified() || endpoint.ip().is_multicast() {
        return false;
    }
    !matches!(endpoint, SocketAddr::V4(endpoint) if *endpoint.ip() == Ipv4Addr::BROADCAST)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn schema_validation_rejects_sql_and_mixed_case() {
        assert!(validate_schema("xs_nexus").is_ok());
        assert!(validate_schema("_test1").is_ok());
        assert!(validate_schema("XS_NEXUS").is_err());
        assert!(validate_schema("xs;drop schema public").is_err());
        assert!(validate_schema("").is_err());
    }

    #[test]
    fn relay_catalog_is_sorted_and_rejects_duplicate_endpoints() {
        let catalog = NamedTempFile::new().expect("temporary Relay catalog");
        let relays = vec![
            relay(1, 31, "127.0.0.1:42002"),
            relay(2, 32, "127.0.0.1:42001"),
        ];
        write_catalog(&catalog, &relays);
        let loaded = load_relay_catalog(catalog.path()).expect("valid Relay catalog");
        assert_eq!(loaded[0].priority, 32);
        assert_eq!(loaded[1].priority, 31);

        let duplicates = vec![
            relay(3, 33, "127.0.0.1:42003"),
            relay(4, 34, "127.0.0.1:42003"),
        ];
        write_catalog(&catalog, &duplicates);
        assert!(load_relay_catalog(catalog.path()).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn relay_catalog_rejects_group_or_other_writable_files() {
        let catalog = NamedTempFile::new().expect("temporary Relay catalog");
        write_catalog(&catalog, &[relay(5, 35, "127.0.0.1:42005")]);
        std::fs::set_permissions(catalog.path(), std::fs::Permissions::from_mode(0o622))
            .expect("set unsafe Relay catalog mode");
        assert!(load_relay_catalog(catalog.path()).is_err());
    }

    fn relay(id_seed: u8, priority: u32, endpoint: &str) -> ConfigurationRelay {
        ConfigurationRelay {
            relay_id_base64: URL_SAFE_NO_PAD.encode([id_seed; 16]),
            endpoint: endpoint.parse().expect("Relay endpoint"),
            identity_public_key_base64: URL_SAFE_NO_PAD.encode(
                SigningKey::from_bytes(&[id_seed.saturating_add(20); 32])
                    .verifying_key()
                    .to_bytes(),
            ),
            priority,
            expires_at: Utc::now() + chrono::Duration::minutes(5),
        }
    }

    fn write_catalog(catalog: &NamedTempFile, relays: &[ConfigurationRelay]) {
        std::fs::write(
            catalog.path(),
            serde_json::to_vec(relays).expect("serialize Relay catalog"),
        )
        .expect("write Relay catalog");
    }
}
