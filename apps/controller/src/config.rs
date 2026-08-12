use std::{
    collections::HashSet,
    env, fs,
    net::{Ipv4Addr, SocketAddr},
    path::{Path, PathBuf},
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use ed25519_dalek::{SigningKey, VerifyingKey};
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
    pub database_expected_role: Option<String>,
    pub admin_token_hash: [u8; 32],
    pub console_bootstrap_username: Option<String>,
    pub console_bootstrap_password: Option<Zeroizing<String>>,
    pub console_cookie_secure: bool,
    pub console_session_ttl_seconds: u64,
    pub credential_signing_key: SigningKey,
    pub config_signing_key: SigningKey,
    pub update_signing_public_key: Option<VerifyingKey>,
    pub linux_release_directory: Option<PathBuf>,
    pub windows_release_directory: Option<PathBuf>,
    pub credential_ttl_seconds: u64,
    pub max_nodes_per_network: u32,
    pub max_control_sessions: usize,
    pub configuration_send_concurrency: usize,
    pub relays: Vec<ConfigurationRelay>,
}

pub struct MigrationConfig {
    pub database_url: String,
    pub database_schema: String,
    pub database_owner_role: Option<String>,
    pub database_app_role: Option<String>,
}

struct ConsoleSettings {
    bootstrap_username: Option<String>,
    bootstrap_password: Option<Zeroizing<String>>,
    cookie_secure: bool,
    session_ttl_seconds: u64,
}

struct CapacitySettings {
    max_nodes_per_network: u32,
    max_control_sessions: usize,
    configuration_send_concurrency: usize,
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
    #[error("invalid database role identifier")]
    DatabaseRole,
    #[error("ADMIN_API_TOKEN must contain at least 32 characters")]
    AdminToken,
    #[error("CONSOLE_BOOTSTRAP_USERNAME and CONSOLE_BOOTSTRAP_PASSWORD must form a valid pair")]
    ConsoleBootstrap,
    #[error("CONSOLE_COOKIE_SECURE must be true or false")]
    ConsoleCookieSecure,
    #[error("CONSOLE_SESSION_TTL_SECONDS must be between 900 and 86400")]
    ConsoleSessionTtl,
    #[error("invalid NODE_CREDENTIAL_TTL_SECONDS")]
    CredentialTtl,
    #[error("MAX_NODES_PER_NETWORK must be between 1 and 1000")]
    MaxNodesPerNetwork,
    #[error("MAX_CONTROL_SESSIONS must be between 1 and 1000")]
    MaxControlSessions,
    #[error(
        "CONFIGURATION_SEND_CONCURRENCY must be between 1 and 128 and not exceed MAX_CONTROL_SESSIONS"
    )]
    ConfigurationSendConcurrency,
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
    #[error("invalid UPDATE_SIGNING_PUBLIC_KEY_PATH")]
    UpdateSigningPublicKey,
    #[error("invalid LINUX_RELEASE_DIRECTORY")]
    LinuxReleaseDirectory,
    #[error("invalid WINDOWS_RELEASE_DIRECTORY")]
    WindowsReleaseDirectory,
    #[error("invalid RELAY_CATALOG_PATH")]
    RelayCatalog,
    #[error("{0} and {0}_FILE must not both be set")]
    SecretConflict(&'static str),
    #[error("unable to inspect secret file for {0}")]
    SecretMetadata(&'static str),
    #[error("secret file for {0} must be a regular non-symlink file")]
    SecretType(&'static str),
    #[error("secret file permissions for {0} must not grant group or other access")]
    SecretPermissions(&'static str),
    #[error("unable to read secret file for {0}")]
    SecretRead(&'static str),
    #[error("secret value for {0} is invalid")]
    SecretValue(&'static str),
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
        let database_url = required_secret("DATABASE_URL")?.to_string();
        let database_schema = env::var("DATABASE_SCHEMA").unwrap_or_else(|_| "xs_nexus".to_owned());
        validate_schema(&database_schema)?;
        let database_expected_role = required_database_role("DATABASE_EXPECTED_ROLE")?;

        let admin_token = required_secret("ADMIN_API_TOKEN")?;
        if admin_token.chars().count() < 32 {
            return Err(ConfigError::AdminToken);
        }
        let admin_token_hash = Sha256::digest(admin_token.as_bytes()).into();
        drop(admin_token);

        let console = load_console_settings()?;

        let credential_signing_key =
            load_signing_key(Path::new(&required("CREDENTIAL_SIGNING_KEY_PATH")?))?;
        let config_signing_key =
            load_signing_key(Path::new(&required("CONFIG_SIGNING_KEY_PATH")?))?;
        if credential_signing_key.to_bytes() == config_signing_key.to_bytes() {
            return Err(ConfigError::KeyReuse);
        }
        let update_signing_public_key = env::var("UPDATE_SIGNING_PUBLIC_KEY_PATH")
            .ok()
            .map(|path| load_update_verifying_key(Path::new(&path)))
            .transpose()?;
        let linux_release_directory = optional_linux_release_directory()?;
        let windows_release_directory = optional_windows_release_directory()?;

        let credential_ttl_seconds = env::var("NODE_CREDENTIAL_TTL_SECONDS")
            .unwrap_or_else(|_| "2592000".to_owned())
            .parse::<u64>()
            .map_err(|_| ConfigError::CredentialTtl)?;
        if !(3600..=31_536_000).contains(&credential_ttl_seconds) {
            return Err(ConfigError::CredentialTtl);
        }
        let capacity = load_capacity_settings()?;
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
            database_expected_role: Some(database_expected_role),
            admin_token_hash,
            console_bootstrap_username: console.bootstrap_username,
            console_bootstrap_password: console.bootstrap_password,
            console_cookie_secure: console.cookie_secure,
            console_session_ttl_seconds: console.session_ttl_seconds,
            credential_signing_key,
            config_signing_key,
            update_signing_public_key,
            linux_release_directory,
            windows_release_directory,
            credential_ttl_seconds,
            max_nodes_per_network: capacity.max_nodes_per_network,
            max_control_sessions: capacity.max_control_sessions,
            configuration_send_concurrency: capacity.configuration_send_concurrency,
            relays,
        })
    }
}

fn load_console_settings() -> Result<ConsoleSettings, ConfigError> {
    let bootstrap_password = optional_secret("CONSOLE_BOOTSTRAP_PASSWORD")?;
    let configured_username = env::var("CONSOLE_BOOTSTRAP_USERNAME")
        .ok()
        .filter(|username| !username.is_empty());
    let bootstrap_username = match (configured_username, bootstrap_password.as_ref()) {
        (Some(username), Some(password))
            if valid_console_username(&username) && valid_console_password(password) =>
        {
            Some(username)
        }
        (None, Some(password)) if valid_console_password(password) => Some("admin".to_owned()),
        (None, None) => None,
        _ => return Err(ConfigError::ConsoleBootstrap),
    };
    let cookie_secure =
        env::var("CONSOLE_COOKIE_SECURE").map_or(Ok(true), |value| match value.as_str() {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err(ConfigError::ConsoleCookieSecure),
        })?;
    let session_ttl_seconds = env::var("CONSOLE_SESSION_TTL_SECONDS")
        .unwrap_or_else(|_| "28800".to_owned())
        .parse::<u64>()
        .map_err(|_| ConfigError::ConsoleSessionTtl)?;
    if !(900..=86_400).contains(&session_ttl_seconds) {
        return Err(ConfigError::ConsoleSessionTtl);
    }
    Ok(ConsoleSettings {
        bootstrap_username,
        bootstrap_password,
        cookie_secure,
        session_ttl_seconds,
    })
}

fn load_capacity_settings() -> Result<CapacitySettings, ConfigError> {
    let max_nodes_per_network = env::var("MAX_NODES_PER_NETWORK")
        .unwrap_or_else(|_| "1000".to_owned())
        .parse::<u32>()
        .map_err(|_| ConfigError::MaxNodesPerNetwork)?;
    if !(1..=1000).contains(&max_nodes_per_network) {
        return Err(ConfigError::MaxNodesPerNetwork);
    }
    let max_control_sessions = env::var("MAX_CONTROL_SESSIONS")
        .unwrap_or_else(|_| "1000".to_owned())
        .parse::<usize>()
        .map_err(|_| ConfigError::MaxControlSessions)?;
    if !(1..=1000).contains(&max_control_sessions) {
        return Err(ConfigError::MaxControlSessions);
    }
    let configuration_send_concurrency = env::var("CONFIGURATION_SEND_CONCURRENCY")
        .unwrap_or_else(|_| "64".to_owned())
        .parse::<usize>()
        .map_err(|_| ConfigError::ConfigurationSendConcurrency)?;
    if !(1..=128).contains(&configuration_send_concurrency)
        || configuration_send_concurrency > max_control_sessions
    {
        return Err(ConfigError::ConfigurationSendConcurrency);
    }
    Ok(CapacitySettings {
        max_nodes_per_network,
        max_control_sessions,
        configuration_send_concurrency,
    })
}

impl MigrationConfig {
    /// Loads only the database settings required by the migration job.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] when the database secret or schema is invalid.
    pub fn from_env() -> Result<Self, ConfigError> {
        let database_url = required_secret("DATABASE_URL")?.to_string();
        let database_schema = env::var("DATABASE_SCHEMA").unwrap_or_else(|_| "xs_nexus".to_owned());
        validate_schema(&database_schema)?;
        let database_owner_role = env::var("DATABASE_OWNER_ROLE").ok();
        if let Some(role) = database_owner_role.as_deref() {
            validate_database_role(role)?;
        }
        let database_app_role = env::var("DATABASE_APP_ROLE").ok();
        if let Some(role) = database_app_role.as_deref() {
            validate_database_role(role)?;
        }
        Ok(Self {
            database_url,
            database_schema,
            database_owner_role,
            database_app_role,
        })
    }
}

fn valid_console_username(username: &str) -> bool {
    (3..=64).contains(&username.len())
        && username.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || (index > 0 && matches!(byte, b'.' | b'_' | b'-'))
        })
}

fn valid_console_password(password: &str) -> bool {
    (12..=128).contains(&password.chars().count())
}

fn required(name: &'static str) -> Result<String, ConfigError> {
    env::var(name).map_err(|_| ConfigError::Missing(name))
}

fn required_database_role(name: &'static str) -> Result<String, ConfigError> {
    let role = required(name)?;
    validate_database_role(&role)?;
    Ok(role)
}

fn required_secret(name: &'static str) -> Result<Zeroizing<String>, ConfigError> {
    optional_secret(name)?.ok_or(ConfigError::Missing(name))
}

fn optional_secret(name: &'static str) -> Result<Option<Zeroizing<String>>, ConfigError> {
    let direct = env::var(name).ok();
    let file_variable = format!("{name}_FILE");
    let file = env::var(file_variable).ok().filter(|path| !path.is_empty());
    match (direct, file) {
        (Some(_), Some(_)) => Err(ConfigError::SecretConflict(name)),
        (Some(value), None) => validate_secret_value(name, value).map(Some),
        (None, Some(path)) => read_secret_file(name, Path::new(&path)).map(Some),
        (None, None) => Ok(None),
    }
}

fn read_secret_file(name: &'static str, path: &Path) -> Result<Zeroizing<String>, ConfigError> {
    let metadata = path
        .symlink_metadata()
        .map_err(|_| ConfigError::SecretMetadata(name))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(ConfigError::SecretType(name));
    }
    #[cfg(unix)]
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err(ConfigError::SecretPermissions(name));
    }
    if metadata.len() == 0 || metadata.len() > 8_192 {
        return Err(ConfigError::SecretValue(name));
    }
    let mut value = fs::read_to_string(path).map_err(|_| ConfigError::SecretRead(name))?;
    if value.ends_with('\n') {
        value.pop();
        if value.ends_with('\r') {
            value.pop();
        }
    }
    validate_secret_value(name, value)
}

fn validate_secret_value(
    name: &'static str,
    value: String,
) -> Result<Zeroizing<String>, ConfigError> {
    if value.is_empty() || value.len() > 8_192 || value.contains(['\0', '\r', '\n']) {
        Err(ConfigError::SecretValue(name))
    } else {
        Ok(Zeroizing::new(value))
    }
}

fn optional_socket(name: &'static str) -> Result<Option<SocketAddr>, ConfigError> {
    env::var(name)
        .ok()
        .map(|value| value.parse().map_err(|_| ConfigError::Discovery))
        .transpose()
}

fn optional_linux_release_directory() -> Result<Option<PathBuf>, ConfigError> {
    env::var("LINUX_RELEASE_DIRECTORY")
        .ok()
        .map(PathBuf::from)
        .map(|path| {
            crate::downloads::validate_release_directory(&path)
                .map(|()| path)
                .map_err(|_| ConfigError::LinuxReleaseDirectory)
        })
        .transpose()
}

fn optional_windows_release_directory() -> Result<Option<PathBuf>, ConfigError> {
    env::var("WINDOWS_RELEASE_DIRECTORY")
        .ok()
        .map(PathBuf::from)
        .map(|path| {
            crate::downloads::validate_windows_release_directory(&path)
                .map(|()| path)
                .map_err(|_| ConfigError::WindowsReleaseDirectory)
        })
        .transpose()
}

/// Validates a `PostgreSQL` schema identifier before it is quoted into SQL.
///
/// # Errors
///
/// Returns `ConfigError::Schema` for identifiers outside the accepted grammar.
pub fn validate_schema(schema: &str) -> Result<(), ConfigError> {
    validate_identifier(schema).map_err(|()| ConfigError::Schema)
}

/// Validates a `PostgreSQL` role identifier before it is quoted into SQL.
///
/// # Errors
///
/// Returns `ConfigError::DatabaseRole` for identifiers outside the accepted grammar.
pub fn validate_database_role(role: &str) -> Result<(), ConfigError> {
    validate_identifier(role).map_err(|()| ConfigError::DatabaseRole)
}

fn validate_identifier(identifier: &str) -> Result<(), ()> {
    let valid_length = !identifier.is_empty() && identifier.len() <= 63;
    let mut characters = identifier.bytes();
    let valid_first = characters
        .next()
        .is_some_and(|byte| byte.is_ascii_lowercase() || byte == b'_');
    let valid_rest =
        characters.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
    if valid_length && valid_first && valid_rest {
        Ok(())
    } else {
        Err(())
    }
}

fn load_signing_key(path: &Path) -> Result<SigningKey, ConfigError> {
    let metadata = path.metadata().map_err(|_| ConfigError::KeyMetadata)?;
    #[cfg(unix)]
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err(ConfigError::KeyPermissions);
    }
    #[cfg(not(unix))]
    let _ = metadata;

    let bytes = Zeroizing::new(std::fs::read(path).map_err(|_| ConfigError::KeyRead)?);
    let seed: &[u8; 32] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| ConfigError::KeyLength)?;
    Ok(SigningKey::from_bytes(seed))
}

fn load_update_verifying_key(path: &Path) -> Result<VerifyingKey, ConfigError> {
    let metadata = path
        .symlink_metadata()
        .map_err(|_| ConfigError::UpdateSigningPublicKey)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() != 32 {
        return Err(ConfigError::UpdateSigningPublicKey);
    }
    #[cfg(unix)]
    if metadata.permissions().mode() & 0o022 != 0 {
        return Err(ConfigError::UpdateSigningPublicKey);
    }
    let bytes = fs::read(path).map_err(|_| ConfigError::UpdateSigningPublicKey)?;
    let encoded: [u8; 32] = bytes
        .try_into()
        .map_err(|_| ConfigError::UpdateSigningPublicKey)?;
    VerifyingKey::from_bytes(&encoded).map_err(|_| ConfigError::UpdateSigningPublicKey)
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
    fn database_role_validation_rejects_sql_and_mixed_case() {
        assert!(validate_database_role("xs_nexus_app").is_ok());
        assert!(validate_database_role("_migration1").is_ok());
        assert!(validate_database_role("XS_NEXUS_APP").is_err());
        assert!(validate_database_role("app;set role root").is_err());
        assert!(validate_database_role("").is_err());
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

    #[test]
    fn secret_files_allow_one_trailing_newline_and_reject_embedded_lines() {
        let secret = NamedTempFile::new().expect("temporary secret");
        std::fs::write(secret.path(), b"secret-value\n").expect("write secret");
        assert_eq!(
            read_secret_file("TEST_SECRET", secret.path())
                .expect("valid secret")
                .as_str(),
            "secret-value"
        );

        std::fs::write(secret.path(), b"line-one\nline-two\n").expect("write invalid secret");
        assert!(read_secret_file("TEST_SECRET", secret.path()).is_err());
    }

    #[test]
    fn update_verifying_key_is_raw_and_rejects_wrong_length() {
        let key = NamedTempFile::new().expect("temporary update key");
        let expected = SigningKey::from_bytes(&[44_u8; 32]).verifying_key();
        std::fs::write(key.path(), expected.to_bytes()).expect("write update key");
        assert_eq!(
            load_update_verifying_key(key.path()).expect("valid update key"),
            expected
        );
        std::fs::write(key.path(), [1_u8; 31]).expect("write invalid update key");
        assert!(load_update_verifying_key(key.path()).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn secret_files_reject_group_or_other_permissions() {
        let secret = NamedTempFile::new().expect("temporary secret");
        std::fs::write(secret.path(), b"secret-value").expect("write secret");
        std::fs::set_permissions(secret.path(), std::fs::Permissions::from_mode(0o640))
            .expect("set unsafe secret mode");
        assert!(read_secret_file("TEST_SECRET", secret.path()).is_err());
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
