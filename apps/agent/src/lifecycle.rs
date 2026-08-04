use crate::{config::AgentConfig, error::Result};

#[cfg(target_os = "linux")]
use crate::{
    network::{NetworkPlan, TunNetwork},
    state::NodeState,
    storage::{Identity, read_json},
};

#[cfg(not(target_os = "linux"))]
use crate::error::AgentError;

/// Removes only stale network resources described by trusted local Agent state.
///
/// # Errors
///
/// Returns an Agent error when local identity or signed state is invalid, the project interface is
/// still active, or a recorded project-owned resource cannot be restored.
#[cfg(target_os = "linux")]
pub async fn cleanup_network(config: &AgentConfig) -> Result<()> {
    config.validate()?;
    let identity = Identity::load(&config.identity_path())?;
    let state: NodeState = read_json(&config.node_state_path())?;
    let controller_url = config.controller_url()?;
    state.validate(&identity, controller_url.as_str())?;
    let plan = NetworkPlan::from_state(config, &state)?;
    TunNetwork::recover_stale(&plan, &config.network_manifest_path()).await
}

/// Rejects lifecycle cleanup on platforms without the Linux TUN implementation.
///
/// # Errors
///
/// Always returns [`AgentError::UnsupportedPlatform`].
#[cfg(not(target_os = "linux"))]
#[allow(clippy::unused_async)] // Keeps the shared lifecycle cleanup API awaitable across targets.
pub async fn cleanup_network(_config: &AgentConfig) -> Result<()> {
    Err(AgentError::UnsupportedPlatform)
}
