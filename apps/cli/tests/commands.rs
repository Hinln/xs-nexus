#![cfg(unix)]

use std::{
    io::{Read as _, Write as _},
    os::unix::net::UnixListener,
    path::Path,
    process::Command,
    thread,
};

#[test]
fn commands_use_the_strict_local_protocol() {
    let temporary = tempfile::tempdir().expect("temporary CLI sockets");
    let cases = overview_cases()
        .into_iter()
        .chain(path_cases())
        .chain(route_cases())
        .chain(maintenance_cases());
    for (index, (command, expected_request, response, expected_output)) in cases.enumerate() {
        let socket_path = temporary.path().join(format!("agent-{index}.sock"));
        let server = serve_once(&socket_path, expected_request, response);
        let output = Command::new(env!("CARGO_BIN_EXE_xs"))
            .args(command)
            .arg("--socket")
            .arg(&socket_path)
            .output()
            .expect("run xs command");
        server.join().expect("fake Agent server joins");
        assert!(
            output.status.success(),
            "stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains(expected_output),
            "stdout: {}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
}

type CommandCase = (
    Vec<&'static str>,
    serde_json::Value,
    serde_json::Value,
    &'static str,
);

fn overview_cases() -> Vec<CommandCase> {
    vec![
        (
            vec!["status"],
            serde_json::json!({"command":"status"}),
            status_response(),
            "node_id=node-1",
        ),
        (
            vec!["peers"],
            serde_json::json!({"command":"peers"}),
            serde_json::json!({
                "type":"peers", "schema_version":1,
                "peers":[{
                    "node_id_base64":"peer-1", "virtual_ip":"100.64.0.3",
                    "credential_not_after":"2026-08-29T00:00:00Z", "role_bitmap":2,
                    "tags":["server"], "candidates":[],
                    "active_endpoint":"10.0.0.3:42000", "active_candidate_kind":"local",
                    "path_reason":"authenticated_handshake", "session_established":true
                }],
                "total":1, "truncated":false
            }),
            "configured_peers=1 returned=1 truncated=false",
        ),
    ]
}

fn path_cases() -> Vec<CommandCase> {
    vec![
        (
            vec!["ping", "100.64.0.3"],
            serde_json::json!({"command":"ping","virtual_ip":"100.64.0.3"}),
            serde_json::json!({
                "type":"ping", "schema_version":1,
                "result":{
                    "virtual_ip":"100.64.0.3", "reachable":true,
                    "latency_microseconds":1250, "active_candidate_kind":"local",
                    "path_reason":"authenticated_path_probe", "error_code":null
                }
            }),
            "reachable=true",
        ),
        (
            vec!["path", "100.64.0.3"],
            serde_json::json!({"command":"path","virtual_ip":"100.64.0.3"}),
            serde_json::json!({
                "type":"path", "schema_version":1,
                "path":{
                    "node_id_base64":"peer-1", "virtual_ip":"100.64.0.3",
                    "session_established":true, "active_endpoint":"10.0.0.3:42000",
                    "active_candidate_kind":"local", "path_reason":"authenticated_handshake",
                    "last_latency_microseconds":1250, "tx_packets_total":2,
                    "tx_bytes_total":128, "rx_packets_total":3, "rx_bytes_total":192
                }
            }),
            "session_established=true",
        ),
    ]
}

fn route_cases() -> Vec<CommandCase> {
    vec![
        (
            vec!["routes"],
            serde_json::json!({"command":"routes"}),
            serde_json::json!({
                "type":"routes", "schema_version":1, "configuration_version":4,
                "routes":[{
                    "route_id":"lan-primary", "prefix":"192.168.20.0/24",
                    "gateway_node_id_base64":"peer-1", "gateway_virtual_ip":"100.64.0.3",
                    "mode":"routed", "interface_name":"eth0", "priority":100,
                    "local_is_gateway":false, "gateway_reachable":true
                }]
            }),
            "configured_routes=1",
        ),
        (
            vec!["netcheck"],
            serde_json::json!({"command":"netcheck"}),
            serde_json::json!({
                "type":"netcheck", "schema_version":1,
                "result":{
                    "healthy":true, "controller_connected":true, "network_active":true,
                    "local_candidate_count":1, "configured_peer_count":1,
                    "established_peer_count":1, "direct_peer_count":1,
                    "relay_peer_count":0, "last_error_code":null
                }
            }),
            "healthy=true",
        ),
    ]
}

fn maintenance_cases() -> Vec<CommandCase> {
    vec![
        (
            vec!["diagnostics"],
            serde_json::json!({"command":"diagnostics"}),
            serde_json::json!({
                "type":"diagnostics", "schema_version":1,
                "diagnostics":{
                    "status":status_value(), "configuration_generated_at":"2026-07-29T00:00:00Z",
                    "address_pool":"100.64.0.0/24", "configuration_sha256":"abcd",
                    "tun_packets_received":2, "tun_packets_dropped":1,
                    "last_error_code":null, "local_candidates":[]
                }
            }),
            "tun_packets_dropped=1",
        ),
        (
            vec!["reconnect"],
            serde_json::json!({"command":"reconnect"}),
            serde_json::json!({
                "type":"reconnect", "schema_version":1,
                "accepted":true, "error_code":null
            }),
            "accepted=true",
        ),
    ]
}

#[test]
fn version_aliases_and_failure_status_are_stable() {
    for command in ["version", "--version"] {
        let output = Command::new(env!("CARGO_BIN_EXE_xs"))
            .arg(command)
            .output()
            .expect("run xs version");
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout).expect("version output"),
            format!("xs {}\n", env!("CARGO_PKG_VERSION"))
        );
    }

    let temporary = tempfile::tempdir().expect("temporary CLI socket");
    let socket_path = temporary.path().join("agent-unreachable.sock");
    let server = serve_once(
        &socket_path,
        serde_json::json!({"command":"ping","virtual_ip":"100.64.0.3"}),
        serde_json::json!({
            "type":"ping",
            "schema_version":1,
            "result":{
                "virtual_ip":"100.64.0.3",
                "reachable":false,
                "latency_microseconds":null,
                "active_candidate_kind":null,
                "path_reason":null,
                "error_code":"agent_path_probe_timeout"
            }
        }),
    );
    let output = Command::new(env!("CARGO_BIN_EXE_xs"))
        .args(["ping", "100.64.0.3", "--socket"])
        .arg(&socket_path)
        .output()
        .expect("run failed xs ping");
    server.join().expect("fake Agent server joins");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("reachable=false"));
}

#[test]
fn json_output_and_response_type_are_enforced() {
    let temporary = tempfile::tempdir().expect("temporary CLI socket");
    let socket_path = temporary.path().join("agent-json.sock");
    let server = serve_once(
        &socket_path,
        serde_json::json!({"command":"status"}),
        status_response(),
    );
    let output = Command::new(env!("CARGO_BIN_EXE_xs"))
        .args(["status", "--json", "--socket"])
        .arg(&socket_path)
        .output()
        .expect("run xs status JSON");
    server.join().expect("fake Agent server joins");
    assert!(output.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid JSON CLI output");
    assert_eq!(value["type"], "status");

    let mismatch_path = temporary.path().join("agent-mismatch.sock");
    let mismatch_server = serve_once(
        &mismatch_path,
        serde_json::json!({"command":"status"}),
        serde_json::json!({
            "type":"peers",
            "schema_version":1,
            "peers":[],
            "total":0,
            "truncated":false
        }),
    );
    let mismatch = Command::new(env!("CARGO_BIN_EXE_xs"))
        .args(["status", "--socket"])
        .arg(&mismatch_path)
        .output()
        .expect("run mismatched xs status");
    mismatch_server.join().expect("mismatch server joins");
    assert!(!mismatch.status.success());
}

fn serve_once(
    path: &Path,
    expected_request: serde_json::Value,
    response: serde_json::Value,
) -> thread::JoinHandle<()> {
    let listener = UnixListener::bind(path).expect("bind fake Agent socket");
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept xs client");
        let mut prefix = [0_u8; 4];
        stream.read_exact(&mut prefix).expect("read request length");
        let request_length = usize::try_from(u32::from_be_bytes(prefix)).expect("request length");
        let mut request = vec![0_u8; request_length];
        stream.read_exact(&mut request).expect("read xs request");
        let actual_request: serde_json::Value =
            serde_json::from_slice(&request).expect("valid request JSON");
        assert_eq!(actual_request, expected_request);
        let encoded = serde_json::to_vec(&response).expect("serialize fake response");
        let response_length = u32::try_from(encoded.len()).expect("response length");
        stream
            .write_all(&response_length.to_be_bytes())
            .expect("write response length");
        stream.write_all(&encoded).expect("write fake response");
    })
}

fn status_response() -> serde_json::Value {
    serde_json::json!({
        "type":"status",
        "schema_version":1,
        "status":status_value()
    })
}

fn status_value() -> serde_json::Value {
    serde_json::json!({
        "node_id_base64":"node-1",
        "virtual_ip":"100.64.0.2",
        "controller_connected":true,
        "network_active":true,
        "interface_name":"xsn0",
        "interface_index":7,
        "configuration_version":4,
        "uptime_seconds":9
    })
}
