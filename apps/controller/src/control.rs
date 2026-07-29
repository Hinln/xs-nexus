use axum::extract::ws::{Message, WebSocket};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use tokio::time::{Duration, interval, timeout};

use crate::{
    model::{ControlClientMessage, ControlServerMessage},
    service::AuthenticatedNode,
    state::AppState,
};

pub(crate) async fn serve(mut socket: WebSocket, state: AppState) {
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
    if send_initial_configuration(&mut socket, &state, &authenticated)
        .await
        .is_err()
    {
        return;
    }
    serve_authenticated_loop(&mut socket, &state, &authenticated).await;
}

async fn send_challenge(socket: &mut WebSocket, challenge: &[u8; 32]) -> Result<(), axum::Error> {
    send_json(
        socket,
        &ControlServerMessage::Challenge {
            challenge_base64: URL_SAFE_NO_PAD.encode(challenge),
        },
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
        send_error(socket, "authentication_failed").await;
        return None;
    };
    let Message::Text(text) = message else {
        send_error(socket, "authentication_failed").await;
        return None;
    };
    if text.len() > 4096 {
        send_error(socket, "authentication_failed").await;
        return None;
    }
    let Ok(ControlClientMessage::Authenticate {
        node_id_base64,
        credential_base64,
        signature_base64,
    }) = serde_json::from_str(&text)
    else {
        send_error(socket, "authentication_failed").await;
        return None;
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
        send_error(socket, "authentication_failed").await;
        return None;
    };
    Some(authenticated)
}

async fn send_initial_configuration(
    socket: &mut WebSocket,
    state: &AppState,
    authenticated: &AuthenticatedNode,
) -> Result<(), axum::Error> {
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
    )
    .await
}

async fn serve_authenticated_loop(
    socket: &mut WebSocket,
    state: &AppState,
    authenticated: &AuthenticatedNode,
) {
    let mut heartbeat = interval(Duration::from_secs(30));
    heartbeat.tick().await;
    loop {
        tokio::select! {
            _ = heartbeat.tick() => {
                if socket.send(Message::Ping(Vec::new().into())).await.is_err() {
                    return;
                }
            }
            incoming = socket.recv() => {
                let Some(Ok(message)) = incoming else {
                    return;
                };
                if !handle_authenticated_message(socket, state, authenticated, message).await {
                    return;
                }
            }
        }
    }
}

async fn handle_authenticated_message(
    socket: &mut WebSocket,
    state: &AppState,
    authenticated: &AuthenticatedNode,
    message: Message,
) -> bool {
    match message {
        Message::Text(text) if text.len() <= 4096 => {
            handle_sync(socket, state, authenticated, &text).await
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

async fn handle_sync(
    socket: &mut WebSocket,
    state: &AppState,
    authenticated: &AuthenticatedNode,
    text: &str,
) -> bool {
    let Ok(ControlClientMessage::Sync { last_version }) = serde_json::from_str(text) else {
        send_error(socket, "invalid_control_message").await;
        return false;
    };
    let Ok(configuration) =
        crate::service::latest_configuration(state, authenticated.network_id).await
    else {
        send_error(socket, "control_unavailable").await;
        return false;
    };
    let response = if configuration.version > last_version {
        ControlServerMessage::Configuration { configuration }
    } else {
        ControlServerMessage::UpToDate {
            version: configuration.version,
        }
    };
    send_json(socket, &response).await.is_ok()
}

async fn send_json(
    socket: &mut WebSocket,
    message: &ControlServerMessage,
) -> Result<(), axum::Error> {
    let encoded = serde_json::to_string(message).map_err(axum::Error::new)?;
    socket.send(Message::Text(encoded.into())).await
}

async fn send_error(socket: &mut WebSocket, code: &'static str) {
    let _ = send_json(
        socket,
        &ControlServerMessage::Error {
            code: code.to_owned(),
        },
    )
    .await;
    let _ = socket.send(Message::Close(None)).await;
}
