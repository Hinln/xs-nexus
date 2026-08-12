use axum::extract::ws::{Message, WebSocket};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
use tokio::sync::OwnedSemaphorePermit;
use tokio::time::{Duration, interval, timeout};
use xs_core::{
    CONTROL_MESSAGE_LIMIT_BYTES, CONTROL_TRANSFER_CHUNK_BYTES, CONTROL_TRANSFER_THRESHOLD_BYTES,
};

use crate::{
    model::{ControlClientMessage, ControlServerMessage},
    service::AuthenticatedNode,
    state::AppState,
};

const CONTROL_MESSAGE_LIMIT: usize = CONTROL_MESSAGE_LIMIT_BYTES;
const CONTROL_SEND_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) async fn serve(
    mut socket: WebSocket,
    state: AppState,
    _session_permit: OwnedSemaphorePermit,
    chunked_transport: bool,
) {
    let mut challenge = [0_u8; 32];
    if getrandom::fill(&mut challenge).is_err() {
        send_error(&mut socket, "control_unavailable").await;
        return;
    }
    if send_challenge(&mut socket, &challenge).await.is_err() {
        return;
    }

    let Some(authenticated) = authenticate_socket(&mut socket, &state, &challenge).await else {
        return;
    };
    let mut configuration_events = state.subscribe_configuration_events();
    let mut update_events = state.subscribe_update_events();
    if send_initial_configuration(&mut socket, &state, &authenticated, chunked_transport)
        .await
        .is_err()
    {
        return;
    }
    state.mark_node_online(authenticated.node_id).await;
    if crate::service::record_control_connected(&state, &authenticated.node_id)
        .await
        .is_err()
    {
        tracing::warn!(event = "control_presence_write_failed", state = "connected");
    }
    serve_authenticated_loop(
        &mut socket,
        &state,
        &authenticated,
        chunked_transport,
        &mut configuration_events,
        &mut update_events,
    )
    .await;
    if state.mark_node_offline(authenticated.node_id).await
        && crate::service::record_control_disconnected(&state, &authenticated.node_id)
            .await
            .is_err()
    {
        tracing::warn!(
            event = "control_presence_write_failed",
            state = "disconnected"
        );
    }
}

async fn send_challenge(socket: &mut WebSocket, challenge: &[u8; 32]) -> Result<(), axum::Error> {
    send_json(
        socket,
        &ControlServerMessage::Challenge {
            challenge_base64: URL_SAFE_NO_PAD.encode(challenge),
        },
        false,
    )
    .await
}

async fn authenticate_socket(
    socket: &mut WebSocket,
    state: &AppState,
    challenge: &[u8; 32],
) -> Option<AuthenticatedNode> {
    let Some(message) = timeout(Duration::from_secs(10), socket.recv())
        .await
        .ok()
        .flatten()
        .and_then(Result::ok)
    else {
        return reject_authentication(socket, state).await;
    };
    let Message::Text(text) = message else {
        return reject_authentication(socket, state).await;
    };
    if text.len() > 4096 {
        return reject_authentication(socket, state).await;
    }
    let Ok(ControlClientMessage::Authenticate {
        node_id_base64,
        credential_base64,
        signature_base64,
    }) = serde_json::from_str(&text)
    else {
        return reject_authentication(socket, state).await;
    };

    let Ok(authenticated) = crate::service::authenticate_control(
        state,
        challenge,
        &node_id_base64,
        &credential_base64,
        &signature_base64,
    )
    .await
    else {
        return reject_authentication(socket, state).await;
    };
    Some(authenticated)
}

async fn reject_authentication(
    socket: &mut WebSocket,
    state: &AppState,
) -> Option<AuthenticatedNode> {
    state.record_control_auth_failure();
    send_error(socket, "authentication_failed").await;
    None
}

async fn send_initial_configuration(
    socket: &mut WebSocket,
    state: &AppState,
    authenticated: &AuthenticatedNode,
    chunked_transport: bool,
) -> Result<(), axum::Error> {
    let Some(_configuration_permit) = state.acquire_configuration_send().await else {
        send_error(socket, "control_unavailable").await;
        return Err(control_error("configuration transfer unavailable"));
    };
    let Ok(configuration) =
        crate::service::latest_configuration(state, authenticated.network_id).await
    else {
        send_error(socket, "control_unavailable").await;
        return Err(axum::Error::new(std::io::Error::other(
            "configuration unavailable",
        )));
    };
    send_json(
        socket,
        &ControlServerMessage::Authenticated {
            node_id_base64: authenticated.node_id_base64.clone(),
            configuration,
        },
        chunked_transport,
    )
    .await
}

async fn serve_authenticated_loop(
    socket: &mut WebSocket,
    state: &AppState,
    authenticated: &AuthenticatedNode,
    chunked_transport: bool,
    configuration_events: &mut tokio::sync::broadcast::Receiver<uuid::Uuid>,
    update_events: &mut tokio::sync::broadcast::Receiver<uuid::Uuid>,
) {
    enum Event {
        Heartbeat,
        Configuration(Result<uuid::Uuid, tokio::sync::broadcast::error::RecvError>),
        Update(Result<uuid::Uuid, tokio::sync::broadcast::error::RecvError>),
        Incoming(Option<Result<Message, axum::Error>>),
    }

    let mut heartbeat = interval(Duration::from_secs(30));
    heartbeat.tick().await;
    loop {
        let event = tokio::select! {
            _ = heartbeat.tick() => Event::Heartbeat,
            event = configuration_events.recv() => Event::Configuration(event),
            event = update_events.recv() => Event::Update(event),
            incoming = socket.recv() => Event::Incoming(incoming),
        };
        match event {
            Event::Heartbeat => {
                if socket.send(Message::Ping(Vec::new().into())).await.is_err() {
                    return;
                }
            }
            Event::Configuration(event) => {
                let Ok(network_id) = event else {
                    continue;
                };
                if network_id == authenticated.network_id
                    && send_latest_configuration(socket, state, authenticated, chunked_transport)
                        .await
                        .is_err()
                {
                    return;
                }
            }
            Event::Update(event) => {
                let Ok(network_id) = event else {
                    continue;
                };
                if network_id == authenticated.network_id
                    && send_current_update_directive(
                        socket,
                        state,
                        authenticated,
                        chunked_transport,
                    )
                    .await
                    .is_err()
                {
                    return;
                }
            }
            Event::Incoming(incoming) => {
                let Some(Ok(message)) = incoming else {
                    return;
                };
                if !handle_authenticated_message(
                    socket,
                    state,
                    authenticated,
                    chunked_transport,
                    message,
                )
                .await
                {
                    return;
                }
            }
        }
    }
}

async fn send_current_update_directive(
    socket: &mut WebSocket,
    state: &AppState,
    authenticated: &AuthenticatedNode,
    chunked_transport: bool,
) -> Result<(), axum::Error> {
    let directive = crate::updates::current_update_directive(state, authenticated)
        .await
        .map_err(|_| axum::Error::new(std::io::Error::other("update directive unavailable")))?;
    send_json(
        socket,
        &ControlServerMessage::UpdateDirective { directive },
        chunked_transport,
    )
    .await
}

async fn send_latest_configuration(
    socket: &mut WebSocket,
    state: &AppState,
    authenticated: &AuthenticatedNode,
    chunked_transport: bool,
) -> Result<(), axum::Error> {
    let Some(_configuration_permit) = state.acquire_configuration_send().await else {
        return Err(control_error("configuration transfer unavailable"));
    };
    let configuration = crate::service::latest_configuration(state, authenticated.network_id)
        .await
        .map_err(|_| axum::Error::new(std::io::Error::other("configuration unavailable")))?;
    send_json(
        socket,
        &ControlServerMessage::Configuration { configuration },
        chunked_transport,
    )
    .await
}

async fn handle_authenticated_message(
    socket: &mut WebSocket,
    state: &AppState,
    authenticated: &AuthenticatedNode,
    chunked_transport: bool,
    message: Message,
) -> bool {
    match message {
        Message::Text(text) if text.len() <= CONTROL_MESSAGE_LIMIT => {
            handle_authenticated_text(socket, state, authenticated, chunked_transport, &text).await
        }
        Message::Ping(payload) => socket.send(Message::Pong(payload)).await.is_ok(),
        Message::Pong(_) => true,
        Message::Close(_) => false,
        _ => {
            send_error(socket, "invalid_control_message").await;
            false
        }
    }
}

async fn handle_authenticated_text(
    socket: &mut WebSocket,
    state: &AppState,
    authenticated: &AuthenticatedNode,
    chunked_transport: bool,
    text: &str,
) -> bool {
    let Ok(message) = serde_json::from_str::<ControlClientMessage>(text) else {
        send_error(socket, "invalid_control_message").await;
        return false;
    };
    let mut configuration_permit = None;
    let response = match message {
        ControlClientMessage::Sync { last_version } => {
            configuration_permit = state.acquire_configuration_send().await;
            if configuration_permit.is_none() {
                send_error(socket, "control_unavailable").await;
                return false;
            }
            let Ok(configuration) =
                crate::service::latest_configuration(state, authenticated.network_id).await
            else {
                send_error(socket, "control_unavailable").await;
                return false;
            };
            if configuration.version > last_version {
                ControlServerMessage::Configuration { configuration }
            } else {
                ControlServerMessage::UpToDate {
                    version: configuration.version,
                }
            }
        }
        ControlClientMessage::AdvertiseCandidates {
            advertisement,
            signature_base64,
        } => {
            configuration_permit = state.acquire_configuration_send().await;
            if configuration_permit.is_none() {
                send_error(socket, "control_unavailable").await;
                return false;
            }
            let Ok(configuration) = crate::service::advertise_candidates(
                state,
                authenticated,
                advertisement,
                &signature_base64,
            )
            .await
            else {
                send_error(socket, "candidate_advertisement_rejected").await;
                return false;
            };
            ControlServerMessage::Configuration { configuration }
        }
        ControlClientMessage::AdvertiseSubnetRoutes {
            advertisement,
            signature_base64,
        } => {
            configuration_permit = state.acquire_configuration_send().await;
            if configuration_permit.is_none() {
                send_error(socket, "control_unavailable").await;
                return false;
            }
            let Ok(configuration) = crate::service::advertise_subnet_routes(
                state,
                authenticated,
                advertisement,
                &signature_base64,
            )
            .await
            else {
                send_error(socket, "subnet_route_advertisement_rejected").await;
                return false;
            };
            ControlServerMessage::Configuration { configuration }
        }
        ControlClientMessage::ReportRuntime {
            report,
            signature_base64,
        } => {
            let Ok(directive) = crate::updates::record_runtime_report(
                state,
                authenticated,
                report,
                &signature_base64,
            )
            .await
            else {
                send_error(socket, "runtime_report_rejected").await;
                return false;
            };
            ControlServerMessage::UpdateDirective { directive }
        }
        ControlClientMessage::ReportTelemetry {
            report,
            signature_base64,
        } => {
            let Ok(sequence) = crate::telemetry::record_agent_report(
                state,
                authenticated,
                report,
                &signature_base64,
            )
            .await
            else {
                send_error(socket, "telemetry_report_rejected").await;
                return false;
            };
            ControlServerMessage::TelemetryAccepted { sequence }
        }
        ControlClientMessage::Authenticate { .. } => {
            send_error(socket, "invalid_control_message").await;
            return false;
        }
    };
    let sent = send_json(socket, &response, chunked_transport)
        .await
        .is_ok();
    drop(configuration_permit);
    sent
}

async fn send_json(
    socket: &mut WebSocket,
    message: &ControlServerMessage,
    chunked_transport: bool,
) -> Result<(), axum::Error> {
    let encoded = serde_json::to_vec(message).map_err(axum::Error::new)?;
    if encoded.len() > CONTROL_MESSAGE_LIMIT {
        return Err(control_error("control message exceeds configured limit"));
    }
    timeout(
        CONTROL_SEND_TIMEOUT,
        send_encoded_json(socket, encoded, chunked_transport),
    )
    .await
    .map_err(|_| control_error("control message send timed out"))?
}

async fn send_encoded_json(
    socket: &mut WebSocket,
    encoded: Vec<u8>,
    chunked_transport: bool,
) -> Result<(), axum::Error> {
    if encoded.len() <= CONTROL_TRANSFER_THRESHOLD_BYTES {
        return send_encoded(socket, encoded).await;
    }
    if !chunked_transport {
        return Err(control_error("control transport upgrade required"));
    }
    send_chunked(socket, &encoded).await
}

async fn send_chunked(socket: &mut WebSocket, encoded: &[u8]) -> Result<(), axum::Error> {
    let mut transfer_id = [0_u8; 16];
    getrandom::fill(&mut transfer_id).map_err(axum::Error::new)?;
    let transfer_id_base64 = URL_SAFE_NO_PAD.encode(transfer_id);
    let chunk_count = encoded.len().div_ceil(CONTROL_TRANSFER_CHUNK_BYTES);
    let start = ControlServerMessage::TransferStart {
        transfer_id_base64: transfer_id_base64.clone(),
        total_bytes: u32::try_from(encoded.len())
            .map_err(|_| control_error("control transfer length overflow"))?,
        chunk_count: u16::try_from(chunk_count)
            .map_err(|_| control_error("control transfer chunk overflow"))?,
        sha256_base64: URL_SAFE_NO_PAD.encode(Sha256::digest(encoded)),
    };
    send_single_json(socket, &start).await?;
    for (index, chunk) in encoded.chunks(CONTROL_TRANSFER_CHUNK_BYTES).enumerate() {
        send_single_json(
            socket,
            &ControlServerMessage::TransferChunk {
                transfer_id_base64: transfer_id_base64.clone(),
                index: u16::try_from(index)
                    .map_err(|_| control_error("control transfer index overflow"))?,
                data_base64: URL_SAFE_NO_PAD.encode(chunk),
            },
        )
        .await?;
    }
    send_single_json(
        socket,
        &ControlServerMessage::TransferEnd { transfer_id_base64 },
    )
    .await
}

async fn send_single_json(
    socket: &mut WebSocket,
    message: &ControlServerMessage,
) -> Result<(), axum::Error> {
    let encoded = serde_json::to_vec(message).map_err(axum::Error::new)?;
    if encoded.len() > CONTROL_TRANSFER_THRESHOLD_BYTES {
        return Err(control_error(
            "control transfer envelope exceeds configured limit",
        ));
    }
    send_encoded(socket, encoded).await
}

async fn send_encoded(socket: &mut WebSocket, encoded: Vec<u8>) -> Result<(), axum::Error> {
    let encoded = String::from_utf8(encoded).map_err(axum::Error::new)?;
    socket.send(Message::Text(encoded.into())).await
}

fn control_error(message: &'static str) -> axum::Error {
    axum::Error::new(std::io::Error::other(message))
}

async fn send_error(socket: &mut WebSocket, code: &'static str) {
    let _ = send_json(
        socket,
        &ControlServerMessage::Error {
            code: code.to_owned(),
        },
        false,
    )
    .await;
    let _ = socket.send(Message::Close(None)).await;
}
