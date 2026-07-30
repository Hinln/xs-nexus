use std::net::SocketAddr;
use std::sync::Arc;

use ed25519_dalek::SigningKey;
use sqlx::PgPool;
use xs_core::ConfigurationRelay;

use crate::config::ControllerConfig;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub admin_token_hash: [u8; 32],
    pub credential_signing_key: Arc<SigningKey>,
    pub config_signing_key: Arc<SigningKey>,
    pub credential_ttl_seconds: u64,
    pub discovery_public_endpoints: Arc<Vec<SocketAddr>>,
    pub relays: Arc<Vec<ConfigurationRelay>>,
}

impl AppState {
    #[must_use]
    pub fn new(pool: PgPool, config: &ControllerConfig) -> Self {
        Self {
            pool,
            admin_token_hash: config.admin_token_hash,
            credential_signing_key: Arc::new(config.credential_signing_key.clone()),
            config_signing_key: Arc::new(config.config_signing_key.clone()),
            credential_ttl_seconds: config.credential_ttl_seconds,
            discovery_public_endpoints: Arc::new(
                config.discovery_public_endpoint.into_iter().collect(),
            ),
            relays: Arc::new(config.relays.clone()),
        }
    }
}
