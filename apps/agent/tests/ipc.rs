#![cfg(unix)]

use std::{
    net::Ipv4Addr, os::unix::fs::PermissionsExt as _, path::Path, sync::Arc, time::Duration,
};

use chrono::{DateTime, Utc};
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    net::UnixStream,
    sync::{mpsc, watch},
};
use uuid::Uuid;
use xs_agent::{
    data_plane::{DataPlaneStatus, PeerPathStatus},
    health::AgentHealth,
    ipc::{IpcContext, RuntimeCommand, run_ipc_server},
    state::NodeState,
};
use xs_core::{
    ConfigurationNode, ConfigurationPayload, ConfigurationSubnetRoute, EndpointCandidateKind,
    LocalAgentRequest, LocalAgentResponse, PathSelectionReason, SignedConfiguration,
    SubnetRouteMode,
};

#[tokio::test]
async fn local_ipc_is_private_bounded_and_redacted() {
    let temporary = tempfile::tempdir().expect("temporary IPC directory");
    let socket_path = temporary.path().join("runtime/agent.sock");
    let state = Arc::new(tokio::sync::RwLock::new(fixture_state()));
    let health = Arc::new(AgentHealth::new());
    health.set_controller_connected(true);
    health.record_tun_packet(false);
    health.record_tun_packet(true);
    let (shutdown_sender, shutdown_receiver) = watch::channel(false);
    let data_plane_status = Arc::new(tokio::sync::RwLock::new(fixture_data_plane_status()));
    let (runtime_command_sender, mut runtime_commands) = mpsc::channel(16);
    let command_status = Arc::clone(&data_plane_status);
    let command_task = tokio::spawn(async move {
        while let Some(command) = runtime_commands.recv().await {
            match command {
                RuntimeCommand::Probe {
                    virtual_ip,
                    response,
                } => {
                    let mut status = command_status.write().await;
                    let peer = status.peers.get_mut(&virtual_ip).expect("fixture peer");
                    peer.latency_samples_total = peer.latency_samples_total.saturating_add(1);
                    peer.last_latency_microseconds = Some(1_250);
                    let _ = response.send(Ok(()));
                }
                RuntimeCommand::Reconnect { response } => {
                    let _ = response.send(Ok(()));
                }
            }
        }
    });
    let server = tokio::spawn(run_ipc_server(
        socket_path.clone(),
        Arc::clone(&state),
        Arc::clone(&health),
        IpcContext {
            interface_name: "xstest0".to_owned(),
            interface_index: 17,
            data_plane_status,
            runtime_commands: runtime_command_sender,
        },
        shutdown_receiver,
    ));
    wait_for_socket(&socket_path).await;

    let mode = socket_path
        .metadata()
        .expect("socket metadata")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);

    assert_overview_commands(&socket_path).await;
    assert_network_commands(&socket_path).await;

    let malformed = raw_request(&socket_path, br#"{"command":"status","unexpected":true}"#).await;
    assert!(matches!(
        malformed,
        LocalAgentResponse::Error {
            schema_version: 1,
            ref code
        } if code == "agent_ipc_failed"
    ));
    for invalid_length in [0_u32, 4097_u32] {
        let invalid = raw_frame(&socket_path, &invalid_length.to_be_bytes()).await;
        assert!(matches!(
            invalid,
            LocalAgentResponse::Error {
                schema_version: 1,
                ref code
            } if code == "agent_ipc_failed"
        ));
    }

    shutdown_sender.send(true).expect("request shutdown");
    tokio::time::timeout(Duration::from_secs(2), server)
        .await
        .expect("IPC server stops")
        .expect("IPC task joins")
        .expect("IPC shutdown succeeds");
    assert!(!socket_path.exists());
    command_task.await.expect("runtime command task joins");
}

async fn assert_overview_commands(socket_path: &Path) {
    let status = request(socket_path, LocalAgentRequest::Status {}).await;
    assert!(matches!(
        status,
        LocalAgentResponse::Status {
            schema_version: 1,
            status: xs_core::LocalAgentStatus {
                controller_connected: true,
                network_active: true,
                interface_index: 17,
                configuration_version: 7,
                ..
            }
        }
    ));

    let peers = request(socket_path, LocalAgentRequest::Peers {}).await;
    match peers {
        LocalAgentResponse::Peers {
            schema_version,
            peers,
            total,
            truncated,
        } => {
            assert_eq!(schema_version, 1);
            assert_eq!(total, 1);
            assert!(!truncated);
            assert_eq!(peers.len(), 1);
            assert_eq!(peers[0].node_id_base64, "peer-node");
            assert_eq!(peers[0].virtual_ip, Ipv4Addr::new(100, 127, 20, 3));
        }
        response => panic!("unexpected peers response: {response:?}"),
    }

    let diagnostics = request(socket_path, LocalAgentRequest::Diagnostics {}).await;
    let encoded = serde_json::to_string(&diagnostics).expect("serialize diagnostics");
    assert!(!encoded.contains("secret-credential"));
    assert!(!encoded.contains("identity_public_key_base64"));
    assert!(!encoded.contains("credential_signing_public_key_base64"));
    assert!(matches!(
        diagnostics,
        LocalAgentResponse::Diagnostics {
            diagnostics: xs_core::LocalAgentDiagnostics {
                tun_packets_received: 2,
                tun_packets_dropped: 1,
                ..
            },
            ..
        }
    ));
}

async fn assert_network_commands(socket_path: &Path) {
    let path = request(
        socket_path,
        LocalAgentRequest::Path {
            virtual_ip: Ipv4Addr::new(100, 127, 20, 3),
        },
    )
    .await;
    assert!(matches!(
        path,
        LocalAgentResponse::Path {
            path: xs_core::LocalPeerPath {
                session_established: true,
                active_candidate_kind: Some(EndpointCandidateKind::Local),
                ..
            },
            ..
        }
    ));

    let routes = request(socket_path, LocalAgentRequest::Routes {}).await;
    assert!(matches!(
        routes,
        LocalAgentResponse::Routes {
            configuration_version: 7,
            ref routes,
            ..
        } if routes.len() == 1 && routes[0].gateway_reachable
    ));

    let netcheck = request(socket_path, LocalAgentRequest::Netcheck {}).await;
    assert!(matches!(
        netcheck,
        LocalAgentResponse::Netcheck {
            result: xs_core::LocalNetcheckResult {
                healthy: true,
                direct_peer_count: 1,
                relay_peer_count: 0,
                ..
            },
            ..
        }
    ));

    let ping = request(
        socket_path,
        LocalAgentRequest::Ping {
            virtual_ip: Ipv4Addr::new(100, 127, 20, 3),
        },
    )
    .await;
    assert!(matches!(
        ping,
        LocalAgentResponse::Ping {
            result: xs_core::LocalPingResult {
                reachable: true,
                latency_microseconds: Some(1_250),
                ..
            },
            ..
        }
    ));

    let reconnect = request(socket_path, LocalAgentRequest::Reconnect {}).await;
    assert!(matches!(
        reconnect,
        LocalAgentResponse::Reconnect {
            accepted: true,
            error_code: None,
            ..
        }
    ));
}

async fn request(path: &Path, request: LocalAgentRequest) -> LocalAgentResponse {
    let encoded = serde_json::to_vec(&request).expect("serialize request");
    raw_request(path, &encoded).await
}

async fn raw_request(path: &Path, request: &[u8]) -> LocalAgentResponse {
    let mut frame = Vec::with_capacity(4 + request.len());
    let length = u32::try_from(request.len()).expect("request length");
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(request);
    raw_frame(path, &frame).await
}

async fn raw_frame(path: &Path, frame: &[u8]) -> LocalAgentResponse {
    let mut stream = UnixStream::connect(path)
        .await
        .expect("connect to Agent IPC");
    stream
        .write_all(frame)
        .await
        .expect("write IPC request frame");
    stream.flush().await.expect("flush IPC request frame");
    let mut prefix = [0_u8; 4];
    stream
        .read_exact(&mut prefix)
        .await
        .expect("read IPC response length");
    let response_length = usize::try_from(u32::from_be_bytes(prefix)).expect("response length");
    let mut response = vec![0_u8; response_length];
    stream
        .read_exact(&mut response)
        .await
        .expect("read IPC response");
    serde_json::from_slice(&response).expect("valid IPC response")
}

async fn wait_for_socket(path: &Path) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while !path.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("IPC socket appears");
}

fn fixture_state() -> NodeState {
    let generated_at = DateTime::parse_from_rfc3339("2026-07-29T00:00:00Z")
        .expect("fixture timestamp")
        .with_timezone(&Utc);
    let expires_at = DateTime::parse_from_rfc3339("2026-08-29T00:00:00Z")
        .expect("fixture expiry")
        .with_timezone(&Utc);
    let local_node = ConfigurationNode {
        node_id_base64: "local-node".to_owned(),
        identity_public_key_base64: "local-public-key".to_owned(),
        virtual_ip: "100.127.20.2".to_owned(),
        direct_endpoints: Vec::new(),
        candidates: Vec::new(),
        credential_serial: 1,
        credential_not_after: expires_at,
        role_bitmap: 1,
        groups: Vec::new(),
        tags: vec!["linux".to_owned()],
        update_channel: None,
    };
    let peer_node = ConfigurationNode {
        node_id_base64: "peer-node".to_owned(),
        identity_public_key_base64: "peer-public-key".to_owned(),
        virtual_ip: "100.127.20.3".to_owned(),
        direct_endpoints: Vec::new(),
        candidates: Vec::new(),
        credential_serial: 2,
        credential_not_after: expires_at,
        role_bitmap: 2,
        groups: Vec::new(),
        tags: vec!["server".to_owned()],
        update_channel: None,
    };
    NodeState {
        schema_version: 1,
        controller_url: "https://controller.example/".to_owned(),
        network_id: Uuid::from_u128(1),
        node_id_base64: "local-node".to_owned(),
        virtual_ip: Ipv4Addr::new(100, 127, 20, 2),
        credential_base64: "secret-credential".to_owned(),
        credential_key_id: 4,
        credential_signing_public_key_base64: "credential-key".to_owned(),
        configuration_signing_public_key_base64: "configuration-key".to_owned(),
        configuration: SignedConfiguration {
            version: 7,
            payload_base64: "payload".to_owned(),
            signature_base64: "signature".to_owned(),
            signer_key_id: 5,
        },
        configuration_payload: ConfigurationPayload {
            schema_version: 1,
            network_id: Uuid::from_u128(1),
            version: 7,
            policy_version: 1,
            generated_at,
            address_pool: "100.127.20.0/24".to_owned(),
            discovery_endpoints: Vec::new(),
            nodes: vec![local_node, peer_node],
            relays: Vec::new(),
            policies: Vec::new(),
            subnet_routes: vec![ConfigurationSubnetRoute {
                route_id: "lan-primary".to_owned(),
                prefix: "192.168.20.0/24".to_owned(),
                gateway_node_id_base64: "peer-node".to_owned(),
                mode: SubnetRouteMode::Routed,
                interface_name: "eth0".to_owned(),
                priority: 100,
            }],
        },
        configuration_sha256: "7f76e8d8f19d3c1d8fcd65767889d66f592b37b5cd45d5ef7ac38dcaf8268efb"
            .to_owned(),
        credential_serial: 1,
        candidate_generation: 0,
        subnet_route_generation: 0,
    }
}

fn fixture_data_plane_status() -> DataPlaneStatus {
    let mut status = DataPlaneStatus::default();
    status.peers.insert(
        Ipv4Addr::new(100, 127, 20, 3),
        PeerPathStatus {
            candidates: Vec::new(),
            active_endpoint: Some("10.0.0.3:42000".parse().expect("fixture endpoint")),
            active_candidate_kind: Some(EndpointCandidateKind::Local),
            path_reason: Some(PathSelectionReason::AuthenticatedHandshake),
            session_established: true,
            tx_packets_total: 2,
            tx_bytes_total: 128,
            rx_packets_total: 3,
            rx_bytes_total: 192,
            handshake_attempts_total: 1,
            handshake_successes_total: 1,
            latency_samples_total: 0,
            latency_microseconds_total: 0,
            last_latency_microseconds: None,
        },
    );
    status
}
