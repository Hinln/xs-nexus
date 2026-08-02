use std::{net::Ipv4Addr, path::PathBuf, sync::Arc, time::Duration};

use tokio::{
    io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _},
    sync::{mpsc, oneshot, watch},
    time::timeout,
};
use xs_core::{
    EndpointCandidateKind, LocalAgentDiagnostics, LocalAgentRequest, LocalAgentResponse,
    LocalAgentStatus, LocalNetcheckResult, LocalPeerPath, LocalPeerStatus, LocalPingResult,
    LocalRouteStatus,
};

use crate::{
    data_plane::{DataPlaneStatus, PeerPathStatus, SharedDataPlaneStatus},
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
const FRAME_PREFIX_BYTES: usize = 4;
const MAX_PEERS_PER_RESPONSE: usize = 512;
pub(super) const MAX_CONNECTIONS: usize = 16;
const IO_TIMEOUT: Duration = Duration::from_secs(2);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(1);
const PING_TIMEOUT: Duration = Duration::from_millis(1_500);
const PING_POLL_INTERVAL: Duration = Duration::from_millis(20);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeCommandError {
    PeerNotFound,
    SessionUnavailable,
    Busy,
    Network,
    RateLimited,
}

impl RuntimeCommandError {
    const fn code(self) -> &'static str {
        match self {
            Self::PeerNotFound => "agent_peer_not_found",
            Self::SessionUnavailable => "agent_peer_session_unavailable",
            Self::Busy => "agent_path_probe_busy",
            Self::Network => "agent_path_probe_failed",
            Self::RateLimited => "agent_reconnect_rate_limited",
        }
    }
}

#[derive(Debug)]
pub enum RuntimeCommand {
    Probe {
        virtual_ip: Ipv4Addr,
        response: oneshot::Sender<std::result::Result<(), RuntimeCommandError>>,
    },
    Reconnect {
        response: oneshot::Sender<std::result::Result<(), RuntimeCommandError>>,
    },
}

#[derive(Clone, Debug)]
pub struct IpcContext {
    pub interface_name: String,
    pub interface_index: u32,
    pub data_plane_status: SharedDataPlaneStatus,
    pub runtime_commands: mpsc::Sender<RuntimeCommand>,
}

/// Serves bounded local diagnostics and explicitly constrained management requests.
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
    let length = u32::try_from(encoded.len()).map_err(|_| AgentError::Ipc)?;
    timeout(IO_TIMEOUT, async {
        stream.write_all(&length.to_be_bytes()).await?;
        stream.write_all(&encoded).await
    })
    .await
    .map_err(|_| AgentError::Ipc)?
    .map_err(|_| AgentError::Ipc)?;
    stream.shutdown().await.map_err(|_| AgentError::Ipc)
}

async fn read_request<S>(stream: &mut S) -> Result<LocalAgentRequest>
where
    S: AsyncRead + Unpin,
{
    let mut prefix = [0_u8; FRAME_PREFIX_BYTES];
    stream
        .read_exact(&mut prefix)
        .await
        .map_err(|_| AgentError::Ipc)?;
    let length = usize::try_from(u32::from_be_bytes(prefix)).map_err(|_| AgentError::Ipc)?;
    if length == 0 || length > MAX_REQUEST_BYTES {
        return Err(AgentError::Ipc);
    }
    let mut bytes = vec![0_u8; length];
    stream
        .read_exact(&mut bytes)
        .await
        .map_err(|_| AgentError::Ipc)?;
    serde_json::from_slice(&bytes).map_err(|_| AgentError::Ipc)
}

async fn build_response(
    request: LocalAgentRequest,
    state: &tokio::sync::RwLock<NodeState>,
    health: &AgentHealth,
    context: &IpcContext,
) -> Result<LocalAgentResponse> {
    if let LocalAgentRequest::Ping { virtual_ip } = request {
        return ping_response(virtual_ip, context).await;
    }
    if let LocalAgentRequest::Reconnect {} = request {
        return reconnect_response(context).await;
    }
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
        LocalAgentRequest::Path { virtual_ip } => {
            let node = state
                .configuration_payload
                .nodes
                .iter()
                .find(|node| node.virtual_ip == virtual_ip.to_string())
                .ok_or(AgentError::Ipc)?;
            let path = data_plane_status
                .peers
                .get(&virtual_ip)
                .ok_or(AgentError::Ipc)?;
            Ok(LocalAgentResponse::Path {
                schema_version: LOCAL_PROTOCOL_VERSION,
                path: local_peer_path(node.node_id_base64.clone(), virtual_ip, path),
            })
        }
        LocalAgentRequest::Routes {} => routes_response(&state, &data_plane_status),
        LocalAgentRequest::Netcheck {} => {
            netcheck_response(&state, health, &status, &data_plane_status)
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
        LocalAgentRequest::Ping { .. } | LocalAgentRequest::Reconnect {} => Err(AgentError::Ipc),
    }
}

fn routes_response(
    state: &NodeState,
    data_plane_status: &DataPlaneStatus,
) -> Result<LocalAgentResponse> {
    let routes = state
        .configuration_payload
        .subnet_routes
        .iter()
        .map(|route| {
            let gateway = state
                .configuration_payload
                .nodes
                .iter()
                .find(|node| node.node_id_base64 == route.gateway_node_id_base64)
                .ok_or(AgentError::State)?;
            let gateway_virtual_ip = gateway
                .virtual_ip
                .parse::<Ipv4Addr>()
                .map_err(|_| AgentError::State)?;
            Ok(LocalRouteStatus {
                route_id: route.route_id.clone(),
                prefix: route.prefix.clone(),
                gateway_node_id_base64: route.gateway_node_id_base64.clone(),
                gateway_virtual_ip,
                mode: route.mode,
                interface_name: route.interface_name.clone(),
                priority: route.priority,
                local_is_gateway: route.gateway_node_id_base64 == state.node_id_base64,
                gateway_reachable: route.gateway_node_id_base64 == state.node_id_base64
                    || data_plane_status
                        .peers
                        .get(&gateway_virtual_ip)
                        .is_some_and(|path| path.session_established),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(LocalAgentResponse::Routes {
        schema_version: LOCAL_PROTOCOL_VERSION,
        configuration_version: state.configuration.version,
        routes,
    })
}

fn netcheck_response(
    state: &NodeState,
    health: &AgentHealth,
    status: &LocalAgentStatus,
    data_plane_status: &DataPlaneStatus,
) -> Result<LocalAgentResponse> {
    let configured_peer_count = state
        .configuration_payload
        .nodes
        .iter()
        .filter(|node| node.node_id_base64 != state.node_id_base64)
        .count();
    let established_peer_count = data_plane_status
        .peers
        .values()
        .filter(|path| path.session_established)
        .count();
    let direct_peer_count = data_plane_status
        .peers
        .values()
        .filter(|path| {
            path.session_established
                && path
                    .active_candidate_kind
                    .is_some_and(|kind| kind != EndpointCandidateKind::Relay)
        })
        .count();
    let relay_peer_count = data_plane_status
        .peers
        .values()
        .filter(|path| {
            path.session_established
                && path.active_candidate_kind == Some(EndpointCandidateKind::Relay)
        })
        .count();
    let last_error_code = health.last_error_code();
    let healthy = status.controller_connected && status.network_active && last_error_code.is_none();
    Ok(LocalAgentResponse::Netcheck {
        schema_version: LOCAL_PROTOCOL_VERSION,
        result: LocalNetcheckResult {
            healthy,
            controller_connected: status.controller_connected,
            network_active: status.network_active,
            local_candidate_count: bounded_count(data_plane_status.local_candidates.len())?,
            configured_peer_count: bounded_count(configured_peer_count)?,
            established_peer_count: bounded_count(established_peer_count)?,
            direct_peer_count: bounded_count(direct_peer_count)?,
            relay_peer_count: bounded_count(relay_peer_count)?,
            last_error_code,
        },
    })
}

async fn ping_response(virtual_ip: Ipv4Addr, context: &IpcContext) -> Result<LocalAgentResponse> {
    let baseline = {
        let status = context.data_plane_status.read().await;
        let Some(peer) = status.peers.get(&virtual_ip) else {
            return Ok(ping_failure(virtual_ip, "agent_peer_not_found"));
        };
        if !peer.session_established {
            return Ok(ping_failure(virtual_ip, "agent_peer_session_unavailable"));
        }
        peer.latency_samples_total
    };
    let (response_sender, response_receiver) = oneshot::channel();
    timeout(
        COMMAND_TIMEOUT,
        context.runtime_commands.send(RuntimeCommand::Probe {
            virtual_ip,
            response: response_sender,
        }),
    )
    .await
    .map_err(|_| AgentError::Ipc)?
    .map_err(|_| AgentError::Ipc)?;
    match timeout(COMMAND_TIMEOUT, response_receiver)
        .await
        .map_err(|_| AgentError::Ipc)?
        .map_err(|_| AgentError::Ipc)?
    {
        Ok(()) => {}
        Err(error) => return Ok(ping_failure(virtual_ip, error.code())),
    }

    let observed = timeout(PING_TIMEOUT, async {
        loop {
            let path = context
                .data_plane_status
                .read()
                .await
                .peers
                .get(&virtual_ip)
                .cloned();
            if path
                .as_ref()
                .is_some_and(|path| path.latency_samples_total > baseline)
            {
                return path;
            }
            tokio::time::sleep(PING_POLL_INTERVAL).await;
        }
    })
    .await
    .ok()
    .flatten();
    let Some(path) = observed else {
        return Ok(ping_failure(virtual_ip, "agent_path_probe_timeout"));
    };
    Ok(LocalAgentResponse::Ping {
        schema_version: LOCAL_PROTOCOL_VERSION,
        result: LocalPingResult {
            virtual_ip,
            reachable: true,
            latency_microseconds: path.last_latency_microseconds,
            active_candidate_kind: path.active_candidate_kind,
            path_reason: path.path_reason,
            error_code: None,
        },
    })
}

async fn reconnect_response(context: &IpcContext) -> Result<LocalAgentResponse> {
    let (response_sender, response_receiver) = oneshot::channel();
    timeout(
        COMMAND_TIMEOUT,
        context.runtime_commands.send(RuntimeCommand::Reconnect {
            response: response_sender,
        }),
    )
    .await
    .map_err(|_| AgentError::Ipc)?
    .map_err(|_| AgentError::Ipc)?;
    let result = timeout(COMMAND_TIMEOUT, response_receiver)
        .await
        .map_err(|_| AgentError::Ipc)?
        .map_err(|_| AgentError::Ipc)?;
    Ok(LocalAgentResponse::Reconnect {
        schema_version: LOCAL_PROTOCOL_VERSION,
        accepted: result.is_ok(),
        error_code: result.err().map(|error| error.code().to_owned()),
    })
}

fn ping_failure(virtual_ip: Ipv4Addr, error_code: &str) -> LocalAgentResponse {
    LocalAgentResponse::Ping {
        schema_version: LOCAL_PROTOCOL_VERSION,
        result: LocalPingResult {
            virtual_ip,
            reachable: false,
            latency_microseconds: None,
            active_candidate_kind: None,
            path_reason: None,
            error_code: Some(error_code.to_owned()),
        },
    }
}

fn local_peer_path(
    node_id_base64: String,
    virtual_ip: Ipv4Addr,
    path: &PeerPathStatus,
) -> LocalPeerPath {
    LocalPeerPath {
        node_id_base64,
        virtual_ip,
        session_established: path.session_established,
        active_endpoint: path.active_endpoint,
        active_candidate_kind: path.active_candidate_kind,
        path_reason: path.path_reason,
        last_latency_microseconds: path.last_latency_microseconds,
        tx_packets_total: path.tx_packets_total,
        tx_bytes_total: path.tx_bytes_total,
        rx_packets_total: path.rx_packets_total,
        rx_bytes_total: path.rx_bytes_total,
    }
}

fn bounded_count(value: usize) -> Result<u64> {
    u64::try_from(value).map_err(|_| AgentError::Ipc)
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
