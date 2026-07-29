use std::{path::PathBuf, process::ExitCode, time::Duration};

use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    net::UnixStream,
    time::timeout,
};
use xs_core::{LocalAgentRequest, LocalAgentResponse, LocalAgentStatus};

const DEFAULT_SOCKET_PATH: &str = "/run/xs-nexus/agent.sock";
const MAX_RESPONSE_BYTES: usize = 512 * 1024;
const IO_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Clone, Copy)]
enum Command {
    Status,
    Peers,
    Diagnostics,
    Version,
}

struct Options {
    command: Command,
    socket_path: PathBuf,
    json: bool,
}

#[tokio::main]
async fn main() -> ExitCode {
    if entrypoint().await.is_ok() {
        ExitCode::SUCCESS
    } else {
        eprintln!("xs error=cli_ipc_failed");
        ExitCode::FAILURE
    }
}

async fn entrypoint() -> Result<(), ()> {
    let options = parse_options()?;
    if matches!(options.command, Command::Version) {
        println!("xs {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let request = match options.command {
        Command::Status => LocalAgentRequest::Status {},
        Command::Peers => LocalAgentRequest::Peers {},
        Command::Diagnostics => LocalAgentRequest::Diagnostics {},
        Command::Version => return Err(()),
    };
    let response = send_request(&options.socket_path, request).await?;
    validate_response(options.command, &response)?;
    if options.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&response).map_err(|_| ())?
        );
    } else {
        print_text(response)?;
    }
    Ok(())
}

fn parse_options() -> Result<Options, ()> {
    let mut arguments = std::env::args_os().skip(1);
    let command = match arguments.next().and_then(|value| value.into_string().ok()) {
        Some(value) if value == "status" => Command::Status,
        Some(value) if value == "peers" => Command::Peers,
        Some(value) if value == "diagnostics" => Command::Diagnostics,
        Some(value) if value == "--version" => Command::Version,
        _ => return Err(()),
    };
    let mut socket_path = PathBuf::from(DEFAULT_SOCKET_PATH);
    let mut socket_overridden = false;
    let mut json = false;
    while let Some(argument) = arguments.next() {
        if argument == "--socket" && !socket_overridden && !matches!(command, Command::Version) {
            socket_path = PathBuf::from(arguments.next().ok_or(())?);
            socket_overridden = true;
        } else if argument == "--json" && !json && !matches!(command, Command::Version) {
            json = true;
        } else {
            return Err(());
        }
    }
    Ok(Options {
        command,
        socket_path,
        json,
    })
}

async fn send_request(
    path: &PathBuf,
    request: LocalAgentRequest,
) -> Result<LocalAgentResponse, ()> {
    let mut stream = timeout(IO_TIMEOUT, UnixStream::connect(path))
        .await
        .map_err(|_| ())?
        .map_err(|_| ())?;
    let encoded = serde_json::to_vec(&request).map_err(|_| ())?;
    timeout(IO_TIMEOUT, stream.write_all(&encoded))
        .await
        .map_err(|_| ())?
        .map_err(|_| ())?;
    stream.shutdown().await.map_err(|_| ())?;
    let mut response = Vec::new();
    timeout(
        IO_TIMEOUT,
        stream
            .take(u64::try_from(MAX_RESPONSE_BYTES + 1).map_err(|_| ())?)
            .read_to_end(&mut response),
    )
    .await
    .map_err(|_| ())?
    .map_err(|_| ())?;
    if response.is_empty() || response.len() > MAX_RESPONSE_BYTES {
        return Err(());
    }
    serde_json::from_slice(&response).map_err(|_| ())
}

fn validate_response(command: Command, response: &LocalAgentResponse) -> Result<(), ()> {
    let valid = match command {
        Command::Status => {
            matches!(
                response,
                LocalAgentResponse::Status {
                    schema_version: 1,
                    ..
                }
            )
        }
        Command::Peers => {
            matches!(
                response,
                LocalAgentResponse::Peers {
                    schema_version: 1,
                    ..
                }
            )
        }
        Command::Diagnostics => matches!(
            response,
            LocalAgentResponse::Diagnostics {
                schema_version: 1,
                ..
            }
        ),
        Command::Version => false,
    };
    if valid { Ok(()) } else { Err(()) }
}

fn print_text(response: LocalAgentResponse) -> Result<(), ()> {
    match response {
        LocalAgentResponse::Status { status, .. } => {
            print_status(&status);
            Ok(())
        }
        LocalAgentResponse::Peers {
            peers,
            total,
            truncated,
            ..
        } => {
            println!(
                "configured_peers={} returned={} truncated={truncated}",
                total,
                peers.len()
            );
            for peer in peers {
                println!(
                    "{} {} roles=0x{:08x} expires={} tags={}",
                    peer.node_id_base64,
                    peer.virtual_ip,
                    peer.role_bitmap,
                    peer.credential_not_after.to_rfc3339(),
                    peer.tags.join(",")
                );
            }
            Ok(())
        }
        LocalAgentResponse::Diagnostics { diagnostics, .. } => {
            print_status(&diagnostics.status);
            println!(
                "configuration_generated_at={}",
                diagnostics.configuration_generated_at.to_rfc3339()
            );
            println!("address_pool={}", diagnostics.address_pool);
            println!("configuration_sha256={}", diagnostics.configuration_sha256);
            println!("tun_packets_received={}", diagnostics.tun_packets_received);
            println!("tun_packets_dropped={}", diagnostics.tun_packets_dropped);
            println!(
                "last_error_code={}",
                diagnostics.last_error_code.as_deref().unwrap_or("none")
            );
            Ok(())
        }
        LocalAgentResponse::Error { .. } => Err(()),
    }
}

fn print_status(status: &LocalAgentStatus) {
    println!("node_id={}", status.node_id_base64);
    println!("virtual_ip={}", status.virtual_ip);
    println!(
        "controller={}",
        if status.controller_connected {
            "connected"
        } else {
            "disconnected"
        }
    );
    println!(
        "network={}",
        if status.network_active {
            "active"
        } else {
            "inactive"
        }
    );
    println!(
        "interface={} index={}",
        status.interface_name, status.interface_index
    );
    println!("configuration_version={}", status.configuration_version);
    println!("uptime_seconds={}", status.uptime_seconds);
}
