use std::{path::PathBuf, process::ExitCode};

use tokio::sync::watch;
use xs_agent::{
    config::AgentConfig,
    enrollment::enroll,
    error::{AgentError, Result},
    runtime::run_agent,
};

enum Command {
    Enroll {
        config: PathBuf,
        token_file: PathBuf,
    },
    Run {
        config: PathBuf,
    },
    Version,
}

#[tokio::main]
async fn main() -> ExitCode {
    match entrypoint().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("xs-agent error={}", error.code());
            ExitCode::FAILURE
        }
    }
}

async fn entrypoint() -> Result<()> {
    match parse_command()? {
        Command::Enroll { config, token_file } => {
            let config = AgentConfig::load(&config)?;
            let state = enroll(&config, &token_file).await?;
            println!(
                "xs-agent enrolled node_id={} virtual_ip={}",
                state.node_id_base64, state.virtual_ip
            );
            Ok(())
        }
        Command::Run { config } => {
            let config = AgentConfig::load(&config)?;
            let (shutdown_sender, shutdown_receiver) = watch::channel(false);
            let signal_task = tokio::spawn(async move {
                let _ = wait_for_shutdown_signal().await;
                let _ = shutdown_sender.send(true);
            });
            let result = run_agent(config, shutdown_receiver).await;
            signal_task.abort();
            let _ = signal_task.await;
            result
        }
        Command::Version => {
            println!("xs-agent {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
    }
}

fn parse_command() -> Result<Command> {
    let mut arguments = std::env::args_os().skip(1);
    let command = arguments.next().ok_or(AgentError::Configuration)?;
    if command == "--version" {
        if arguments.next().is_some() {
            return Err(AgentError::Configuration);
        }
        return Ok(Command::Version);
    }

    let mut config = None;
    let mut token_file = None;
    while let Some(argument) = arguments.next() {
        if argument == "--config" && config.is_none() {
            config = Some(PathBuf::from(
                arguments.next().ok_or(AgentError::Configuration)?,
            ));
        } else if argument == "--token-file" && token_file.is_none() {
            token_file = Some(PathBuf::from(
                arguments.next().ok_or(AgentError::Configuration)?,
            ));
        } else {
            return Err(AgentError::Configuration);
        }
    }

    if command == "run" && token_file.is_none() {
        return Ok(Command::Run {
            config: config.ok_or(AgentError::Configuration)?,
        });
    }
    if command == "enroll" {
        return Ok(Command::Enroll {
            config: config.ok_or(AgentError::Configuration)?,
            token_file: token_file.ok_or(AgentError::Configuration)?,
        });
    }
    Err(AgentError::Configuration)
}

#[cfg(unix)]
async fn wait_for_shutdown_signal() -> Result<()> {
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .map_err(|_| AgentError::Runtime)?;
    tokio::select! {
        result = tokio::signal::ctrl_c() => result.map_err(|_| AgentError::Runtime),
        signal = terminate.recv() => signal.map_or(Err(AgentError::Runtime), |()| Ok(())),
    }
}

#[cfg(not(unix))]
async fn wait_for_shutdown_signal() -> Result<()> {
    tokio::signal::ctrl_c()
        .await
        .map_err(|_| AgentError::Runtime)
}
