use std::{path::Path, time::Duration};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use futures_util::StreamExt as _;
use reqwest::redirect::Policy;
use xs_core::{EnrollRequest, EnrollResponse};

use crate::{
    config::AgentConfig,
    error::{AgentError, Result},
    state::NodeState,
    storage::{Identity, read_token, write_json},
};

const MAX_ENROLLMENT_RESPONSE_BYTES: usize = 64 * 1024;

/// Enrolls a new local identity and persists only a fully verified node state.
///
/// # Errors
///
/// Returns an Agent error when local state is unsafe, the request fails, the response exceeds
/// its bound, Controller trust validation fails, or the verified state cannot be persisted.
pub async fn enroll(config: &AgentConfig, token_path: &Path) -> Result<NodeState> {
    if config.node_state_path().symlink_metadata().is_ok() {
        return Err(AgentError::State);
    }
    let identity = Identity::load_or_create(&config.identity_path())?;
    let token = read_token(token_path)?;
    let request = EnrollRequest {
        token: token.to_string(),
        name: config.node_name.clone(),
        device_type: config.device_type.clone(),
        identity_public_key_base64: URL_SAFE_NO_PAD.encode(identity.public_key()),
    };
    let client = reqwest::Client::builder()
        .redirect(Policy::none())
        .timeout(Duration::from_secs(15))
        .user_agent(concat!("xs-agent/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| AgentError::Enrollment)?;
    let response = client
        .post(config.enrollment_url()?)
        .json(&request)
        .send()
        .await
        .map_err(|_| AgentError::Enrollment)?;
    if response.status() != reqwest::StatusCode::CREATED {
        return Err(AgentError::Enrollment);
    }

    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| AgentError::Enrollment)?;
        if body.len().saturating_add(chunk.len()) > MAX_ENROLLMENT_RESPONSE_BYTES {
            return Err(AgentError::Enrollment);
        }
        body.extend_from_slice(&chunk);
    }
    let response: EnrollResponse =
        serde_json::from_slice(&body).map_err(|_| AgentError::Enrollment)?;
    let normalized_controller = config.controller_url()?.to_string();
    let state = NodeState::from_enrollment(response, &identity, &normalized_controller)?;
    write_json(&config.node_state_path(), &state)?;
    std::fs::remove_file(token_path).map_err(|_| AgentError::State)?;
    Ok(state)
}
