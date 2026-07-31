use std::{path::PathBuf, sync::Arc};

use tokio::{
    sync::{Semaphore, watch},
    task::JoinSet,
};
use xs_windows_local_ipc::create_agent_pipe_server;

use super::{IpcContext, MAX_CONNECTIONS, handle_connection};
use crate::{
    error::{AgentError, Result},
    health::AgentHealth,
    state::NodeState,
};

pub(super) async fn run(
    socket_path: PathBuf,
    state: Arc<tokio::sync::RwLock<NodeState>>,
    health: Arc<AgentHealth>,
    context: IpcContext,
    mut shutdown: watch::Receiver<bool>,
) -> Result<()> {
    if socket_path.as_os_str() != xs_windows_local_ipc::AGENT_PIPE_NAME {
        return Err(AgentError::Ipc);
    }
    let mut server = create_agent_pipe_server(true).map_err(|_| AgentError::Ipc)?;
    let permits = Arc::new(Semaphore::new(MAX_CONNECTIONS));
    let mut connections = JoinSet::new();

    loop {
        tokio::select! {
            result = shutdown.changed() => {
                if result.is_err() || *shutdown.borrow() {
                    break;
                }
            }
            connected = server.connect(), if permits.available_permits() > 0 => {
                connected.map_err(|_| AgentError::Ipc)?;
                let stream = server;
                server = create_agent_pipe_server(false).map_err(|_| AgentError::Ipc)?;
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
