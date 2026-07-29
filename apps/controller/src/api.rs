use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, State, WebSocketUpgrade},
    http::{HeaderMap, StatusCode, header},
    response::Response,
    routing::{get, post},
};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use tower_http::{
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer},
};

use crate::{
    error::ApiError,
    model::{CreateEnrollmentTokenRequest, CreateNetworkRequest, EnrollRequest, HealthResponse},
    state::AppState,
};

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health/live", get(live))
        .route("/health/ready", get(ready))
        .route("/v1/admin/networks", post(create_network))
        .route("/v1/admin/enrollment-tokens", post(create_enrollment_token))
        .route("/v1/enroll", post(enroll))
        .route("/v1/control", get(control))
        .fallback(not_found)
        .layer(DefaultBodyLimit::max(64 * 1024))
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().include_headers(false))
                .on_response(DefaultOnResponse::new().include_headers(false)),
        )
        .with_state(state)
}

async fn live() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        database: "unchecked",
    })
}

async fn ready(State(state): State<AppState>) -> Result<Json<HealthResponse>, ApiError> {
    sqlx::query("SELECT 1")
        .execute(&state.pool)
        .await
        .map_err(|_| ApiError::unavailable())?;
    Ok(Json(HealthResponse {
        status: "ok",
        database: "ok",
    }))
}

async fn create_network(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateNetworkRequest>,
) -> Result<(StatusCode, Json<crate::model::NetworkResponse>), ApiError> {
    authenticate_admin(&state, &headers)?;
    let response = crate::service::create_network(&state, request).await?;
    Ok((StatusCode::CREATED, Json(response)))
}

async fn create_enrollment_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateEnrollmentTokenRequest>,
) -> Result<(StatusCode, Json<crate::model::EnrollmentTokenResponse>), ApiError> {
    authenticate_admin(&state, &headers)?;
    let response = crate::service::create_enrollment_token(&state, request).await?;
    Ok((StatusCode::CREATED, Json(response)))
}

async fn enroll(
    State(state): State<AppState>,
    Json(request): Json<EnrollRequest>,
) -> Result<(StatusCode, Json<crate::model::EnrollResponse>), ApiError> {
    let response = crate::service::enroll_node(&state, request).await?;
    Ok((StatusCode::CREATED, Json(response)))
}

async fn control(State(state): State<AppState>, upgrade: WebSocketUpgrade) -> Response {
    upgrade
        .max_message_size(4096)
        .max_frame_size(4096)
        .on_upgrade(move |socket| crate::control::serve(socket, state))
}

async fn not_found() -> ApiError {
    ApiError::not_found()
}

fn authenticate_admin(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or_else(ApiError::unauthorized)?;
    if token.chars().count() < 32 || token.len() > 256 {
        return Err(ApiError::unauthorized());
    }

    let candidate: [u8; 32] = Sha256::digest(token.as_bytes()).into();
    if bool::from(candidate.ct_eq(&state.admin_token_hash)) {
        Ok(())
    } else {
        Err(ApiError::unauthorized())
    }
}
