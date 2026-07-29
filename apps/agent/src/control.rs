use std::{sync::Arc, time::Duration};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::Signer as _;
use futures_util::{SinkExt as _, StreamExt as _};
use tokio::{sync::watch, time::timeout};
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{Message, protocol::WebSocketConfig},
};
use xs_core::{ControlClientMessage, ControlServerMessage, SignedConfiguration};

use crate::{
    config::AgentConfig,
    error::{AgentError, Result},
    health::AgentHealth,
    state::{NodeState, decode_fixed},
    storage::{Identity, write_json},
};

const CONTROL_AUTHENTICATION_DOMAIN: &[u8] = b"XS Nexus control authentication v1";
const CONTROL_MESSAGE_LIMIT: usize = 4096;

pub async fn run_control_loop(
    config: AgentConfig,
    identity: Arc<Identity>,
    state: Arc<tokio::sync::RwLock<NodeState>>,
    health: Arc<AgentHealth>,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut backoff = Duration::from_secs(1);
    loop {
        if *shutdown.borrow() {
            return;
        }
        match control_session(&config, &identity, &state, &health, &mut shutdown).await {
            Ok(()) if *shutdown.borrow() => return,
            Ok(()) | Err(_) => {
                health.set_controller_connected(false);
                health.set_last_error(Some(AgentError::Control.code()));
            }
        }

        tokio::select! {
            result = shutdown.changed() => {
                if result.is_err() || *shutdown.borrow() {
                    return;
                }
            }
            () = tokio::time::sleep(backoff) => {}
        }
        backoff = backoff.saturating_mul(2).min(Duration::from_secs(30));
    }
}

async fn control_session(
    config: &AgentConfig,
    identity: &Identity,
    state: &tokio::sync::RwLock<NodeState>,
    health: &AgentHealth,
    shutdown: &mut watch::Receiver<bool>,
) -> Result<()> {
    let websocket_config = WebSocketConfig::default()
        .max_message_size(Some(CONTROL_MESSAGE_LIMIT))
        .max_frame_size(Some(CONTROL_MESSAGE_LIMIT));
    let control_url = config.control_url()?.to_string();
    let (mut socket, _) = timeout(
        Duration::from_secs(15),
        connect_async_with_config(control_url, Some(websocket_config), false),
    )
    .await
    .map_err(|_| AgentError::Control)?
    .map_err(|_| AgentError::Control)?;

    let challenge = match timeout(Duration::from_secs(10), receive_json(&mut socket)).await {
        Ok(Ok(ControlServerMessage::Challenge { challenge_base64 })) => {
            decode_fixed::<32>(&challenge_base64)?
        }
        _ => return Err(AgentError::Control),
    };
    let (node_id_base64, credential_base64, node_id) = {
        let state = state.read().await;
        (
            state.node_id_base64.clone(),
            state.credential_base64.clone(),
            decode_fixed::<16>(&state.node_id_base64)?,
        )
    };
    let mut authentication_input =
        Vec::with_capacity(CONTROL_AUTHENTICATION_DOMAIN.len() + challenge.len() + node_id.len());
    authentication_input.extend_from_slice(CONTROL_AUTHENTICATION_DOMAIN);
    authentication_input.extend_from_slice(&challenge);
    authentication_input.extend_from_slice(&node_id);
    let signature = identity.signing_key().sign(&authentication_input);
    send_json(
        &mut socket,
        &ControlClientMessage::Authenticate {
            node_id_base64: node_id_base64.clone(),
            credential_base64,
            signature_base64: URL_SAFE_NO_PAD.encode(signature.to_bytes()),
        },
    )
    .await?;

    let initial = match timeout(Duration::from_secs(10), receive_json(&mut socket)).await {
        Ok(Ok(ControlServerMessage::Authenticated {
            node_id_base64: authenticated_node_id,
            configuration,
        })) if authenticated_node_id == node_id_base64 => configuration,
        _ => return Err(AgentError::Control),
    };
    apply_configuration(state, identity, initial, &config.node_state_path()).await?;
    health.set_controller_connected(true);

    let mut synchronization =
        tokio::time::interval(Duration::from_secs(config.control_sync_interval_seconds));
    synchronization.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    synchronization.tick().await;

    loop {
        tokio::select! {
            result = shutdown.changed() => {
                if result.is_err() || *shutdown.borrow() {
                    let _ = socket.send(Message::Close(None)).await;
                    return Ok(());
                }
            }
            _ = synchronization.tick() => {
                let last_version = state.read().await.configuration.version;
                send_json(&mut socket, &ControlClientMessage::Sync { last_version }).await?;
            }
            incoming = receive_json(&mut socket) => {
                match incoming? {
                    ControlServerMessage::Configuration { configuration } => {
                        apply_configuration(state, identity, configuration, &config.node_state_path()).await?;
                    }
                    ControlServerMessage::UpToDate { version }
                        if version == state.read().await.configuration.version => {}
                    ControlServerMessage::Error { .. }
                    | ControlServerMessage::Challenge { .. }
                    | ControlServerMessage::Authenticated { .. }
                    | ControlServerMessage::UpToDate { .. } => return Err(AgentError::Control),
                }
            }
        }
    }
}

async fn apply_configuration(
    state: &tokio::sync::RwLock<NodeState>,
    identity: &Identity,
    configuration: SignedConfiguration,
    state_path: &std::path::Path,
) -> Result<()> {
    let mut state = state.write().await;
    if state.apply_configuration(configuration, identity)? {
        write_json(state_path, &*state)?;
    }
    Ok(())
}

async fn send_json<S>(
    socket: &mut tokio_tungstenite::WebSocketStream<S>,
    message: &ControlClientMessage,
) -> Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let encoded = serde_json::to_string(message).map_err(|_| AgentError::Control)?;
    if encoded.len() > CONTROL_MESSAGE_LIMIT {
        return Err(AgentError::Control);
    }
    socket
        .send(Message::Text(encoded.into()))
        .await
        .map_err(|_| AgentError::Control)
}

async fn receive_json<S>(
    socket: &mut tokio_tungstenite::WebSocketStream<S>,
) -> Result<ControlServerMessage>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    loop {
        let message = socket
            .next()
            .await
            .ok_or(AgentError::Control)?
            .map_err(|_| AgentError::Control)?;
        match message {
            Message::Text(text) if text.len() <= CONTROL_MESSAGE_LIMIT => {
                return serde_json::from_str(&text).map_err(|_| AgentError::Control);
            }
            Message::Ping(payload) => socket
                .send(Message::Pong(payload))
                .await
                .map_err(|_| AgentError::Control)?,
            Message::Pong(_) => {}
            Message::Close(_) | Message::Binary(_) | Message::Frame(_) | Message::Text(_) => {
                return Err(AgentError::Control);
            }
        }
    }
}
