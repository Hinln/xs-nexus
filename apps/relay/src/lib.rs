#![forbid(unsafe_code)]

use axum::{Json, Router, extract::State, routing::get};
use serde::Serialize;

pub mod config;
mod metrics;
mod reporter;
mod server;

pub use metrics::{RelayMetrics, RelayMetricsSnapshot};
pub use reporter::{RelayTelemetryReporter, ReporterError};
pub use server::RelayServer;

#[derive(Clone, Copy, Serialize)]
struct Health {
    status: &'static str,
}

pub fn health_router(metrics: RelayMetrics) -> Router {
    Router::new()
        .route("/health/live", get(health))
        .route("/health/ready", get(health))
        .route("/version", get(version))
        .route("/metrics", get(snapshot))
        .with_state(metrics)
}

async fn health() -> Json<Health> {
    Json(Health { status: "ok" })
}

async fn version() -> Json<xs_core::BuildIdentity> {
    Json(xs_core::BuildIdentity::current(xs_core::Component::Relay))
}

async fn snapshot(State(metrics): State<RelayMetrics>) -> Json<RelayMetricsSnapshot> {
    Json(metrics.snapshot())
}

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use http_body_util::BodyExt;
    use serde_json::Value;
    use tower::ServiceExt;

    use super::{RelayMetrics, health_router};

    #[tokio::test]
    async fn version_endpoint_reports_exact_build_identity() {
        let response = health_router(RelayMetrics::default())
            .oneshot(
                Request::builder()
                    .uri("/version")
                    .body(Body::empty())
                    .expect("version request"),
            )
            .await
            .expect("version response");
        assert_eq!(response.status(), StatusCode::OK);
        let body = response
            .into_body()
            .collect()
            .await
            .expect("version response body")
            .to_bytes();
        let value: Value = serde_json::from_slice(&body).expect("version response JSON");
        assert_eq!(value["product"], "xs-nexus");
        assert_eq!(value["component"], "xs-relay");
        assert_eq!(value["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(value["commit"], xs_core::BUILD_GIT_COMMIT);
        assert_eq!(value["protocol_version"], "XSP/1");
        assert_eq!(value["build_date_epoch"], xs_core::BUILD_DATE_EPOCH);
    }
}
