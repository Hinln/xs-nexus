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
        .route("/metrics", get(snapshot))
        .with_state(metrics)
}

async fn health() -> Json<Health> {
    Json(Health { status: "ok" })
}

async fn snapshot(State(metrics): State<RelayMetrics>) -> Json<RelayMetricsSnapshot> {
    Json(metrics.snapshot())
}
