use std::{path::PathBuf, process::ExitCode};

use tokio::sync::watch;
use xs_agent::{
    config::AgentConfig,
    enrollment::enroll,
    error::{AgentError, Result},
    lifecycle::cleanup_network,
    runtime::run_agent,
    updates::apply_staged_update,
};

const WINDOWS_SERVICE_NAME: &str = "XsNexusAgent";

#[derive(Debug, Eq, PartialEq)]
enum Command {
    Cleanup {
        config: PathBuf,
    },
    Enroll {
        config: PathBuf,
        token_file: PathBuf,
    },
    Run {
        config: PathBuf,
    },
    ApplyStagedUpdate {
        config: PathBuf,
        installer: PathBuf,
        install_root: PathBuf,
        service_manager: Option<PathBuf>,
    },
    Service {
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
        Command::ApplyStagedUpdate {
            config,
            installer,
            install_root,
            service_manager,
        } => {
            let config = AgentConfig::load(&config)?;
            apply_staged_update(
                &config,
                &installer,
                &install_root,
                service_manager.as_deref(),
            )
            .map_err(|error| {
                eprintln!("xs-agent update_error={}", error.code());
                AgentError::Update
            })
        }
        Command::Service { config } => run_windows_service(config),
        Command::Cleanup { config } => {
            let config = AgentConfig::load(&config)?;
            cleanup_network(&config).await?;
            println!("xs-agent cleanup complete");
            Ok(())
        }
        Command::Version => {
            println!(
                "{}",
                xs_core::BuildIdentity::current(xs_core::Component::Agent)
            );
            Ok(())
        }
    }
}

fn parse_command() -> Result<Command> {
    parse_command_from_with_platform(std::env::args_os().skip(1), cfg!(windows))
}

#[cfg(test)]
fn parse_command_from(arguments: impl IntoIterator<Item = std::ffi::OsString>) -> Result<Command> {
    parse_command_from_with_platform(arguments, cfg!(windows))
}

fn parse_command_from_with_platform(
    arguments: impl IntoIterator<Item = std::ffi::OsString>,
    windows: bool,
) -> Result<Command> {
    let mut arguments = arguments.into_iter();
    let command = arguments.next().ok_or(AgentError::Configuration)?;
    if command == "--version" {
        if arguments.next().is_some() {
            return Err(AgentError::Configuration);
        }
        return Ok(Command::Version);
    }

    let mut config = None;
    let mut token_file = None;
    let mut installer = None;
    let mut install_root = None;
    let mut service_manager = None;
    while let Some(argument) = arguments.next() {
        if argument == "--config" && config.is_none() {
            config = Some(PathBuf::from(
                arguments.next().ok_or(AgentError::Configuration)?,
            ));
        } else if argument == "--token-file" && token_file.is_none() {
            token_file = Some(PathBuf::from(
                arguments.next().ok_or(AgentError::Configuration)?,
            ));
        } else if argument == "--installer" && installer.is_none() {
            installer = Some(PathBuf::from(
                arguments.next().ok_or(AgentError::Configuration)?,
            ));
        } else if argument == "--root" && install_root.is_none() {
            install_root = Some(PathBuf::from(
                arguments.next().ok_or(AgentError::Configuration)?,
            ));
        } else if argument == "--service-manager" && service_manager.is_none() {
            service_manager = Some(PathBuf::from(
                arguments.next().ok_or(AgentError::Configuration)?,
            ));
        } else {
            return Err(AgentError::Configuration);
        }
    }

    let update_options_absent =
        installer.is_none() && install_root.is_none() && service_manager.is_none();
    if command == "run" && token_file.is_none() && update_options_absent {
        return Ok(Command::Run {
            config: config.ok_or(AgentError::Configuration)?,
        });
    }
    if windows && command == "service" && token_file.is_none() && update_options_absent {
        return Ok(Command::Service {
            config: config.ok_or(AgentError::Configuration)?,
        });
    }
    if command == "cleanup" && token_file.is_none() && update_options_absent {
        return Ok(Command::Cleanup {
            config: config.ok_or(AgentError::Configuration)?,
        });
    }
    if command == "enroll" && update_options_absent {
        return Ok(Command::Enroll {
            config: config.ok_or(AgentError::Configuration)?,
            token_file: token_file.ok_or(AgentError::Configuration)?,
        });
    }
    if !windows && command == "apply-staged-update" && token_file.is_none() {
        return Ok(Command::ApplyStagedUpdate {
            config: config.ok_or(AgentError::Configuration)?,
            installer: installer.ok_or(AgentError::Configuration)?,
            install_root: install_root.unwrap_or_else(|| PathBuf::from("/")),
            service_manager,
        });
    }
    Err(AgentError::Configuration)
}

fn run_windows_service(config_path: PathBuf) -> Result<()> {
    let runtime = tokio::runtime::Handle::current();
    xs_windows_service::run_service(WINDOWS_SERVICE_NAME, move |shutdown| {
        runtime.block_on(async move {
            let config = match AgentConfig::load(&config_path) {
                Ok(config) => config,
                Err(error) => {
                    eprintln!("xs-agent service error={}", error.code());
                    return false;
                }
            };
            let (shutdown_sender, shutdown_receiver) = watch::channel(false);
            let shutdown_task = tokio::spawn(async move {
                shutdown.cancelled().await;
                let _ = shutdown_sender.send(true);
            });
            let result = run_agent(config, shutdown_receiver).await;
            shutdown_task.abort();
            let _ = shutdown_task.await;
            if let Err(error) = result {
                eprintln!("xs-agent service error={}", error.code());
                false
            } else {
                true
            }
        })
    })
    .map_err(|_| AgentError::Runtime)
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

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::{Command, parse_command_from, parse_command_from_with_platform};

    fn arguments(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn cleanup_requires_exactly_one_config_path() {
        assert_eq!(
            parse_command_from(arguments(&[
                "cleanup",
                "--config",
                "/etc/xs-nexus/agent.json",
            ]))
            .expect("cleanup command"),
            Command::Cleanup {
                config: "/etc/xs-nexus/agent.json".into(),
            }
        );
        assert!(parse_command_from(arguments(&["cleanup"])).is_err());
        assert!(
            parse_command_from(arguments(&[
                "cleanup",
                "--config",
                "/etc/xs-nexus/agent.json",
                "--token-file",
                "/tmp/token",
            ]))
            .is_err()
        );
    }

    #[test]
    fn service_command_is_windows_only_and_exact() {
        let values = arguments(&["service", "--config", r"C:\ProgramData\XS Nexus\agent.json"]);
        assert_eq!(
            parse_command_from_with_platform(values.clone(), true).expect("Windows service"),
            Command::Service {
                config: r"C:\ProgramData\XS Nexus\agent.json".into(),
            }
        );
        assert!(parse_command_from_with_platform(values, false).is_err());
        assert!(
            parse_command_from_with_platform(
                arguments(&[
                    "service",
                    "--config",
                    r"C:\ProgramData\XS Nexus\agent.json",
                    "--token-file",
                    r"C:\token",
                ]),
                true,
            )
            .is_err()
        );
    }

    #[test]
    fn staged_update_command_is_linux_only_and_exact() {
        let values = arguments(&[
            "apply-staged-update",
            "--config",
            "/etc/xs-nexus/agent.json",
            "--installer",
            "/usr/local/lib/xs-nexus/current/share/xs-nexus/xs-nexus-installer.sh",
        ]);
        assert_eq!(
            parse_command_from_with_platform(values.clone(), false).expect("Linux update helper"),
            Command::ApplyStagedUpdate {
                config: "/etc/xs-nexus/agent.json".into(),
                installer: "/usr/local/lib/xs-nexus/current/share/xs-nexus/xs-nexus-installer.sh"
                    .into(),
                install_root: "/".into(),
                service_manager: None,
            }
        );
        assert!(parse_command_from_with_platform(values, true).is_err());
        assert!(
            parse_command_from(arguments(&[
                "run",
                "--config",
                "/etc/xs-nexus/agent.json",
                "--installer",
                "/tmp/installer",
            ]))
            .is_err()
        );
    }
}
