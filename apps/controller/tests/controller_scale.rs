use std::{
    fs,
    net::SocketAddr,
    path::PathBuf,
    process::Stdio,
    time::{Duration, Instant},
};

use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::{Signer, SigningKey};
use futures_util::{SinkExt, StreamExt, stream};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    process::{Child, Command},
    time::timeout,
};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async_with_config,
    tungstenite::{Message, protocol::WebSocketConfig},
};
use tower::ServiceExt;
use url::Url;
use xs_controller::config::{ControllerConfig, MigrationConfig};

const ADMIN_TOKEN: &str = "scale-admin-token-with-at-least-32-characters";
const CHECKPOINTS: [usize; 3] = [100, 500, 1000];
const TOKEN_COUNT: usize = 20;
const REGISTRATION_CONCURRENCY: usize = 32;
const CONTROL_CONCURRENCY: usize = 64;
const CONTROL_SYNC_CONCURRENCY: usize = 128;
const CONTROL_HOLD_SECONDS: u64 = 5;
const CONTROL_BUFFER_SIZE: usize = 4 * 1024;
const CONTROL_MESSAGE_LIMIT: usize = 512 * 1024;
const CONTROL_WRITE_BUFFER_LIMIT: usize = 1024 * 1024;
const MINIMUM_REGISTRATIONS_PER_SECOND: f64 = 5.0;
const MINIMUM_CONTROL_AUTHENTICATIONS_PER_SECOND: f64 = 5.0;
const MAXIMUM_CONSOLE_SNAPSHOT_MILLISECONDS: f64 = 2_000.0;
const MAXIMUM_DATABASE_QUERY_MILLISECONDS: f64 = 100.0;
const MAXIMUM_CONTROL_AUTHENTICATION_P95_MILLISECONDS: f64 = 10_000.0;
const MAXIMUM_CONTROL_SYNC_P95_MILLISECONDS: f64 = 5_000.0;
const MAXIMUM_PRESENCE_CONVERGENCE_MILLISECONDS: f64 = 30_000.0;
const MAXIMUM_CONTROLLER_RSS_KIB: f64 = 524_288.0;
const MAXIMUM_CONTROLLER_FILE_DESCRIPTORS: f64 = 2_500.0;
const MAXIMUM_DATABASE_CONNECTIONS: u32 = 16;
const CONTROL_AUTHENTICATION_DOMAIN: &[u8] = b"XS Nexus control authentication v1";

type ControlSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

struct ScaleNode {
    enrollment: Value,
    identity: SigningKey,
}

struct AuthenticatedControl {
    socket: ControlSocket,
    version: u64,
    authentication_milliseconds: f64,
}

struct ControllerProcess {
    address: SocketAddr,
    child: Child,
    _directory: tempfile::TempDir,
}

struct ControlSynchronization {
    sockets: Vec<ControlSocket>,
    elapsed: Duration,
    milliseconds: Vec<f64>,
}

#[tokio::test]
async fn controller_registers_and_queries_one_thousand_nodes() {
    let config = test_config();
    xs_controller::db::migrate(&MigrationConfig {
        database_url: config.database_url.clone(),
        database_schema: config.database_schema.clone(),
        database_owner_role: None,
        database_app_role: None,
    })
    .await
    .expect("controller database migrates");
    let (router, state) = xs_controller::build(&config)
        .await
        .expect("controller database initializes");
    reset_database(&state.pool).await;
    let tokens = create_scale_network_and_tokens(&router).await;

    let started = Instant::now();
    let mut previous = 0;
    let mut measurements = Vec::new();
    let mut nodes = Vec::with_capacity(CHECKPOINTS[2]);
    for checkpoint in CHECKPOINTS {
        measurements.push(
            register_and_measure(
                &router,
                &state.pool,
                &tokens,
                &mut nodes,
                previous,
                checkpoint,
                started,
            )
            .await,
        );
        previous = checkpoint;
    }
    drop(router);
    state.pool.close().await;
    drop(state);
    let control = measure_control_capacity(&config, &nodes).await;

    let report = json!({
        "package_version": env!("CARGO_PKG_VERSION"),
        "git_revision": std::env::var("XS_SCALE_GIT_REVISION").ok(),
        "registration_concurrency": REGISTRATION_CONCURRENCY,
        "token_count": TOKEN_COUNT,
        "load_generator_websocket": {
            "read_buffer_bytes": CONTROL_BUFFER_SIZE,
            "write_buffer_bytes": CONTROL_BUFFER_SIZE,
            "maximum_write_buffer_bytes": CONTROL_WRITE_BUFFER_LIMIT,
            "maximum_message_bytes": CONTROL_MESSAGE_LIMIT
        },
        "measurements": measurements,
        "control": control,
        "safe_operating_envelope": {
            "minimum_registrations_per_second": MINIMUM_REGISTRATIONS_PER_SECOND,
            "minimum_control_authentications_per_second": MINIMUM_CONTROL_AUTHENTICATIONS_PER_SECOND,
            "maximum_console_snapshot_milliseconds": MAXIMUM_CONSOLE_SNAPSHOT_MILLISECONDS,
            "maximum_database_query_milliseconds": MAXIMUM_DATABASE_QUERY_MILLISECONDS,
            "maximum_control_authentication_p95_milliseconds": MAXIMUM_CONTROL_AUTHENTICATION_P95_MILLISECONDS,
            "maximum_control_sync_p95_milliseconds": MAXIMUM_CONTROL_SYNC_P95_MILLISECONDS,
            "maximum_presence_convergence_milliseconds": MAXIMUM_PRESENCE_CONVERGENCE_MILLISECONDS,
            "maximum_controller_rss_kib": MAXIMUM_CONTROLLER_RSS_KIB,
            "maximum_controller_file_descriptors": MAXIMUM_CONTROLLER_FILE_DESCRIPTORS,
            "maximum_database_connections": MAXIMUM_DATABASE_CONNECTIONS
        }
    });
    if let Some(path) = std::env::var_os("XS_SCALE_REPORT_PATH") {
        let path = PathBuf::from(path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create scale report directory");
        }
        std::fs::write(
            path,
            serde_json::to_vec_pretty(&report).expect("serialize scale report"),
        )
        .expect("write scale report");
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&report).expect("report JSON")
    );
    assert_safe_operating_envelope(&report);
}

async fn create_scale_network_and_tokens(router: &Router) -> Vec<String> {
    let network = request_json(
        router,
        Method::POST,
        "/v1/admin/networks",
        Some(json!({
            "name": "scale-network",
            "address_pool": "100.96.0.0/16",
            "reserved_addresses": 16
        })),
        Some(ADMIN_TOKEN),
    )
    .await;
    assert_eq!(network.0, StatusCode::CREATED);
    let network_id = network.1["id"].as_str().expect("network id");
    let mut tokens = Vec::with_capacity(TOKEN_COUNT);
    for _ in 0..TOKEN_COUNT {
        let response = request_json(
            router,
            Method::POST,
            "/v1/admin/enrollment-tokens",
            Some(json!({
                "network_id": network_id,
                "expires_in_seconds": 3600,
                "max_uses": CHECKPOINTS[2] / TOKEN_COUNT,
                "default_role_bitmap": 1,
                "default_tags": ["scale"]
            })),
            Some(ADMIN_TOKEN),
        )
        .await;
        assert_eq!(response.0, StatusCode::CREATED);
        tokens.push(response.1["token"].as_str().expect("token").to_owned());
    }
    tokens
}

async fn register_and_measure(
    router: &Router,
    pool: &sqlx::PgPool,
    tokens: &[String],
    nodes: &mut Vec<ScaleNode>,
    previous: usize,
    checkpoint: usize,
    started: Instant,
) -> Value {
    let batch_started = Instant::now();
    let registered = stream::iter(previous..checkpoint)
        .map(|index| enroll_scale_node(router.clone(), tokens[index % TOKEN_COUNT].clone(), index))
        .buffer_unordered(REGISTRATION_CONCURRENCY)
        .collect::<Vec<_>>()
        .await;
    nodes.extend(registered);
    let batch_elapsed = batch_started.elapsed();

    let snapshot_started = Instant::now();
    let snapshot = request_json(
        router,
        Method::GET,
        "/v1/admin/console",
        None,
        Some(ADMIN_TOKEN),
    )
    .await;
    let snapshot_elapsed = snapshot_started.elapsed();
    assert_eq!(snapshot.0, StatusCode::OK);
    assert_eq!(
        snapshot.1["nodes"].as_array().map(Vec::len),
        Some(checkpoint)
    );

    let query_started = Instant::now();
    let (active_nodes, distinct_addresses): (i64, i64) = sqlx::query_as(
        "SELECT count(*)::bigint, count(DISTINCT virtual_ip)::bigint
         FROM nodes WHERE revoked_at IS NULL",
    )
    .fetch_one(pool)
    .await
    .expect("query node consistency");
    let query_elapsed = query_started.elapsed();
    assert_eq!(
        active_nodes,
        i64::try_from(checkpoint).expect("checkpoint fits i64")
    );
    assert_eq!(distinct_addresses, active_nodes);
    let batch_nodes = u32::try_from(checkpoint - previous).expect("batch size fits u32");
    let registrations_per_second = f64::from(batch_nodes) / batch_elapsed.as_secs_f64();
    let snapshot_milliseconds = snapshot_elapsed.as_secs_f64() * 1000.0;
    let query_milliseconds = query_elapsed.as_secs_f64() * 1000.0;
    json!({
        "nodes": checkpoint,
        "batch_nodes": batch_nodes,
        "batch_elapsed_ms": batch_elapsed.as_secs_f64() * 1000.0,
        "batch_registrations_per_second": registrations_per_second,
        "cumulative_elapsed_ms": started.elapsed().as_secs_f64() * 1000.0,
        "console_snapshot_ms": snapshot_milliseconds,
        "database_count_query_ms": query_milliseconds,
        "active_nodes": active_nodes,
        "distinct_addresses": distinct_addresses
    })
}

async fn enroll_scale_node(router: Router, token: String, index: usize) -> ScaleNode {
    let seed: [u8; 32] = Sha256::digest(index.to_be_bytes()).into();
    let identity = SigningKey::from_bytes(&seed);
    let public_key = identity.verifying_key().to_bytes();
    let response = request_json(
        &router,
        Method::POST,
        "/v1/enroll",
        Some(json!({
            "token": token,
            "name": format!("scale-node-{index:04}"),
            "device_type": "linux",
            "identity_public_key_base64": URL_SAFE_NO_PAD.encode(public_key)
        })),
        None,
    )
    .await;
    assert_eq!(response.0, StatusCode::CREATED);
    ScaleNode {
        enrollment: response.1,
        identity,
    }
}

async fn measure_control_capacity(config: &ControllerConfig, nodes: &[ScaleNode]) -> Value {
    let mut controller = start_controller_process(config).await;
    let authentication_started = Instant::now();
    let authenticated = timeout(
        Duration::from_secs(300),
        stream::iter(nodes)
            .map(|node| authenticate_control(controller.address, node))
            .buffer_unordered(CONTROL_CONCURRENCY)
            .collect::<Vec<_>>(),
    )
    .await
    .expect("one thousand control sockets authenticate within five minutes");
    let authentication_elapsed = authentication_started.elapsed();
    assert_eq!(authenticated.len(), CHECKPOINTS[2]);
    let expected_nodes = u64::try_from(nodes.len()).expect("node count fits u64");
    let authenticated_count = u32::try_from(authenticated.len()).expect("node count fits u32");
    let authentication_milliseconds = authenticated
        .iter()
        .map(|control| control.authentication_milliseconds)
        .collect::<Vec<_>>();
    let authentication_rate = f64::from(authenticated_count) / authentication_elapsed.as_secs_f64();
    let authentication_p95 = percentile(&authentication_milliseconds, 95);

    let online_convergence = wait_console_online_count(controller.address, expected_nodes).await;
    let ControlSynchronization {
        sockets,
        elapsed: synchronization_elapsed,
        milliseconds: synchronization_milliseconds,
    } = synchronize_controls(authenticated).await;
    let snapshot_started = Instant::now();
    assert_eq!(
        console_online_count(controller.address).await,
        expected_nodes
    );
    let snapshot_milliseconds = snapshot_started.elapsed().as_secs_f64() * 1000.0;
    tokio::time::sleep(Duration::from_secs(CONTROL_HOLD_SECONDS)).await;
    assert_eq!(
        console_online_count(controller.address).await,
        expected_nodes
    );

    let controller_process_id = controller.process_id();
    let (controller_rss_kib, controller_file_descriptors) =
        process_resource_snapshot(controller_process_id);
    let current_process_id = std::process::id();
    let (load_generator_rss_kib, load_generator_file_descriptors) =
        process_resource_snapshot(current_process_id);
    let database_connections = database_connection_count(config).await;

    close_controls(sockets).await;
    let offline_convergence = wait_console_online_count(controller.address, 0).await;
    controller.stop().await;

    json!({
        "connections": CHECKPOINTS[2],
        "authentication_concurrency": CONTROL_CONCURRENCY,
        "authentication_elapsed_ms": authentication_elapsed.as_secs_f64() * 1000.0,
        "authentications_per_second": authentication_rate,
        "authentication_average_ms": average(&authentication_milliseconds),
        "authentication_p95_ms": authentication_p95,
        "online_convergence_ms": online_convergence.as_secs_f64() * 1000.0,
        "synchronization_concurrency": CONTROL_SYNC_CONCURRENCY,
        "synchronization_elapsed_ms": synchronization_elapsed.as_secs_f64() * 1000.0,
        "synchronization_average_ms": average(&synchronization_milliseconds),
        "synchronization_p95_ms": percentile(&synchronization_milliseconds, 95),
        "console_snapshot_ms": snapshot_milliseconds,
        "hold_seconds": CONTROL_HOLD_SECONDS,
        "controller_process_id": controller_process_id,
        "controller_process_rss_kib": controller_rss_kib,
        "controller_process_file_descriptors": controller_file_descriptors,
        "load_generator_process_id": current_process_id,
        "load_generator_process_rss_kib": load_generator_rss_kib,
        "load_generator_process_file_descriptors": load_generator_file_descriptors,
        "database_connections": database_connections,
        "offline_convergence_ms": offline_convergence.as_secs_f64() * 1000.0
    })
}

async fn start_controller_process(config: &ControllerConfig) -> ControllerProcess {
    let listener =
        std::net::TcpListener::bind(("127.0.0.1", 0)).expect("reserve scale Controller address");
    let address = listener.local_addr().expect("scale Controller address");
    drop(listener);
    let directory = tempfile::tempdir().expect("create Controller process directory");
    let credential_key = directory.path().join("credential.key");
    let configuration_key = directory.path().join("configuration.key");
    write_private_file(&credential_key, &config.credential_signing_key.to_bytes());
    write_private_file(&configuration_key, &config.config_signing_key.to_bytes());
    let database_role = Url::parse(&config.database_url)
        .expect("parse scale database URL")
        .username()
        .to_owned();
    assert!(!database_role.is_empty());
    let child = Command::new(env!("CARGO_BIN_EXE_xs-controller"))
        .arg("serve")
        .env_clear()
        .env("CONTROLLER_LISTEN", address.to_string())
        .env("DATABASE_URL", &config.database_url)
        .env("DATABASE_SCHEMA", &config.database_schema)
        .env("DATABASE_EXPECTED_ROLE", database_role)
        .env("ADMIN_API_TOKEN", ADMIN_TOKEN)
        .env("CREDENTIAL_SIGNING_KEY_PATH", &credential_key)
        .env("CONFIG_SIGNING_KEY_PATH", &configuration_key)
        .env("CONSOLE_COOKIE_SECURE", "true")
        .env("NODE_CREDENTIAL_TTL_SECONDS", "86400")
        .env("RUST_LOG", "warn")
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .expect("start scale Controller binary");
    let mut controller = ControllerProcess {
        address,
        child,
        _directory: directory,
    };
    for _attempt in 0..300 {
        if let Some(status) = controller
            .child
            .try_wait()
            .expect("inspect scale Controller process")
        {
            panic!("scale Controller exited during startup with {status}");
        }
        if xs_core::check_local_http_health(address, "/health/ready", Duration::from_millis(500))
            .is_ok()
        {
            return controller;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("scale Controller did not become ready");
}

async fn authenticate_control(address: SocketAddr, node: &ScaleNode) -> AuthenticatedControl {
    let started = Instant::now();
    let websocket_config = WebSocketConfig::default()
        .read_buffer_size(CONTROL_BUFFER_SIZE)
        .write_buffer_size(CONTROL_BUFFER_SIZE)
        .max_write_buffer_size(CONTROL_WRITE_BUFFER_LIMIT)
        .max_message_size(Some(CONTROL_MESSAGE_LIMIT))
        .max_frame_size(Some(CONTROL_MESSAGE_LIMIT));
    let (mut socket, _) = connect_async_with_config(
        format!("ws://{address}/v1/control"),
        Some(websocket_config),
        false,
    )
    .await
    .expect("connect control socket");
    let challenge = next_text(&mut socket, "challenge").await;
    let challenge: Value = serde_json::from_str(&challenge).expect("challenge JSON");
    let challenge = URL_SAFE_NO_PAD
        .decode(
            challenge["challenge_base64"]
                .as_str()
                .expect("challenge value"),
        )
        .expect("decode challenge");
    assert_eq!(challenge.len(), 32);
    let node_id = URL_SAFE_NO_PAD
        .decode(node.enrollment["node_id_base64"].as_str().expect("node ID"))
        .expect("decode node ID");
    let mut input = Vec::with_capacity(CONTROL_AUTHENTICATION_DOMAIN.len() + 48);
    input.extend_from_slice(CONTROL_AUTHENTICATION_DOMAIN);
    input.extend_from_slice(&challenge);
    input.extend_from_slice(&node_id);
    socket
        .send(Message::Text(
            json!({
                "type": "authenticate",
                "node_id_base64": node.enrollment["node_id_base64"],
                "credential_base64": node.enrollment["credential_base64"],
                "signature_base64": URL_SAFE_NO_PAD.encode(node.identity.sign(&input).to_bytes())
            })
            .to_string()
            .into(),
        ))
        .await
        .expect("send control authentication");
    let authenticated = next_text(&mut socket, "authenticated response").await;
    let authenticated: Value = serde_json::from_str(&authenticated).expect("authenticated JSON");
    assert_eq!(authenticated["type"], "authenticated");
    AuthenticatedControl {
        socket,
        version: authenticated["configuration"]["version"]
            .as_u64()
            .expect("configuration version"),
        authentication_milliseconds: started.elapsed().as_secs_f64() * 1000.0,
    }
}

async fn next_text(socket: &mut ControlSocket, name: &str) -> String {
    let message = timeout(Duration::from_secs(30), socket.next())
        .await
        .unwrap_or_else(|_| panic!("{name} timed out"))
        .unwrap_or_else(|| panic!("{name} missing"))
        .unwrap_or_else(|error| panic!("{name} failed: {error}"));
    let Message::Text(text) = message else {
        panic!("{name} was not text");
    };
    text.to_string()
}

async fn synchronize_controls(controls: Vec<AuthenticatedControl>) -> ControlSynchronization {
    let started = Instant::now();
    let synchronized = timeout(
        Duration::from_secs(60),
        stream::iter(controls)
            .map(synchronize_control)
            .buffer_unordered(CONTROL_SYNC_CONCURRENCY)
            .collect::<Vec<_>>(),
    )
    .await
    .expect("control synchronization completes");
    let elapsed = started.elapsed();
    let mut sockets = Vec::with_capacity(synchronized.len());
    let mut milliseconds = Vec::with_capacity(synchronized.len());
    for (socket, value) in synchronized {
        sockets.push(socket);
        milliseconds.push(value);
    }
    ControlSynchronization {
        sockets,
        elapsed,
        milliseconds,
    }
}

async fn synchronize_control(mut control: AuthenticatedControl) -> (ControlSocket, f64) {
    let started = Instant::now();
    control
        .socket
        .send(Message::Text(
            json!({"type": "sync", "last_version": control.version})
                .to_string()
                .into(),
        ))
        .await
        .expect("send control sync");
    let response = next_text(&mut control.socket, "control sync response").await;
    let response: Value = serde_json::from_str(&response).expect("control sync JSON");
    assert_eq!(response["type"], "up_to_date");
    assert_eq!(response["version"], control.version);
    (control.socket, started.elapsed().as_secs_f64() * 1000.0)
}

async fn wait_console_online_count(address: SocketAddr, expected: u64) -> Duration {
    let started = Instant::now();
    timeout(Duration::from_secs(60), async {
        loop {
            if console_online_count(address).await == expected {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("online node count converges");
    started.elapsed()
}

async fn console_online_count(address: SocketAddr) -> u64 {
    let snapshot = request_admin_json(address, "/v1/admin/console").await;
    snapshot["dashboard"]["online_nodes"]
        .as_u64()
        .expect("online node count")
}

fn process_resource_snapshot(process_id: u32) -> (u64, u64) {
    let process = PathBuf::from("/proc").join(process_id.to_string());
    let status = fs::read_to_string(process.join("status")).expect("read process status");
    let rss_kib = status
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))
        .and_then(|value| value.split_whitespace().next())
        .and_then(|value| value.parse::<u64>().ok())
        .expect("process RSS");
    let file_descriptors = u64::try_from(
        fs::read_dir(process.join("fd"))
            .expect("read process descriptors")
            .count(),
    )
    .expect("descriptor count fits u64");
    (rss_kib, file_descriptors)
}

async fn database_connection_count(config: &ControllerConfig) -> u32 {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(&config.database_url)
        .await
        .expect("connect database capacity probe");
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint FROM pg_stat_activity
         WHERE datname = current_database() AND usename = current_user",
    )
    .fetch_one(&pool)
    .await
    .expect("count database connections");
    pool.close().await;
    u32::try_from(count).expect("database connection count fits u32")
}

async fn request_admin_json(address: SocketAddr, path: &str) -> Value {
    let mut stream = TcpStream::connect(address)
        .await
        .expect("connect Controller admin endpoint");
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {ADMIN_TOKEN}\r\nConnection: close\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .await
        .expect("send Controller admin request");
    let mut response = Vec::new();
    stream
        .take(8 * 1024 * 1024)
        .read_to_end(&mut response)
        .await
        .expect("read Controller admin response");
    let header_end = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .expect("Controller admin response headers");
    let headers = std::str::from_utf8(&response[..header_end]).expect("HTTP response headers");
    assert!(
        headers
            .lines()
            .next()
            .is_some_and(|line| line.contains(" 200 ")),
        "Controller admin request failed: {headers}"
    );
    serde_json::from_slice(&response[header_end + 4..]).expect("Controller admin JSON")
}

fn write_private_file(path: &std::path::Path, value: &[u8]) {
    fs::write(path, value).expect("write private Controller file");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .expect("protect private Controller file");
    }
}

impl ControllerProcess {
    fn process_id(&self) -> u32 {
        self.child.id().expect("scale Controller process ID")
    }

    async fn stop(&mut self) {
        if self
            .child
            .try_wait()
            .expect("inspect scale Controller shutdown")
            .is_none()
        {
            self.child
                .start_kill()
                .expect("terminate scale Controller process");
        }
        self.child
            .wait()
            .await
            .expect("reap scale Controller process");
    }
}

fn assert_safe_operating_envelope(report: &Value) {
    for measurement in report["measurements"]
        .as_array()
        .expect("scale measurements")
    {
        let nodes = measurement["nodes"].as_u64().expect("measurement nodes");
        assert_minimum(
            &format!("{nodes}-node registration rate"),
            report_number(measurement, "batch_registrations_per_second"),
            MINIMUM_REGISTRATIONS_PER_SECOND,
        );
        assert_maximum(
            &format!("{nodes}-node Console snapshot"),
            report_number(measurement, "console_snapshot_ms"),
            MAXIMUM_CONSOLE_SNAPSHOT_MILLISECONDS,
        );
        assert_maximum(
            &format!("{nodes}-node database count query"),
            report_number(measurement, "database_count_query_ms"),
            MAXIMUM_DATABASE_QUERY_MILLISECONDS,
        );
    }
    let control = &report["control"];
    assert_minimum(
        "control authentication rate",
        report_number(control, "authentications_per_second"),
        MINIMUM_CONTROL_AUTHENTICATIONS_PER_SECOND,
    );
    assert_maximum(
        "control authentication p95",
        report_number(control, "authentication_p95_ms"),
        MAXIMUM_CONTROL_AUTHENTICATION_P95_MILLISECONDS,
    );
    assert_maximum(
        "control synchronization p95",
        report_number(control, "synchronization_p95_ms"),
        MAXIMUM_CONTROL_SYNC_P95_MILLISECONDS,
    );
    assert_maximum(
        "online presence convergence",
        report_number(control, "online_convergence_ms"),
        MAXIMUM_PRESENCE_CONVERGENCE_MILLISECONDS,
    );
    assert_maximum(
        "offline presence convergence",
        report_number(control, "offline_convergence_ms"),
        MAXIMUM_PRESENCE_CONVERGENCE_MILLISECONDS,
    );
    assert_maximum(
        "control Console snapshot",
        report_number(control, "console_snapshot_ms"),
        MAXIMUM_CONSOLE_SNAPSHOT_MILLISECONDS,
    );
    assert_maximum(
        "Controller RSS KiB",
        report_number(control, "controller_process_rss_kib"),
        MAXIMUM_CONTROLLER_RSS_KIB,
    );
    assert_maximum(
        "Controller file descriptors",
        report_number(control, "controller_process_file_descriptors"),
        MAXIMUM_CONTROLLER_FILE_DESCRIPTORS,
    );
    assert_maximum(
        "database connections",
        report_number(control, "database_connections"),
        f64::from(MAXIMUM_DATABASE_CONNECTIONS),
    );
}

fn report_number(report: &Value, name: &str) -> f64 {
    report[name]
        .as_f64()
        .unwrap_or_else(|| panic!("missing numeric report field {name}"))
}

fn assert_minimum(name: &str, actual: f64, minimum: f64) {
    assert!(
        actual >= minimum,
        "{name} {actual:.3} is below minimum {minimum:.3}"
    );
}

fn assert_maximum(name: &str, actual: f64, maximum: f64) {
    assert!(
        actual <= maximum,
        "{name} {actual:.3} exceeds maximum {maximum:.3}"
    );
}

fn average(values: &[f64]) -> f64 {
    assert!(!values.is_empty());
    let count = u32::try_from(values.len()).expect("sample count fits u32");
    values.iter().sum::<f64>() / f64::from(count)
}

fn percentile(values: &[f64], percentile_value: usize) -> f64 {
    assert!(!values.is_empty());
    assert!((1..=100).contains(&percentile_value));
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let rank = (sorted.len() * percentile_value).div_ceil(100) - 1;
    sorted[rank]
}

async fn close_controls(sockets: Vec<ControlSocket>) {
    let results = timeout(
        Duration::from_secs(60),
        stream::iter(sockets)
            .map(|mut socket| async move { socket.close(None).await })
            .buffer_unordered(CONTROL_SYNC_CONCURRENCY)
            .collect::<Vec<_>>(),
    )
    .await
    .expect("control sockets close");
    assert!(results.iter().all(Result::is_ok));
}

fn test_config() -> ControllerConfig {
    let database_url =
        std::env::var("XS_TEST_DATABASE_URL").expect("XS_TEST_DATABASE_URL is required");
    let database_schema =
        std::env::var("XS_TEST_DATABASE_SCHEMA").unwrap_or_else(|_| "xs_nexus_scale".to_owned());
    ControllerConfig {
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        discovery_listen: None,
        discovery_public_endpoint: None,
        database_url,
        database_schema,
        database_expected_role: None,
        admin_token_hash: Sha256::digest(ADMIN_TOKEN.as_bytes()).into(),
        console_bootstrap_username: None,
        console_bootstrap_password: None,
        console_cookie_secure: true,
        console_session_ttl_seconds: 28_800,
        credential_signing_key: SigningKey::from_bytes(&[31_u8; 32]),
        config_signing_key: SigningKey::from_bytes(&[32_u8; 32]),
        update_signing_public_key: None,
        linux_release_directory: None,
        windows_release_directory: None,
        credential_ttl_seconds: 86_400,
        relays: Vec::new(),
    }
}

async fn reset_database(pool: &sqlx::PgPool) {
    sqlx::query(
        "TRUNCATE update_rollout_policies, update_releases,
                  console_login_attempts, console_sessions, audit_events,
                  configuration_versions, subnet_routes,
                  node_subnet_route_advertisements, acl_rules,
                  node_group_memberships, node_groups, ip_leases, nodes,
                  enrollment_tokens, networks
         RESTART IDENTITY CASCADE",
    )
    .execute(pool)
    .await
    .expect("truncate scale tables");
    sqlx::query("ALTER SEQUENCE credential_serial RESTART WITH 1")
        .execute(pool)
        .await
        .expect("reset credential serial");
}

async fn request_json(
    router: &Router,
    method: Method,
    path: &str,
    body: Option<Value>,
    admin_token: Option<&str>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(path);
    if let Some(admin_token) = admin_token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {admin_token}"));
    }
    let body = if let Some(body) = body {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
        Body::from(serde_json::to_vec(&body).expect("serialize body"))
    } else {
        Body::empty()
    };
    let response = router
        .clone()
        .oneshot(builder.body(body).expect("request"))
        .await
        .expect("router response");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("collect response")
        .to_bytes();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("JSON response")
    };
    (status, value)
}
