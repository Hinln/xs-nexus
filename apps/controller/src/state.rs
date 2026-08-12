use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use ed25519_dalek::{SigningKey, VerifyingKey};
use sqlx::PgPool;
use tokio::sync::{OwnedSemaphorePermit, RwLock, Semaphore, broadcast};
use tokio::time::{Duration, timeout};
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
    pub update_signing_public_key: Option<Arc<VerifyingKey>>,
    pub linux_release_directory: Option<Arc<PathBuf>>,
    pub windows_release_directory: Option<Arc<PathBuf>>,
    pub credential_ttl_seconds: u64,
    pub max_nodes_per_network: u32,
    pub max_control_sessions: usize,
    pub configuration_send_concurrency: usize,
    pub discovery_public_endpoints: Arc<Vec<SocketAddr>>,
    pub relays: Arc<Vec<ConfigurationRelay>>,
    online_nodes: Arc<RwLock<HashMap<[u8; 16], usize>>>,
    control_auth_failures: Arc<AtomicU64>,
    rejected_control_sessions: Arc<AtomicU64>,
    control_sessions: Arc<Semaphore>,
    configuration_sends: Arc<Semaphore>,
    configuration_events: broadcast::Sender<uuid::Uuid>,
    update_events: broadcast::Sender<uuid::Uuid>,
}

impl AppState {
    #[must_use]
    pub fn new(pool: PgPool, config: &ControllerConfig) -> Self {
        let (configuration_events, _) = broadcast::channel(256);
        let (update_events, _) = broadcast::channel(256);
        Self {
            pool,
            admin_token_hash: config.admin_token_hash,
            console_cookie_secure: config.console_cookie_secure,
            console_session_ttl_seconds: config.console_session_ttl_seconds,
            credential_signing_key: Arc::new(config.credential_signing_key.clone()),
            config_signing_key: Arc::new(config.config_signing_key.clone()),
            update_signing_public_key: config.update_signing_public_key.map(Arc::new),
            linux_release_directory: config.linux_release_directory.clone().map(Arc::new),
            windows_release_directory: config.windows_release_directory.clone().map(Arc::new),
            credential_ttl_seconds: config.credential_ttl_seconds,
            max_nodes_per_network: config.max_nodes_per_network,
            max_control_sessions: config.max_control_sessions,
            configuration_send_concurrency: config.configuration_send_concurrency,
            discovery_public_endpoints: Arc::new(
                config.discovery_public_endpoint.into_iter().collect(),
            ),
            relays: Arc::new(config.relays.clone()),
            online_nodes: Arc::new(RwLock::new(HashMap::new())),
            control_auth_failures: Arc::new(AtomicU64::new(0)),
            rejected_control_sessions: Arc::new(AtomicU64::new(0)),
            control_sessions: Arc::new(Semaphore::new(config.max_control_sessions)),
            configuration_sends: Arc::new(Semaphore::new(config.configuration_send_concurrency)),
            configuration_events,
            update_events,
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

    pub(crate) fn record_control_auth_failure(&self) {
        let _ = self.control_auth_failures.fetch_update(
            Ordering::Relaxed,
            Ordering::Relaxed,
            |current| Some(current.saturating_add(1)),
        );
    }

    pub(crate) fn control_auth_failures_total(&self) -> u64 {
        self.control_auth_failures.load(Ordering::Relaxed)
    }

    pub(crate) fn try_acquire_control_session(&self) -> Option<OwnedSemaphorePermit> {
        self.control_sessions.clone().try_acquire_owned().ok()
    }

    pub(crate) async fn acquire_configuration_send(&self) -> Option<OwnedSemaphorePermit> {
        timeout(
            Duration::from_secs(10),
            self.configuration_sends.clone().acquire_owned(),
        )
        .await
        .ok()
        .and_then(Result::ok)
    }

    pub(crate) fn record_rejected_control_session(&self) {
        let _ = self.rejected_control_sessions.fetch_update(
            Ordering::Relaxed,
            Ordering::Relaxed,
            |current| Some(current.saturating_add(1)),
        );
    }

    pub(crate) fn rejected_control_sessions_total(&self) -> u64 {
        self.rejected_control_sessions.load(Ordering::Relaxed)
    }

    pub(crate) fn active_control_sessions(&self) -> usize {
        self.max_control_sessions
            .saturating_sub(self.control_sessions.available_permits())
    }

    pub(crate) fn active_configuration_sends(&self) -> usize {
        self.configuration_send_concurrency
            .saturating_sub(self.configuration_sends.available_permits())
    }

    pub(crate) fn subscribe_configuration_events(&self) -> broadcast::Receiver<uuid::Uuid> {
        self.configuration_events.subscribe()
    }

    pub(crate) fn notify_configuration_changed(&self, network_id: uuid::Uuid) {
        let _ = self.configuration_events.send(network_id);
    }

    pub(crate) fn subscribe_update_events(&self) -> broadcast::Receiver<uuid::Uuid> {
        self.update_events.subscribe()
    }

    pub(crate) fn notify_update_changed(&self, network_id: uuid::Uuid) {
        let _ = self.update_events.send(network_id);
    }
}
