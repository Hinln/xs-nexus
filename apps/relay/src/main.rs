use std::{process::ExitCode, sync::Arc};

use tokio::{signal, sync::watch};
use tracing_subscriber::EnvFilter;
use xs_relay::{RelayMetrics, RelayServer, config::RelayConfig};

#[tokio::main]
async fn main() -> ExitCode {
    if version_requested() {
        println!("xs-relay {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
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
            tracing::error!(event = "relay_start_failed", error = %error);
            ExitCode::FAILURE
        }
    }
}

fn version_requested() -> bool {
    let mut arguments = std::env::args_os().skip(1);
    arguments
        .next()
        .is_some_and(|argument| argument == "--version")
        && arguments.next().is_none()
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config = RelayConfig::from_env()?;
    let listen = config.listen;
    let health_listen = config.health_listen;
    let relay_id = config.relay_id;
    let relay_public_key = config.identity_key.verifying_key();
    let metrics = RelayMetrics::default();
    let server = RelayServer::new(config, metrics.clone());
    let udp_socket = Arc::new(tokio::net::UdpSocket::bind(listen).await?);
    let health_listener = tokio::net::TcpListener::bind(health_listen).await?;
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let mut udp = tokio::spawn(server.serve(Arc::clone(&udp_socket), shutdown_rx.clone()));
    let health_metrics = metrics.clone();
    let mut health = tokio::spawn(async move {
        axum::serve(health_listener, xs_relay::health_router(health_metrics))
            .with_graceful_shutdown(wait_for_shutdown(shutdown_rx))
            .await
    });
    tracing::info!(
        event = "relay_started",
        address = %listen,
        health_address = %health_listen,
        relay_id = %hexadecimal(&relay_id),
        relay_key_id = xs_protocol::controller_key_id(&relay_public_key),
    );
    tokio::select! {
        () = shutdown_signal() => {
            let _ = shutdown_tx.send(true);
            udp.await??;
            health.await??;
        }
        result = &mut udp => {
            let _ = shutdown_tx.send(true);
            health.await??;
            result??;
            return Err("Relay UDP service stopped unexpectedly".into());
        }
        result = &mut health => {
            let _ = shutdown_tx.send(true);
            udp.await??;
            result??;
            return Err("Relay health service stopped unexpectedly".into());
        }
    }
    let snapshot = metrics.snapshot();
    tracing::info!(
        event = "relay_stopped",
        active_leases = snapshot.active_leases,
        packets_forwarded = snapshot.packets_forwarded,
        bytes_forwarded = snapshot.bytes_forwarded,
    );
    Ok(())
}

async fn wait_for_shutdown(mut shutdown: watch::Receiver<bool>) {
    while !*shutdown.borrow() && shutdown.changed().await.is_ok() {}
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

fn hexadecimal(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(DIGITS[usize::from(byte >> 4)]));
        encoded.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    encoded
}
