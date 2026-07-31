use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use ed25519_dalek::SigningKey;
use sqlx::PgPool;
use tokio::sync::{RwLock, broadcast};
use xs_core::ConfigurationRelay;

use crate::config::ControllerConfig;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub admin_token_hash: [u8; 32],
    pub console_cookie_secure: bool,
    pub console_session_ttl_seconds: u64,
    pub credential_signing_key: Arc<SigningKey>,
    pub config_signing_key: Arc<SigningKey>,
    pub credential_ttl_seconds: u64,
    pub discovery_public_endpoints: Arc<Vec<SocketAddr>>,
    pub relays: Arc<Vec<ConfigurationRelay>>,
    online_nodes: Arc<RwLock<HashMap<[u8; 16], usize>>>,
    configuration_events: broadcast::Sender<uuid::Uuid>,
}

impl AppState {
    #[must_use]
    pub fn new(pool: PgPool, config: &ControllerConfig) -> Self {
        let (configuration_events, _) = broadcast::channel(256);
        Self {
            pool,
            admin_token_hash: config.admin_token_hash,
            console_cookie_secure: config.console_cookie_secure,
            console_session_ttl_seconds: config.console_session_ttl_seconds,
            credential_signing_key: Arc::new(config.credential_signing_key.clone()),
            config_signing_key: Arc::new(config.config_signing_key.clone()),
            credential_ttl_seconds: config.credential_ttl_seconds,
            discovery_public_endpoints: Arc::new(
                config.discovery_public_endpoint.into_iter().collect(),
            ),
            relays: Arc::new(config.relays.clone()),
            online_nodes: Arc::new(RwLock::new(HashMap::new())),
            configuration_events,
        }
    }

    pub(crate) async fn mark_node_online(&self, node_id: [u8; 16]) {
        let mut online_nodes = self.online_nodes.write().await;
        *online_nodes.entry(node_id).or_default() += 1;
    }

    pub(crate) async fn mark_node_offline(&self, node_id: [u8; 16]) -> bool {
        let mut online_nodes = self.online_nodes.write().await;
        let Some(connections) = online_nodes.get_mut(&node_id) else {
            return false;
        };
        if *connections > 1 {
            *connections -= 1;
            false
        } else {
            online_nodes.remove(&node_id);
            true
        }
    }

    pub(crate) async fn online_node_ids(&self) -> Vec<[u8; 16]> {
        self.online_nodes.read().await.keys().copied().collect()
    }

    pub(crate) fn subscribe_configuration_events(&self) -> broadcast::Receiver<uuid::Uuid> {
        self.configuration_events.subscribe()
    }

    pub(crate) fn notify_configuration_changed(&self, network_id: uuid::Uuid) {
        let _ = self.configuration_events.send(network_id);
    }
}
