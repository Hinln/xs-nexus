#![forbid(unsafe_code)]

mod auth;
pub mod config;
mod console;
mod control;
pub mod db;
pub mod discovery;
mod downloads;
mod error;
pub mod model;
mod relay_telemetry;
mod service;
mod state;
mod telemetry;
mod updates;

use axum::Router;
use thiserror::Error;

pub use auth::BootstrapError;
pub use state::AppState;

#[derive(Debug, Error)]
pub enum StartupError {
    #[error(transparent)]
    Database(#[from] db::DatabaseError),
    #[error("unable to initialize console authentication")]
    Authentication(#[source] BootstrapError),
    #[error("controller listener failed")]
    Listener(#[source] std::io::Error),
    #[error("controller server failed")]
    Server(#[source] std::io::Error),
}

/// Builds the controller application against an already migrated database.
///
/// # Errors
///
/// Returns `StartupError` when the database cannot be initialized.
pub async fn build(config: &config::ControllerConfig) -> Result<(Router, AppState), StartupError> {
    let pool = db::connect(config).await?;
    auth::ensure_bootstrap_administrator(&pool, config)
        .await
        .map_err(StartupError::Authentication)?;
    let state = AppState::new(pool, config);
    let router = api::router(state.clone());
    Ok((router, state))
}

mod api;
