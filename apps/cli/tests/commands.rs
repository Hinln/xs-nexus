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
    let cases = [
        (
            "status",
            serde_json::json!({"command":"status"}),
            status_response(),
            "node_id=node-1",
        ),
        (
            "peers",
            serde_json::json!({"command":"peers"}),
            serde_json::json!({
                "type":"peers",
                "schema_version":1,
                "peers":[{
                    "node_id_base64":"peer-1",
                    "virtual_ip":"100.64.0.3",
                    "credential_not_after":"2026-08-29T00:00:00Z",
                    "role_bitmap":2,
                    "tags":["server"]
                }],
                "total":1,
                "truncated":false
            }),
            "configured_peers=1 returned=1 truncated=false",
        ),
        (
            "diagnostics",
            serde_json::json!({"command":"diagnostics"}),
            serde_json::json!({
                "type":"diagnostics",
                "schema_version":1,
                "diagnostics":{
                    "status":status_value(),
                    "configuration_generated_at":"2026-07-29T00:00:00Z",
                    "address_pool":"100.64.0.0/24",
                    "configuration_sha256":"abcd",
                    "tun_packets_received":2,
                    "tun_packets_dropped":1,
                    "last_error_code":null
                }
            }),
            "tun_packets_dropped=1",
        ),
    ];

    for (index, (command, expected_request, response, expected_output)) in
        cases.into_iter().enumerate()
    {
        let socket_path = temporary.path().join(format!("agent-{index}.sock"));
        let server = serve_once(&socket_path, expected_request, response);
        let output = Command::new(env!("CARGO_BIN_EXE_xs"))
            .arg(command)
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
        let mut request = Vec::new();
        stream.read_to_end(&mut request).expect("read xs request");
        let actual_request: serde_json::Value =
            serde_json::from_slice(&request).expect("valid request JSON");
        assert_eq!(actual_request, expected_request);
        let encoded = serde_json::to_vec(&response).expect("serialize fake response");
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
