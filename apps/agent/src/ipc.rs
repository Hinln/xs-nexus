use std::{net::Ipv4Addr, path::PathBuf, sync::Arc, time::Duration};

use tokio::{
    io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _},
    sync::watch,
    time::timeout,
};
use xs_core::{
    LocalAgentDiagnostics, LocalAgentRequest, LocalAgentResponse, LocalAgentStatus, LocalPeerStatus,
};

use crate::{
    data_plane::{DataPlaneStatus, SharedDataPlaneStatus},
    error::{AgentError, Result},
    health::AgentHealth,
    state::NodeState,
};

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

const LOCAL_PROTOCOL_VERSION: u8 = 1;
const MAX_REQUEST_BYTES: usize = 4096;
const MAX_RESPONSE_BYTES: usize = 512 * 1024;
const MAX_PEERS_PER_RESPONSE: usize = 512;
pub(super) const MAX_CONNECTIONS: usize = 16;
const IO_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Debug)]
pub struct IpcContext {
    pub interface_name: String,
    pub interface_index: u32,
    pub data_plane_status: SharedDataPlaneStatus,
}

/// Serves bounded, read-only local status requests over the private platform endpoint.
///
/// # Errors
///
/// Returns [`AgentError::Ipc`] when the endpoint is unsafe, another Agent is listening,
/// or the platform listener fails.
#[cfg(unix)]
pub async fn run_ipc_server(
    socket_path: PathBuf,
    state: Arc<tokio::sync::RwLock<NodeState>>,
    health: Arc<AgentHealth>,
    context: IpcContext,
    shutdown: watch::Receiver<bool>,
) -> Result<()> {
    unix::run(socket_path, state, health, context, shutdown).await
}

#[cfg(windows)]
pub async fn run_ipc_server(
    socket_path: PathBuf,
    state: Arc<tokio::sync::RwLock<NodeState>>,
    health: Arc<AgentHealth>,
    context: IpcContext,
    shutdown: watch::Receiver<bool>,
) -> Result<()> {
    windows::run(socket_path, state, health, context, shutdown).await
}

#[cfg(not(any(unix, windows)))]
pub async fn run_ipc_server(
    socket_path: PathBuf,
    state: Arc<tokio::sync::RwLock<NodeState>>,
    health: Arc<AgentHealth>,
    context: IpcContext,
    shutdown: watch::Receiver<bool>,
) -> Result<()> {
    let _ = (socket_path, state, health, context, shutdown);
    Err(AgentError::UnsupportedPlatform)
}

async fn handle_connection<S>(
    mut stream: S,
    state: &tokio::sync::RwLock<NodeState>,
    health: &AgentHealth,
    context: &IpcContext,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let response = match timeout(IO_TIMEOUT, read_request(&mut stream)).await {
        Ok(Ok(request)) => build_response(request, state, health, context)
            .await
            .unwrap_or_else(|error| error_response(error.code())),
        Ok(Err(error)) => error_response(error.code()),
        Err(_) => error_response(AgentError::Ipc.code()),
    };
    let encoded = serde_json::to_vec(&response).map_err(|_| AgentError::Ipc)?;
    if encoded.len() > MAX_RESPONSE_BYTES {
        return Err(AgentError::Ipc);
    }
    timeout(IO_TIMEOUT, stream.write_all(&encoded))
        .await
        .map_err(|_| AgentError::Ipc)?
        .map_err(|_| AgentError::Ipc)?;
    stream.shutdown().await.map_err(|_| AgentError::Ipc)
}

async fn read_request<S>(stream: &mut S) -> Result<LocalAgentRequest>
where
    S: AsyncRead + Unpin,
{
    let mut bytes = Vec::new();
    stream
        .take(u64::try_from(MAX_REQUEST_BYTES + 1).map_err(|_| AgentError::Ipc)?)
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| AgentError::Ipc)?;
    if bytes.is_empty() || bytes.len() > MAX_REQUEST_BYTES {
        return Err(AgentError::Ipc);
    }
    serde_json::from_slice(&bytes).map_err(|_| AgentError::Ipc)
}

async fn build_response(
    request: LocalAgentRequest,
    state: &tokio::sync::RwLock<NodeState>,
    health: &AgentHealth,
    context: &IpcContext,
) -> Result<LocalAgentResponse> {
    let state = state.read().await;
    let data_plane_status = context.data_plane_status.read().await;
    let status = LocalAgentStatus {
        node_id_base64: state.node_id_base64.clone(),
        virtual_ip: state.virtual_ip,
        controller_connected: health.controller_connected(),
        network_active: true,
        interface_name: context.interface_name.clone(),
        interface_index: context.interface_index,
        configuration_version: state.configuration.version,
        uptime_seconds: health.uptime_seconds(),
    };

    match request {
        LocalAgentRequest::Status {} => Ok(LocalAgentResponse::Status {
            schema_version: LOCAL_PROTOCOL_VERSION,
            status,
        }),
        LocalAgentRequest::Peers {} => {
            let configured_peers = state
                .configuration_payload
                .nodes
                .iter()
                .filter(|node| node.node_id_base64 != state.node_id_base64)
                .collect::<Vec<_>>();
            let total = u64::try_from(configured_peers.len()).map_err(|_| AgentError::Ipc)?;
            let peers = configured_peers
                .iter()
                .take(MAX_PEERS_PER_RESPONSE)
                .map(|node| local_peer_status(node, &data_plane_status))
                .collect::<Result<Vec<_>>>()?;
            let truncated = configured_peers.len() > peers.len();
            Ok(LocalAgentResponse::Peers {
                schema_version: LOCAL_PROTOCOL_VERSION,
                peers,
                total,
                truncated,
            })
        }
        LocalAgentRequest::Diagnostics {} => Ok(LocalAgentResponse::Diagnostics {
            schema_version: LOCAL_PROTOCOL_VERSION,
            diagnostics: LocalAgentDiagnostics {
                status,
                configuration_generated_at: state.configuration_payload.generated_at,
                address_pool: state.configuration_payload.address_pool.clone(),
                configuration_sha256: state.configuration_sha256.clone(),
                tun_packets_received: health.tun_packets_received(),
                tun_packets_dropped: health.tun_packets_dropped(),
                last_error_code: health.last_error_code(),
                local_candidates: data_plane_status.local_candidates.clone(),
            },
        }),
    }
}

fn local_peer_status(
    node: &xs_core::ConfigurationNode,
    data_plane_status: &DataPlaneStatus,
) -> Result<LocalPeerStatus> {
    let virtual_ip = node
        .virtual_ip
        .parse::<Ipv4Addr>()
        .map_err(|_| AgentError::State)?;
    let path = data_plane_status.peers.get(&virtual_ip);
    Ok(LocalPeerStatus {
        node_id_base64: node.node_id_base64.clone(),
        virtual_ip,
        credential_not_after: node.credential_not_after,
        role_bitmap: node.role_bitmap,
        tags: node.tags.clone(),
        candidates: path.map_or_else(|| node.candidates.clone(), |path| path.candidates.clone()),
        active_endpoint: path.and_then(|path| path.active_endpoint),
        active_candidate_kind: path.and_then(|path| path.active_candidate_kind),
        path_reason: path.and_then(|path| path.path_reason),
        session_established: path.is_some_and(|path| path.session_established),
    })
}

fn error_response(code: &str) -> LocalAgentResponse {
    LocalAgentResponse::Error {
        schema_version: LOCAL_PROTOCOL_VERSION,
        code: code.to_owned(),
    }
}
