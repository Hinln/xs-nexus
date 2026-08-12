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
    let prepared = match prepare_authenticated_response(state, authenticated, message).await {
        Ok(prepared) => prepared,
        Err(error_code) => {
            send_error(socket, error_code).await;
            return false;
        }
    };
    send_json(socket, &prepared.message, chunked_transport)
        .await
        .is_ok()
}

struct PreparedControlResponse {
    message: ControlServerMessage,
    _configuration_permit: Option<OwnedSemaphorePermit>,
}

async fn prepare_authenticated_response(
    state: &AppState,
    authenticated: &AuthenticatedNode,
    message: ControlClientMessage,
) -> Result<PreparedControlResponse, &'static str> {
    match message {
        ControlClientMessage::Sync { last_version } => {
            prepare_synchronization_response(state, authenticated, last_version).await
        }
        ControlClientMessage::AdvertiseCandidates {
            advertisement,
            signature_base64,
        } => {
            prepare_candidate_advertisement_response(
                state,
                authenticated,
                advertisement,
                &signature_base64,
            )
            .await
        }
        ControlClientMessage::AdvertiseSubnetRoutes {
            advertisement,
            signature_base64,
        } => {
            prepare_subnet_route_advertisement_response(
                state,
                authenticated,
                advertisement,
                &signature_base64,
            )
            .await
        }
        ControlClientMessage::ReportRuntime {
            report,
            signature_base64,
        } => crate::updates::record_runtime_report(state, authenticated, report, &signature_base64)
            .await
            .map(|directive| PreparedControlResponse {
                message: ControlServerMessage::UpdateDirective { directive },
                _configuration_permit: None,
            })
            .map_err(|_| "runtime_report_rejected"),
        ControlClientMessage::ReportTelemetry {
            report,
            signature_base64,
        } => crate::telemetry::record_agent_report(state, authenticated, report, &signature_base64)
            .await
            .map(|sequence| PreparedControlResponse {
                message: ControlServerMessage::TelemetryAccepted { sequence },
                _configuration_permit: None,
            })
            .map_err(|_| "telemetry_report_rejected"),
        ControlClientMessage::Authenticate { .. } => Err("invalid_control_message"),
    }
}

async fn prepare_synchronization_response(
    state: &AppState,
    authenticated: &AuthenticatedNode,
    last_version: u64,
) -> Result<PreparedControlResponse, &'static str> {
    let permit = state
        .acquire_configuration_send()
        .await
        .ok_or("control_unavailable")?;
    let configuration = crate::service::latest_configuration(state, authenticated.network_id)
        .await
        .map_err(|_| "control_unavailable")?;
    let message = if configuration.version > last_version {
        ControlServerMessage::Configuration { configuration }
    } else {
        ControlServerMessage::UpToDate {
            version: configuration.version,
        }
    };
    Ok(PreparedControlResponse {
        message,
        _configuration_permit: Some(permit),
    })
}

async fn prepare_candidate_advertisement_response(
    state: &AppState,
    authenticated: &AuthenticatedNode,
    advertisement: crate::model::CandidateAdvertisement,
    signature_base64: &str,
) -> Result<PreparedControlResponse, &'static str> {
    let permit = state
        .acquire_configuration_send()
        .await
        .ok_or("control_unavailable")?;
    crate::service::advertise_candidates(state, authenticated, advertisement, signature_base64)
        .await
        .map(|configuration| PreparedControlResponse {
            message: ControlServerMessage::Configuration { configuration },
            _configuration_permit: Some(permit),
        })
        .map_err(|_| "candidate_advertisement_rejected")
}

async fn prepare_subnet_route_advertisement_response(
    state: &AppState,
    authenticated: &AuthenticatedNode,
    advertisement: crate::model::SubnetRouteAdvertisement,
    signature_base64: &str,
) -> Result<PreparedControlResponse, &'static str> {
    let permit = state
        .acquire_configuration_send()
        .await
        .ok_or("control_unavailable")?;
    crate::service::advertise_subnet_routes(state, authenticated, advertisement, signature_base64)
        .await
        .map(|configuration| PreparedControlResponse {
            message: ControlServerMessage::Configuration { configuration },
            _configuration_permit: Some(permit),
        })
        .map_err(|_| "subnet_route_advertisement_rejected")
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
