use std::{
    net::Ipv4Addr,
    os::unix::fs::{FileTypeExt as _, PermissionsExt as _},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    net::{UnixListener, UnixStream},
    sync::{Semaphore, watch},
    task::JoinSet,
    time::timeout,
};
use xs_core::{
    LocalAgentDiagnostics, LocalAgentRequest, LocalAgentResponse, LocalAgentStatus, LocalPeerStatus,
};

use crate::{
    error::{AgentError, Result},
    health::AgentHealth,
    state::NodeState,
    storage::ensure_private_directory,
};

const LOCAL_PROTOCOL_VERSION: u8 = 1;
const MAX_REQUEST_BYTES: usize = 4096;
const MAX_RESPONSE_BYTES: usize = 512 * 1024;
const MAX_PEERS_PER_RESPONSE: usize = 512;
const MAX_CONNECTIONS: usize = 16;
const IO_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Debug)]
pub struct IpcContext {
    pub interface_name: String,
    pub interface_index: u32,
}

/// Serves bounded, read-only local status requests over a private Unix socket.
///
/// # Errors
///
/// Returns [`AgentError::Ipc`] when the socket path is unsafe, another Agent is listening,
/// or the listener fails.
pub async fn run_ipc_server(
    socket_path: PathBuf,
    state: Arc<tokio::sync::RwLock<NodeState>>,
    health: Arc<AgentHealth>,
    context: IpcContext,
    mut shutdown: watch::Receiver<bool>,
) -> Result<()> {
    let listener = bind_private_socket(&socket_path).await?;
    let _guard = SocketGuard(socket_path);
    let permits = Arc::new(Semaphore::new(MAX_CONNECTIONS));
    let mut connections = JoinSet::new();

    loop {
        tokio::select! {
            result = shutdown.changed() => {
                if result.is_err() || *shutdown.borrow() {
                    break;
                }
            }
            accepted = listener.accept() => {
                let (stream, _) = accepted.map_err(|_| AgentError::Ipc)?;
                let Ok(permit) = Arc::clone(&permits).try_acquire_owned() else {
                    drop(stream);
                    continue;
                };
                let state = Arc::clone(&state);
                let health = Arc::clone(&health);
                let context = context.clone();
                connections.spawn(async move {
                    let _permit = permit;
                    let _ = handle_connection(stream, &state, &health, &context).await;
                });
            }
            completed = connections.join_next(), if !connections.is_empty() => {
                let _ = completed;
            }
        }
    }

    connections.abort_all();
    while connections.join_next().await.is_some() {}
    Ok(())
}

async fn bind_private_socket(path: &Path) -> Result<UnixListener> {
    let parent = path.parent().ok_or(AgentError::Ipc)?;
    ensure_private_directory(parent).map_err(|_| AgentError::Ipc)?;
    match path.symlink_metadata() {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.file_type().is_socket() {
                return Err(AgentError::Ipc);
            }
            match timeout(Duration::from_millis(250), UnixStream::connect(path)).await {
                Ok(Err(error))
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::NotFound
                    ) =>
                {
                    std::fs::remove_file(path).map_err(|_| AgentError::Ipc)?;
                }
                Ok(Ok(_) | Err(_)) | Err(_) => return Err(AgentError::Ipc),
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(AgentError::Ipc),
    }

    let listener = UnixListener::bind(path).map_err(|_| AgentError::Ipc)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|_| AgentError::Ipc)?;
    Ok(listener)
}

async fn handle_connection(
    mut stream: UnixStream,
    state: &tokio::sync::RwLock<NodeState>,
    health: &AgentHealth,
    context: &IpcContext,
) -> Result<()> {
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

async fn read_request(stream: &mut UnixStream) -> Result<LocalAgentRequest> {
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
                .map(|node| {
                    Ok(LocalPeerStatus {
                        node_id_base64: node.node_id_base64.clone(),
                        virtual_ip: node
                            .virtual_ip
                            .parse::<Ipv4Addr>()
                            .map_err(|_| AgentError::State)?,
                        credential_not_after: node.credential_not_after,
                        role_bitmap: node.role_bitmap,
                        tags: node.tags.clone(),
                    })
                })
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
            },
        }),
    }
}

fn error_response(code: &str) -> LocalAgentResponse {
    LocalAgentResponse::Error {
        schema_version: LOCAL_PROTOCOL_VERSION,
        code: code.to_owned(),
    }
}

struct SocketGuard(PathBuf);

impl Drop for SocketGuard {
    fn drop(&mut self) {
        if self
            .0
            .symlink_metadata()
            .is_ok_and(|metadata| metadata.file_type().is_socket())
        {
            let _ = std::fs::remove_file(&self.0);
        }
    }
}
