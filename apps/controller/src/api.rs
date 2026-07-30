use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, State, WebSocketUpgrade},
    http::{HeaderMap, StatusCode, header},
    response::Response,
    routing::{get, post, put},
};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use tower_http::{
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer},
};

use crate::{
    error::ApiError,
    model::{
        CreateEnrollmentTokenRequest, CreateNetworkRequest, EnrollRequest, ExplainAclRequest,
        HealthResponse, ReplaceAclPolicyRequest, ReplaceSubnetRoutesRequest, RevokeNodeRequest,
    },
    state::AppState,
};

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health/live", get(live))
        .route("/health/ready", get(ready))
        .route("/v1/admin/networks", post(create_network))
        .route("/v1/admin/enrollment-tokens", post(create_enrollment_token))
        .route(
            "/v1/admin/networks/{network_id}/acl",
            put(replace_acl_policy),
        )
        .route(
            "/v1/admin/networks/{network_id}/acl/explain",
            post(explain_acl),
        )
        .route(
            "/v1/admin/networks/{network_id}/subnet-route-suggestions",
            get(list_subnet_route_suggestions),
        )
        .route(
            "/v1/admin/networks/{network_id}/subnet-routes",
            put(replace_subnet_routes),
        )
        .route(
            "/v1/admin/networks/{network_id}/nodes/{node_id_base64}/revoke",
            post(revoke_node),
        )
        .route("/v1/enroll", post(enroll))
        .route("/v1/control", get(control))
        .fallback(not_found)
        .layer(DefaultBodyLimit::max(1024 * 1024))
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

async fn replace_acl_policy(
    State(state): State<AppState>,
    Path(network_id): Path<uuid::Uuid>,
    headers: HeaderMap,
    Json(request): Json<ReplaceAclPolicyRequest>,
) -> Result<Json<crate::model::ReplaceAclPolicyResponse>, ApiError> {
    authenticate_admin(&state, &headers)?;
    let response = crate::service::replace_acl_policy(&state, network_id, request).await?;
    Ok(Json(response))
}

async fn explain_acl(
    State(state): State<AppState>,
    Path(network_id): Path<uuid::Uuid>,
    headers: HeaderMap,
    Json(request): Json<ExplainAclRequest>,
) -> Result<Json<crate::model::ExplainAclResponse>, ApiError> {
    authenticate_admin(&state, &headers)?;
    let response = crate::service::explain_acl(&state, network_id, request).await?;
    Ok(Json(response))
}

async fn revoke_node(
    State(state): State<AppState>,
    Path((network_id, node_id_base64)): Path<(uuid::Uuid, String)>,
    headers: HeaderMap,
    Json(request): Json<RevokeNodeRequest>,
) -> Result<Json<crate::model::RevokeNodeResponse>, ApiError> {
    authenticate_admin(&state, &headers)?;
    let response =
        crate::service::revoke_node(&state, network_id, &node_id_base64, request).await?;
    Ok(Json(response))
}

async fn list_subnet_route_suggestions(
    State(state): State<AppState>,
    Path(network_id): Path<uuid::Uuid>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::model::SubnetRouteSuggestionResponse>>, ApiError> {
    authenticate_admin(&state, &headers)?;
    Ok(Json(
        crate::service::list_subnet_route_suggestions(&state, network_id).await?,
    ))
}

async fn replace_subnet_routes(
    State(state): State<AppState>,
    Path(network_id): Path<uuid::Uuid>,
    headers: HeaderMap,
    Json(request): Json<ReplaceSubnetRoutesRequest>,
) -> Result<Json<crate::model::ReplaceSubnetRoutesResponse>, ApiError> {
    authenticate_admin(&state, &headers)?;
    Ok(Json(
        crate::service::replace_subnet_routes(&state, network_id, request).await?,
    ))
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
        .max_message_size(64 * 1024)
        .max_frame_size(64 * 1024)
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
