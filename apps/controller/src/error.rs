use axum::{
    Json,
    extract::{FromRequest, Request, rejection::JsonRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Serialize, de::DeserializeOwned};

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: &'static str,
}

pub(crate) struct ApiJson<T>(pub T);

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

impl<S, T> FromRequest<S> for ApiJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
    Json<T>: FromRequest<S, Rejection = JsonRejection>,
{
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        Json::<T>::from_request(request, state)
            .await
            .map(|Json(value)| Self(value))
            .map_err(|_| ApiError::validation())
    }
}

#[cfg(test)]
mod tests {
    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode, header},
        routing::post,
    };
    use http_body_util::BodyExt;
    use serde::Deserialize;
    use tower::ServiceExt;

    use super::ApiJson;

    #[derive(Deserialize)]
    struct Fixture {
        value: String,
    }

    async fn accept_json(ApiJson(fixture): ApiJson<Fixture>) -> StatusCode {
        if fixture.value == "accepted" {
            StatusCode::NO_CONTENT
        } else {
            StatusCode::BAD_REQUEST
        }
    }

    async fn request(
        body: &'static str,
        content_type: Option<&'static str>,
    ) -> axum::response::Response {
        let mut builder = Request::builder().method("POST").uri("/");
        if let Some(content_type) = content_type {
            builder = builder.header(header::CONTENT_TYPE, content_type);
        }
        Router::new()
            .route("/", post(accept_json))
            .oneshot(builder.body(Body::from(body)).expect("request"))
            .await
            .expect("response")
    }

    #[tokio::test]
    async fn accepts_valid_json() {
        let response = request(r#"{"value":"accepted"}"#, Some("application/json")).await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn json_rejections_return_one_sanitized_envelope() {
        for (body, content_type) in [
            (
                r#"{"value":"accepted","sensitive_field_marker":}"#,
                Some("application/json"),
            ),
            (r#"{"value":42}"#, Some("application/json")),
            (r#"{"value":"accepted"}"#, Some("text/plain")),
            (r#"{"value":"accepted"}"#, None),
        ] {
            let response = request(body, content_type).await;
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            assert_eq!(
                response.headers().get(header::CONTENT_TYPE),
                Some(&header::HeaderValue::from_static("application/json"))
            );
            let response_body = response
                .into_body()
                .collect()
                .await
                .expect("response body")
                .to_bytes();
            assert_eq!(
                response_body.as_ref(),
                br#"{"error":{"code":"invalid_request","message":"request validation failed"}}"#
            );
            assert!(!String::from_utf8_lossy(&response_body).contains("sensitive_field_marker"));
        }
    }
}
