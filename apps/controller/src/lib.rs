#![forbid(unsafe_code)]

pub mod config;
mod control;
pub mod db;
mod error;
pub mod model;
mod service;
mod state;

use axum::Router;
use thiserror::Error;

pub use state::AppState;

#[derive(Debug, Error)]
pub enum StartupError {
    #[error(transparent)]
    Database(#[from] db::DatabaseError),
    #[error("controller listener failed")]
    Listener(#[source] std::io::Error),
    #[error("controller server failed")]
    Server(#[source] std::io::Error),
}

/// Builds the migrated controller application and shared state.
///
/// # Errors
///
/// Returns `StartupError` when the database cannot be initialized.
pub async fn build(config: &config::ControllerConfig) -> Result<(Router, AppState), StartupError> {
    let pool = db::connect(config).await?;
    let state = AppState::new(pool, config);
    let router = api::router(state.clone());
    Ok((router, state))
}

mod api;
