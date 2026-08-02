use std::{sync::Arc, time::Duration};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use ed25519_dalek::Signer as _;
use futures_util::{SinkExt as _, StreamExt as _};
use tokio::{
    sync::watch,
    time::{Instant, interval_at, timeout},
};
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{Message, protocol::WebSocketConfig},
};
use xs_core::{
    AgentRuntimeReport, AgentUpdateState, CandidateAdvertisement, ControlClientMessage,
    ControlServerMessage, ReleaseVersion, SignedConfiguration, SubnetRouteAdvertisement,
    UpdateChannel, UpdateDirective, agent_runtime_report_signing_input,
};

use crate::{
    config::AgentConfig,
    error::{AgentError, Result},
    health::AgentHealth,
    state::{NodeState, decode_fixed},
    storage::{Identity, write_json},
    updates::stage_update,
};

const CONTROL_AUTHENTICATION_DOMAIN: &[u8] = b"XS Nexus control authentication v1";
const CANDIDATE_ADVERTISEMENT_DOMAIN: &[u8] = b"XS Nexus candidate advertisement v1";
const SUBNET_ROUTE_ADVERTISEMENT_DOMAIN: &[u8] = b"XS Nexus subnet route advertisement v1";
const CONTROL_MESSAGE_LIMIT: usize = 64 * 1024;

#[derive(Clone, Debug)]
struct RuntimeUpdateStatus {
    state: AgentUpdateState,
    observed_release_id: Option<uuid::Uuid>,
    last_error_code: Option<String>,
}

impl RuntimeUpdateStatus {
    const fn idle() -> Self {
        Self {
            state: AgentUpdateState::Idle,
            observed_release_id: None,
            last_error_code: None,
        }
    }
}

pub async fn run_control_loop(
    config: AgentConfig,
    identity: Arc<Identity>,
    state: Arc<tokio::sync::RwLock<NodeState>>,
    health: Arc<AgentHealth>,
    mut candidates: watch::Receiver<Option<CandidateAdvertisement>>,
    mut subnet_routes: watch::Receiver<Option<SubnetRouteAdvertisement>>,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut backoff = Duration::from_secs(1);
    loop {
        if *shutdown.borrow() {
            return;
        }
        match control_session(
            &config,
            &identity,
            &state,
            &health,
            &mut candidates,
            &mut subnet_routes,
            &mut shutdown,
        )
        .await
        {
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
    candidates: &mut watch::Receiver<Option<CandidateAdvertisement>>,
    subnet_routes: &mut watch::Receiver<Option<SubnetRouteAdvertisement>>,
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

    authenticate_control(config, &mut socket, identity, state).await?;
    health.set_controller_connected(true);
    let mut update_status = RuntimeUpdateStatus::idle();
    let mut attempted_update = None;
    send_runtime_report(&mut socket, identity, config, state, &update_status).await?;
    let initial_advertisement = candidates.borrow().clone();
    if let Some(advertisement) = initial_advertisement {
        send_candidate_advertisement(&mut socket, identity, advertisement).await?;
    }
    let initial_subnet_routes = subnet_routes.borrow().clone();
    if let Some(advertisement) = initial_subnet_routes {
        send_subnet_route_advertisement(&mut socket, identity, advertisement).await?;
    }

    let synchronization_period = Duration::from_secs(config.control_sync_interval_seconds);
    let first_synchronization = Instant::now() + Duration::from_secs(1).min(synchronization_period);
    let mut synchronization = interval_at(first_synchronization, synchronization_period);
    synchronization.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

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
                send_runtime_report(&mut socket, identity, config, state, &update_status).await?;
            }
            changed = candidates.changed() => {
                if changed.is_err() {
                    return Err(AgentError::Control);
                }
                let advertisement = candidates.borrow_and_update().clone();
                if let Some(advertisement) = advertisement {
                    send_candidate_advertisement(&mut socket, identity, advertisement).await?;
                }
            }
            changed = subnet_routes.changed() => {
                if changed.is_err() {
                    return Err(AgentError::Control);
                }
                let advertisement = subnet_routes.borrow_and_update().clone();
                if let Some(advertisement) = advertisement {
                    send_subnet_route_advertisement(&mut socket, identity, advertisement).await?;
                }
            }
            incoming = receive_json(&mut socket) => {
                match incoming? {
                    ControlServerMessage::Configuration { configuration } => {
                        apply_configuration(state, identity, configuration, &config.node_state_path()).await?;
                        send_runtime_report(&mut socket, identity, config, state, &update_status).await?;
                    }
                    ControlServerMessage::UpToDate { version }
                        if version == state.read().await.configuration.version => {}
                    ControlServerMessage::UpdateDirective { directive } => {
                        process_update_directive(
                            &mut socket,
                            identity,
                            config,
                            state,
                            health,
                            directive,
                            &mut update_status,
                            &mut attempted_update,
                        ).await?;
                    }
                    ControlServerMessage::Error { .. }
                    | ControlServerMessage::Challenge { .. }
                    | ControlServerMessage::Authenticated { .. }
                    | ControlServerMessage::UpToDate { .. } => return Err(AgentError::Control),
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn process_update_directive<S>(
    socket: &mut tokio_tungstenite::WebSocketStream<S>,
    identity: &Identity,
    config: &AgentConfig,
    state: &tokio::sync::RwLock<NodeState>,
    health: &AgentHealth,
    directive: Option<UpdateDirective>,
    status: &mut RuntimeUpdateStatus,
    attempted_update: &mut Option<(UpdateChannel, u64, uuid::Uuid)>,
) -> Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let Some(directive) = directive else {
        return Ok(());
    };
    let update_channel = {
        let state = state.read().await;
        assigned_update_channel(&state, config.update_channel)
    };
    let attempt = (
        update_channel,
        directive.policy_generation,
        directive.release_id,
    );
    if (status.state == AgentUpdateState::Staged
        && status.observed_release_id == Some(directive.release_id))
        || *attempted_update == Some(attempt)
    {
        return Ok(());
    }
    *attempted_update = Some(attempt);
    status.state = AgentUpdateState::Downloading;
    status.observed_release_id = Some(directive.release_id);
    status.last_error_code = None;
    send_runtime_report(socket, identity, config, state, status).await?;

    match stage_update(config, update_channel, &directive).await {
        Ok(request) => {
            status.state = AgentUpdateState::Staged;
            status.observed_release_id = Some(request.release_id);
            status.last_error_code = None;
            health.set_last_error(None);
        }
        Err(error) => {
            status.state = AgentUpdateState::Failed;
            status.observed_release_id = Some(directive.release_id);
            status.last_error_code = Some(error.code().to_owned());
            health.set_last_error(Some(error.code()));
        }
    }
    send_runtime_report(socket, identity, config, state, status).await
}

async fn send_runtime_report<S>(
    socket: &mut tokio_tungstenite::WebSocketStream<S>,
    identity: &Identity,
    config: &AgentConfig,
    state: &tokio::sync::RwLock<NodeState>,
    status: &RuntimeUpdateStatus,
) -> Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let (network_id, node_id_base64, update_channel) = {
        let state = state.read().await;
        (
            state.network_id,
            state.node_id_base64.clone(),
            assigned_update_channel(&state, config.update_channel),
        )
    };
    let report = AgentRuntimeReport {
        schema_version: 1,
        network_id,
        node_id_base64,
        agent_version: env!("CARGO_PKG_VERSION")
            .parse::<ReleaseVersion>()
            .map_err(|_| AgentError::State)?,
        platform: std::env::consts::OS.to_owned(),
        architecture: std::env::consts::ARCH.to_owned(),
        update_channel,
        update_state: status.state,
        observed_release_id: status.observed_release_id,
        last_error_code: status.last_error_code.clone(),
        generated_at: Utc::now(),
    };
    let signing_input =
        agent_runtime_report_signing_input(&report).map_err(|_| AgentError::Control)?;
    let signature = identity.signing_key().sign(&signing_input);
    send_json(
        socket,
        &ControlClientMessage::ReportRuntime {
            report,
            signature_base64: URL_SAFE_NO_PAD.encode(signature.to_bytes()),
        },
    )
    .await
}

fn assigned_update_channel(state: &NodeState, fallback: UpdateChannel) -> UpdateChannel {
    state
        .configuration_payload
        .nodes
        .iter()
        .find(|node| node.node_id_base64 == state.node_id_base64)
        .and_then(|node| node.update_channel)
        .unwrap_or(fallback)
}

async fn authenticate_control<S>(
    config: &AgentConfig,
    socket: &mut tokio_tungstenite::WebSocketStream<S>,
    identity: &Identity,
    state: &tokio::sync::RwLock<NodeState>,
) -> Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let challenge = match timeout(Duration::from_secs(10), receive_json(socket)).await {
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
        socket,
        &ControlClientMessage::Authenticate {
            node_id_base64: node_id_base64.clone(),
            credential_base64,
            signature_base64: URL_SAFE_NO_PAD.encode(signature.to_bytes()),
        },
    )
    .await?;

    let initial = match timeout(Duration::from_secs(10), receive_json(socket)).await {
        Ok(Ok(ControlServerMessage::Authenticated {
            node_id_base64: authenticated_node_id,
            configuration,
        })) if authenticated_node_id == node_id_base64 => configuration,
        _ => return Err(AgentError::Control),
    };
    apply_configuration(state, identity, initial, &config.node_state_path()).await
}

async fn send_candidate_advertisement<S>(
    socket: &mut tokio_tungstenite::WebSocketStream<S>,
    identity: &Identity,
    advertisement: CandidateAdvertisement,
) -> Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let payload = serde_json::to_vec(&advertisement).map_err(|_| AgentError::Control)?;
    let mut signing_input =
        Vec::with_capacity(CANDIDATE_ADVERTISEMENT_DOMAIN.len() + payload.len());
    signing_input.extend_from_slice(CANDIDATE_ADVERTISEMENT_DOMAIN);
    signing_input.extend_from_slice(&payload);
    let signature = identity.signing_key().sign(&signing_input);
    send_json(
        socket,
        &ControlClientMessage::AdvertiseCandidates {
            advertisement,
            signature_base64: URL_SAFE_NO_PAD.encode(signature.to_bytes()),
        },
    )
    .await
}

async fn send_subnet_route_advertisement<S>(
    socket: &mut tokio_tungstenite::WebSocketStream<S>,
    identity: &Identity,
    advertisement: SubnetRouteAdvertisement,
) -> Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let payload = serde_json::to_vec(&advertisement).map_err(|_| AgentError::Control)?;
    let mut signing_input =
        Vec::with_capacity(SUBNET_ROUTE_ADVERTISEMENT_DOMAIN.len() + payload.len());
    signing_input.extend_from_slice(SUBNET_ROUTE_ADVERTISEMENT_DOMAIN);
    signing_input.extend_from_slice(&payload);
    let signature = identity.signing_key().sign(&signing_input);
    send_json(
        socket,
        &ControlClientMessage::AdvertiseSubnetRoutes {
            advertisement,
            signature_base64: URL_SAFE_NO_PAD.encode(signature.to_bytes()),
        },
    )
    .await
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
