use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: &'static str,
}

#[derive(Serialize)]
struct ErrorEnvelope {
    error: ErrorBody,
}

#[derive(Serialize)]
struct ErrorBody {
    code: &'static str,
    message: &'static str,
}

impl ApiError {
    #[must_use]
    pub const fn unauthorized() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "authentication required",
        )
    }

    #[must_use]
    pub const fn forbidden() -> Self {
        Self::new(StatusCode::FORBIDDEN, "forbidden", "permission denied")
    }

    #[must_use]
    pub const fn rate_limited() -> Self {
        Self::new(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "request rate limit exceeded",
        )
    }

    #[must_use]
    pub const fn validation() -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "request validation failed",
        )
    }

    #[must_use]
    pub const fn not_found() -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", "resource not found")
    }

    #[must_use]
    pub const fn conflict() -> Self {
        Self::new(
            StatusCode::CONFLICT,
            "resource_conflict",
            "resource conflicts with current state",
        )
    }

    #[must_use]
    pub const fn invalid_enrollment() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            "invalid_enrollment",
            "enrollment was rejected",
        )
    }

    #[must_use]
    pub const fn address_pool_exhausted() -> Self {
        Self::new(
            StatusCode::CONFLICT,
            "address_pool_exhausted",
            "network address pool has no available addresses",
        )
    }

    #[must_use]
    pub const fn unavailable() -> Self {
        Self::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "service_unavailable",
            "required service is unavailable",
        )
    }

    #[must_use]
    pub const fn internal() -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "request could not be completed",
        )
    }

    const fn new(status: StatusCode, code: &'static str, message: &'static str) -> Self {
        Self {
            status,
            code,
            message,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorEnvelope {
                error: ErrorBody {
                    code: self.code,
                    message: self.message,
                },
            }),
        )
            .into_response()
    }
}
