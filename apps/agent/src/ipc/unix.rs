use std::{
    os::unix::fs::{FileTypeExt as _, PermissionsExt as _},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use tokio::{
    net::{UnixListener, UnixStream},
    sync::{Semaphore, watch},
    task::JoinSet,
    time::timeout,
};

use super::{IpcContext, MAX_CONNECTIONS, handle_connection};
use crate::{
    error::{AgentError, Result},
    health::AgentHealth,
    state::NodeState,
    storage::ensure_private_directory,
};

pub(super) async fn run(
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
