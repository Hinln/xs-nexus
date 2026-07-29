use std::process::ExitCode;

use tokio::signal;
use tracing_subscriber::EnvFilter;
use xs_controller::config::ControllerConfig;

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_current_span(false)
        .with_span_list(false)
        .init();

    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(event = "controller_start_failed", error = %error);
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config = ControllerConfig::from_env()?;
    let listen = config.listen;
    let (application, state) = xs_controller::build(&config).await?;
    drop(config);
    let listener = tokio::net::TcpListener::bind(listen).await?;
    tracing::info!(
        event = "controller_started",
        address = %listen,
        credential_key_id = xs_protocol::controller_key_id(
            &state.credential_signing_key.verifying_key()
        ),
        configuration_key_id = xs_protocol::controller_key_id(
            &state.config_signing_key.verifying_key()
        ),
    );

    axum::serve(listener, application)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    state.pool.close().await;
    Ok(())
}

async fn shutdown_signal() {
    let interrupt = async {
        if signal::ctrl_c().await.is_err() {
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut stream) = signal::unix::signal(signal::unix::SignalKind::terminate()) {
            stream.recv().await;
        } else {
            std::future::pending::<()>().await;
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = interrupt => {}
        () = terminate => {}
    }
}
