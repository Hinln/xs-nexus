use std::{net::Ipv4Addr, path::PathBuf, process::ExitCode, time::Duration};

#[cfg(unix)]
use tokio::net::UnixStream;
#[cfg(windows)]
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};
use tokio::{
    io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _},
    time::timeout,
};
use xs_core::{
    EndpointCandidateKind, LocalAgentDiagnostics, LocalAgentRequest, LocalAgentResponse,
    LocalAgentStatus, LocalNetcheckResult, LocalPeerPath, LocalPeerStatus, LocalPingResult,
    LocalRouteStatus, PathSelectionReason, SubnetRouteMode,
};

#[cfg(unix)]
const DEFAULT_SOCKET_PATH: &str = "/run/xs-nexus/agent.sock";
#[cfg(windows)]
const DEFAULT_SOCKET_PATH: &str = r"\\.\pipe\xs-nexus-agent";
const MAX_REQUEST_BYTES: usize = 4 * 1024;
const MAX_RESPONSE_BYTES: usize = 512 * 1024;
const FRAME_PREFIX_BYTES: usize = 4;
const IO_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Clone, Copy)]
enum Command {
    Status,
    Peers,
    Ping(Ipv4Addr),
    Path(Ipv4Addr),
    Routes,
    Netcheck,
    Diagnostics,
    Reconnect,
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
        Command::Ping(virtual_ip) => LocalAgentRequest::Ping { virtual_ip },
        Command::Path(virtual_ip) => LocalAgentRequest::Path { virtual_ip },
        Command::Routes => LocalAgentRequest::Routes {},
        Command::Netcheck => LocalAgentRequest::Netcheck {},
        Command::Diagnostics => LocalAgentRequest::Diagnostics {},
        Command::Reconnect => LocalAgentRequest::Reconnect {},
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
        print_text(&response);
    }
    if response_succeeded(&response) {
        Ok(())
    } else {
        Err(())
    }
}

fn parse_options() -> Result<Options, ()> {
    let mut arguments = std::env::args_os().skip(1);
    let command = match arguments.next().and_then(|value| value.into_string().ok()) {
        Some(value) if value == "status" => Command::Status,
        Some(value) if value == "peers" => Command::Peers,
        Some(value) if value == "ping" => Command::Ping(parse_virtual_ip(arguments.next())?),
        Some(value) if value == "path" => Command::Path(parse_virtual_ip(arguments.next())?),
        Some(value) if value == "routes" => Command::Routes,
        Some(value) if value == "netcheck" => Command::Netcheck,
        Some(value) if value == "diagnostics" => Command::Diagnostics,
        Some(value) if value == "reconnect" => Command::Reconnect,
        Some(value) if value == "version" || value == "--version" => Command::Version,
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

fn parse_virtual_ip(value: Option<std::ffi::OsString>) -> Result<Ipv4Addr, ()> {
    let value = value.and_then(|value| value.into_string().ok()).ok_or(())?;
    let parsed = value.parse::<Ipv4Addr>().map_err(|_| ())?;
    if parsed.to_string() == value {
        Ok(parsed)
    } else {
        Err(())
    }
}

async fn send_request(
    path: &PathBuf,
    request: LocalAgentRequest,
) -> Result<LocalAgentResponse, ()> {
    let stream = timeout(IO_TIMEOUT, connect_local(path))
        .await
        .map_err(|_| ())?
        .map_err(|_| ())?;
    timeout(IO_TIMEOUT, exchange(stream, request))
        .await
        .map_err(|_| ())?
}

async fn exchange<S>(mut stream: S, request: LocalAgentRequest) -> Result<LocalAgentResponse, ()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let encoded = serde_json::to_vec(&request).map_err(|_| ())?;
    if encoded.is_empty() || encoded.len() > MAX_REQUEST_BYTES {
        return Err(());
    }
    let request_length = u32::try_from(encoded.len()).map_err(|_| ())?;
    stream
        .write_all(&request_length.to_be_bytes())
        .await
        .map_err(|_| ())?;
    stream.write_all(&encoded).await.map_err(|_| ())?;
    stream.flush().await.map_err(|_| ())?;

    let mut prefix = [0_u8; FRAME_PREFIX_BYTES];
    stream.read_exact(&mut prefix).await.map_err(|_| ())?;
    let response_length = usize::try_from(u32::from_be_bytes(prefix)).map_err(|_| ())?;
    if response_length == 0 || response_length > MAX_RESPONSE_BYTES {
        return Err(());
    }
    let mut response = vec![0_u8; response_length];
    stream.read_exact(&mut response).await.map_err(|_| ())?;
    let result = serde_json::from_slice(&response).map_err(|_| ())?;
    stream.shutdown().await.map_err(|_| ())?;
    Ok(result)
}

#[cfg(unix)]
fn connect_local(
    path: &PathBuf,
) -> impl std::future::Future<Output = std::io::Result<UnixStream>> + '_ {
    UnixStream::connect(path)
}

#[cfg(windows)]
fn connect_local(path: &PathBuf) -> std::future::Ready<std::io::Result<NamedPipeClient>> {
    std::future::ready(ClientOptions::new().read(true).write(true).open(path))
}

#[cfg(not(any(unix, windows)))]
compile_error!("xs local IPC requires Unix or Windows");

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
        Command::Ping(_) => matches!(
            response,
            LocalAgentResponse::Ping {
                schema_version: 1,
                ..
            }
        ),
        Command::Path(_) => matches!(
            response,
            LocalAgentResponse::Path {
                schema_version: 1,
                ..
            }
        ),
        Command::Routes => matches!(
            response,
            LocalAgentResponse::Routes {
                schema_version: 1,
                ..
            }
        ),
        Command::Netcheck => matches!(
            response,
            LocalAgentResponse::Netcheck {
                schema_version: 1,
                ..
            }
        ),
        Command::Diagnostics => matches!(
            response,
            LocalAgentResponse::Diagnostics {
                schema_version: 1,
                ..
            }
        ),
        Command::Reconnect => matches!(
            response,
            LocalAgentResponse::Reconnect {
                schema_version: 1,
                ..
            }
        ),
        Command::Version => false,
    };
    if valid { Ok(()) } else { Err(()) }
}

fn print_text(response: &LocalAgentResponse) {
    match response {
        LocalAgentResponse::Status { status, .. } => print_status(status),
        LocalAgentResponse::Peers {
            peers,
            total,
            truncated,
            ..
        } => print_peers(peers, *total, *truncated),
        LocalAgentResponse::Ping { result, .. } => print_ping(result),
        LocalAgentResponse::Path { path, .. } => print_path(path),
        LocalAgentResponse::Routes {
            configuration_version,
            routes,
            ..
        } => print_routes(*configuration_version, routes),
        LocalAgentResponse::Netcheck { result, .. } => print_netcheck(result),
        LocalAgentResponse::Diagnostics { diagnostics, .. } => print_diagnostics(diagnostics),
        LocalAgentResponse::Reconnect {
            accepted,
            error_code,
            ..
        } => {
            println!("accepted={accepted}");
            println!("error_code={}", error_code.as_deref().unwrap_or("none"));
        }
        LocalAgentResponse::Error { code, .. } => println!("error_code={code}"),
    }
}

fn print_peers(peers: &[LocalPeerStatus], total: u64, truncated: bool) {
    println!(
        "configured_peers={} returned={} truncated={truncated}",
        total,
        peers.len()
    );
    for peer in peers {
        println!(
            "{} {} roles=0x{:08x} expires={} tags={} session={} active={} kind={} reason={}",
            peer.node_id_base64,
            peer.virtual_ip,
            peer.role_bitmap,
            peer.credential_not_after.to_rfc3339(),
            peer.tags.join(","),
            if peer.session_established {
                "established"
            } else {
                "idle"
            },
            peer.active_endpoint
                .map_or_else(|| "none".to_owned(), |endpoint| endpoint.to_string()),
            peer.active_candidate_kind
                .map_or("none", candidate_kind_label),
            peer.path_reason.map_or("none", path_reason_label),
        );
        for candidate in &peer.candidates {
            println!(
                "  candidate={} kind={} priority={} expires={}",
                candidate.endpoint,
                candidate_kind_label(candidate.kind),
                candidate.priority,
                candidate.expires_at.to_rfc3339()
            );
        }
    }
}

fn print_ping(result: &LocalPingResult) {
    println!("virtual_ip={}", result.virtual_ip);
    println!("reachable={}", result.reachable);
    println!(
        "latency_microseconds={}",
        result
            .latency_microseconds
            .map_or_else(|| "unavailable".to_owned(), |value| value.to_string())
    );
    println!(
        "kind={}",
        result
            .active_candidate_kind
            .map_or("none", candidate_kind_label)
    );
    println!(
        "reason={}",
        result.path_reason.map_or("none", path_reason_label)
    );
    println!(
        "error_code={}",
        result.error_code.as_deref().unwrap_or("none")
    );
}

fn print_path(path: &LocalPeerPath) {
    println!("node_id={}", path.node_id_base64);
    println!("virtual_ip={}", path.virtual_ip);
    println!("session_established={}", path.session_established);
    println!(
        "active_endpoint={}",
        path.active_endpoint
            .map_or_else(|| "none".to_owned(), |endpoint| endpoint.to_string())
    );
    println!(
        "kind={}",
        path.active_candidate_kind
            .map_or("none", candidate_kind_label)
    );
    println!(
        "reason={}",
        path.path_reason.map_or("none", path_reason_label)
    );
    println!(
        "last_latency_microseconds={}",
        path.last_latency_microseconds
            .map_or_else(|| "unavailable".to_owned(), |value| value.to_string())
    );
    println!(
        "tx_packets={} tx_bytes={} rx_packets={} rx_bytes={}",
        path.tx_packets_total, path.tx_bytes_total, path.rx_packets_total, path.rx_bytes_total
    );
}

fn print_routes(configuration_version: u64, routes: &[LocalRouteStatus]) {
    println!(
        "configuration_version={} configured_routes={}",
        configuration_version,
        routes.len()
    );
    for route in routes {
        println!(
            "{} {} via={} gateway_node={} mode={} interface={} priority={} local_gateway={} gateway_reachable={}",
            route.route_id,
            route.prefix,
            route.gateway_virtual_ip,
            route.gateway_node_id_base64,
            route_mode_label(route.mode),
            route.interface_name,
            route.priority,
            route.local_is_gateway,
            route.gateway_reachable,
        );
    }
}

fn print_netcheck(result: &LocalNetcheckResult) {
    println!("healthy={}", result.healthy);
    println!("controller_connected={}", result.controller_connected);
    println!("network_active={}", result.network_active);
    println!("local_candidates={}", result.local_candidate_count);
    println!("configured_peers={}", result.configured_peer_count);
    println!("established_peers={}", result.established_peer_count);
    println!("direct_peers={}", result.direct_peer_count);
    println!("relay_peers={}", result.relay_peer_count);
    println!(
        "last_error_code={}",
        result.last_error_code.as_deref().unwrap_or("none")
    );
}

fn print_diagnostics(diagnostics: &LocalAgentDiagnostics) {
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
    for candidate in &diagnostics.local_candidates {
        println!(
            "local_candidate={} kind={} priority={} expires={}",
            candidate.endpoint,
            candidate_kind_label(candidate.kind),
            candidate.priority,
            candidate.expires_at.to_rfc3339()
        );
    }
}

fn response_succeeded(response: &LocalAgentResponse) -> bool {
    match response {
        LocalAgentResponse::Ping { result, .. } => result.reachable,
        LocalAgentResponse::Netcheck { result, .. } => result.healthy,
        LocalAgentResponse::Reconnect { accepted, .. } => *accepted,
        LocalAgentResponse::Error { .. } => false,
        LocalAgentResponse::Status { .. }
        | LocalAgentResponse::Peers { .. }
        | LocalAgentResponse::Path { .. }
        | LocalAgentResponse::Routes { .. }
        | LocalAgentResponse::Diagnostics { .. } => true,
    }
}

const fn candidate_kind_label(kind: EndpointCandidateKind) -> &'static str {
    match kind {
        EndpointCandidateKind::Local => "local",
        EndpointCandidateKind::PublicIpv6 => "public_ipv6",
        EndpointCandidateKind::Mapped => "mapped",
        EndpointCandidateKind::Static => "static",
        EndpointCandidateKind::Relay => "relay",
    }
}

const fn path_reason_label(reason: PathSelectionReason) -> &'static str {
    match reason {
        PathSelectionReason::HighestPriority => "highest_priority",
        PathSelectionReason::HandshakeFallback => "handshake_fallback",
        PathSelectionReason::AuthenticatedHandshake => "authenticated_handshake",
        PathSelectionReason::AuthenticatedPeerTraffic => "authenticated_peer_traffic",
        PathSelectionReason::AuthenticatedPathProbe => "authenticated_path_probe",
        PathSelectionReason::RelayFallback => "relay_fallback",
        PathSelectionReason::RelayFailover => "relay_failover",
        PathSelectionReason::ConfigurationUpdate => "configuration_update",
    }
}

const fn route_mode_label(mode: SubnetRouteMode) -> &'static str {
    match mode {
        SubnetRouteMode::Routed => "routed",
        SubnetRouteMode::Nat => "nat",
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
