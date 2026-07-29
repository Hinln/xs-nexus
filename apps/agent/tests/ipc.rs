#![cfg(unix)]

use std::{
    net::Ipv4Addr, os::unix::fs::PermissionsExt as _, path::Path, sync::Arc, time::Duration,
};

use chrono::{DateTime, Utc};
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    net::UnixStream,
    sync::watch,
};
use uuid::Uuid;
use xs_agent::{
    health::AgentHealth,
    ipc::{IpcContext, run_ipc_server},
    state::NodeState,
};
use xs_core::{
    ConfigurationNode, ConfigurationPayload, LocalAgentRequest, LocalAgentResponse,
    SignedConfiguration,
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
    let server = tokio::spawn(run_ipc_server(
        socket_path.clone(),
        Arc::clone(&state),
        Arc::clone(&health),
        IpcContext {
            interface_name: "xstest0".to_owned(),
            interface_index: 17,
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

    let status = request(&socket_path, LocalAgentRequest::Status {}).await;
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

    let peers = request(&socket_path, LocalAgentRequest::Peers {}).await;
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

    let diagnostics = request(&socket_path, LocalAgentRequest::Diagnostics {}).await;
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

    let malformed = raw_request(&socket_path, br#"{"command":"status","unexpected":true}"#).await;
    assert!(matches!(
        malformed,
        LocalAgentResponse::Error {
            schema_version: 1,
            ref code
        } if code == "agent_ipc_failed"
    ));

    shutdown_sender.send(true).expect("request shutdown");
    tokio::time::timeout(Duration::from_secs(2), server)
        .await
        .expect("IPC server stops")
        .expect("IPC task joins")
        .expect("IPC shutdown succeeds");
    assert!(!socket_path.exists());
}

async fn request(path: &Path, request: LocalAgentRequest) -> LocalAgentResponse {
    let encoded = serde_json::to_vec(&request).expect("serialize request");
    raw_request(path, &encoded).await
}

async fn raw_request(path: &Path, request: &[u8]) -> LocalAgentResponse {
    let mut stream = UnixStream::connect(path)
        .await
        .expect("connect to Agent IPC");
    stream.write_all(request).await.expect("write IPC request");
    stream.shutdown().await.expect("finish IPC request");
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
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
        credential_serial: 1,
        credential_not_after: expires_at,
        role_bitmap: 1,
        tags: vec!["linux".to_owned()],
    };
    let peer_node = ConfigurationNode {
        node_id_base64: "peer-node".to_owned(),
        identity_public_key_base64: "peer-public-key".to_owned(),
        virtual_ip: "100.127.20.3".to_owned(),
        direct_endpoints: Vec::new(),
        credential_serial: 2,
        credential_not_after: expires_at,
        role_bitmap: 2,
        tags: vec!["server".to_owned()],
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
            generated_at,
            address_pool: "100.127.20.0/24".to_owned(),
            nodes: vec![local_node, peer_node],
            relays: Vec::new(),
            policies: Vec::new(),
        },
        configuration_sha256: "7f76e8d8f19d3c1d8fcd65767889d66f592b37b5cd45d5ef7ac38dcaf8268efb"
            .to_owned(),
        credential_serial: 1,
    }
}
