use std::{env, net::SocketAddr, path::Path};

use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};
use thiserror::Error;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_validation_rejects_sql_and_mixed_case() {
        assert!(validate_schema("xs_nexus").is_ok());
        assert!(validate_schema("_test1").is_ok());
        assert!(validate_schema("XS_NEXUS").is_err());
        assert!(validate_schema("xs;drop schema public").is_err());
        assert!(validate_schema("").is_err());
    }
}
