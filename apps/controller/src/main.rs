use std::{ffi::OsString, process::ExitCode};

use tokio::{signal, sync::watch};
use tracing_subscriber::EnvFilter;
use xs_controller::config::{ControllerConfig, MigrationConfig};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Command {
    Serve,
    Migrate,
    Version,
}

#[tokio::main]
async fn main() -> ExitCode {
    let command = match parse_command(std::env::args_os().skip(1)) {
        Ok(command) => command,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    if command == Command::Version {
        println!("xs-controller {}", env!("CARGO_PKG_VERSION"));
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

    match run(command).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(event = "controller_start_failed", error = %error);
            ExitCode::FAILURE
        }
    }
}

async fn run(command: Command) -> Result<(), Box<dyn std::error::Error>> {
    if command == Command::Migrate {
        let config = MigrationConfig::from_env()?;
        let database_schema = config.database_schema.clone();
        xs_controller::db::migrate(&config).await?;
        tracing::info!(
            event = "controller_migration_completed",
            schema = database_schema
        );
        return Ok(());
    }
    let config = ControllerConfig::from_env()?;
    let listen = config.listen;
    let discovery_listen = config.discovery_listen;
    let (application, state) = xs_controller::build(&config).await?;
    drop(config);
    let listener = tokio::net::TcpListener::bind(listen).await?;
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let discovery = if let Some(address) = discovery_listen {
        let socket = tokio::net::UdpSocket::bind(address).await?;
        let state = state.clone();
        Some(tokio::spawn(async move {
            xs_controller::discovery::serve(socket, state, shutdown_rx).await
        }))
    } else {
        None
    };
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
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            let _ = shutdown_tx.send(true);
        })
        .await?;
    if let Some(discovery) = discovery {
        discovery.await??;
    }
    state.pool.close().await;
    Ok(())
}

fn parse_command(arguments: impl Iterator<Item = OsString>) -> Result<Command, &'static str> {
    let arguments: Vec<OsString> = arguments.collect();
    match arguments.as_slice() {
        [] => Ok(Command::Serve),
        [argument] if argument == "serve" => Ok(Command::Serve),
        [argument] if argument == "migrate" => Ok(Command::Migrate),
        [argument] if argument == "--version" => Ok(Command::Version),
        _ => Err("usage: xs-controller [serve|migrate|--version]"),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_parser_is_exact() {
        assert_eq!(
            parse_command(Vec::<OsString>::new().into_iter()),
            Ok(Command::Serve)
        );
        assert_eq!(
            parse_command([OsString::from("serve")].into_iter()),
            Ok(Command::Serve)
        );
        assert_eq!(
            parse_command([OsString::from("migrate")].into_iter()),
            Ok(Command::Migrate)
        );
        assert_eq!(
            parse_command([OsString::from("--version")].into_iter()),
            Ok(Command::Version)
        );
        assert!(
            parse_command([OsString::from("migrate"), OsString::from("extra")].into_iter())
                .is_err()
        );
    }
}
