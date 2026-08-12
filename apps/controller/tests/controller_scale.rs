use std::{
    fs,
    net::SocketAddr,
    path::PathBuf,
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
use tokio::{net::TcpStream, sync::oneshot, time::timeout};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};
use tower::ServiceExt;
use xs_controller::config::{ControllerConfig, MigrationConfig};

const ADMIN_TOKEN: &str = "scale-admin-token-with-at-least-32-characters";
const CHECKPOINTS: [usize; 3] = [100, 500, 1000];
const TOKEN_COUNT: usize = 20;
const REGISTRATION_CONCURRENCY: usize = 32;
const CONTROL_CONCURRENCY: usize = 64;
const CONTROL_SYNC_CONCURRENCY: usize = 128;
const CONTROL_HOLD_SECONDS: u64 = 5;
const MINIMUM_REGISTRATIONS_PER_SECOND: f64 = 5.0;
const MINIMUM_CONTROL_AUTHENTICATIONS_PER_SECOND: f64 = 5.0;
const MAXIMUM_CONSOLE_SNAPSHOT_MILLISECONDS: f64 = 2_000.0;
const MAXIMUM_DATABASE_QUERY_MILLISECONDS: f64 = 100.0;
const MAXIMUM_CONTROL_AUTHENTICATION_P95_MILLISECONDS: f64 = 10_000.0;
const MAXIMUM_CONTROL_SYNC_P95_MILLISECONDS: f64 = 5_000.0;
const MAXIMUM_PRESENCE_CONVERGENCE_MILLISECONDS: f64 = 30_000.0;
const MAXIMUM_PROCESS_RSS_KIB: u64 = 1_048_576;
const MAXIMUM_PROCESS_FILE_DESCRIPTORS: u64 = 5_000;
const MAXIMUM_DATABASE_POOL_CONNECTIONS: u32 = 128;
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

struct ControlServer {
    address: SocketAddr,
    shutdown: oneshot::Sender<()>,
    task: tokio::task::JoinHandle<()>,
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
    let control = measure_control_capacity(&router, &state.pool, &nodes).await;

    let report = json!({
        "package_version": env!("CARGO_PKG_VERSION"),
        "git_revision": std::env::var("XS_SCALE_GIT_REVISION").ok(),
        "registration_concurrency": REGISTRATION_CONCURRENCY,
        "token_count": TOKEN_COUNT,
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
            "maximum_process_rss_kib": MAXIMUM_PROCESS_RSS_KIB,
            "maximum_process_file_descriptors": MAXIMUM_PROCESS_FILE_DESCRIPTORS,
            "maximum_database_pool_connections": MAXIMUM_DATABASE_POOL_CONNECTIONS
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
    assert!(registrations_per_second >= MINIMUM_REGISTRATIONS_PER_SECOND);
    assert!(snapshot_milliseconds <= MAXIMUM_CONSOLE_SNAPSHOT_MILLISECONDS);
    assert!(query_milliseconds <= MAXIMUM_DATABASE_QUERY_MILLISECONDS);

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

async fn measure_control_capacity(
    router: &Router,
    pool: &sqlx::PgPool,
    nodes: &[ScaleNode],
) -> Value {
    let server = start_control_server(router).await;
    let authentication_started = Instant::now();
    let authenticated = timeout(
        Duration::from_secs(300),
        stream::iter(nodes)
            .map(|node| authenticate_control(server.address, node))
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
    assert!(authentication_rate >= MINIMUM_CONTROL_AUTHENTICATIONS_PER_SECOND);
    assert!(authentication_p95 <= MAXIMUM_CONTROL_AUTHENTICATION_P95_MILLISECONDS);

    let online_convergence = wait_console_online_count(router, expected_nodes).await;
    assert!(online_convergence.as_secs_f64() * 1000.0 <= MAXIMUM_PRESENCE_CONVERGENCE_MILLISECONDS);
    let ControlSynchronization {
        sockets,
        elapsed: synchronization_elapsed,
        milliseconds: synchronization_milliseconds,
    } = synchronize_controls(authenticated).await;
    let snapshot_started = Instant::now();
    assert_eq!(console_online_count(router).await, expected_nodes);
    let snapshot_milliseconds = snapshot_started.elapsed().as_secs_f64() * 1000.0;
    assert!(snapshot_milliseconds <= MAXIMUM_CONSOLE_SNAPSHOT_MILLISECONDS);
    tokio::time::sleep(Duration::from_secs(CONTROL_HOLD_SECONDS)).await;
    assert_eq!(console_online_count(router).await, expected_nodes);

    let (rss_kib, file_descriptors) = process_resource_snapshot();
    let database_pool_connections = pool.size();
    assert!(rss_kib <= MAXIMUM_PROCESS_RSS_KIB);
    assert!(file_descriptors <= MAXIMUM_PROCESS_FILE_DESCRIPTORS);
    assert!(database_pool_connections <= MAXIMUM_DATABASE_POOL_CONNECTIONS);

    close_controls(sockets).await;
    let offline_convergence = wait_console_online_count(router, 0).await;
    assert!(
        offline_convergence.as_secs_f64() * 1000.0 <= MAXIMUM_PRESENCE_CONVERGENCE_MILLISECONDS
    );
    server
        .shutdown
        .send(())
        .expect("request Controller shutdown");
    server.task.await.expect("Controller server stops");

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
        "process_rss_kib": rss_kib,
        "process_file_descriptors": file_descriptors,
        "database_pool_connections": database_pool_connections,
        "offline_convergence_ms": offline_convergence.as_secs_f64() * 1000.0
    })
}

async fn start_control_server(router: &Router) -> ControlServer {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind scale Controller");
    let address = listener.local_addr().expect("scale Controller address");
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let application = router.clone();
    let server = tokio::spawn(async move {
        axum::serve(listener, application)
            .with_graceful_shutdown(async {
                let _shutdown_result = shutdown_rx.await;
            })
            .await
            .expect("serve scale Controller");
    });
    ControlServer {
        address,
        shutdown: shutdown_tx,
        task: server,
    }
}

async fn authenticate_control(address: SocketAddr, node: &ScaleNode) -> AuthenticatedControl {
    let started = Instant::now();
    let (mut socket, _) = connect_async(format!("ws://{address}/v1/control"))
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
    assert!(percentile(&milliseconds, 95) <= MAXIMUM_CONTROL_SYNC_P95_MILLISECONDS);
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

async fn wait_console_online_count(router: &Router, expected: u64) -> Duration {
    let started = Instant::now();
    timeout(Duration::from_secs(60), async {
        loop {
            if console_online_count(router).await == expected {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("online node count converges");
    started.elapsed()
}

async fn console_online_count(router: &Router) -> u64 {
    let (status, snapshot) = request_json(
        router,
        Method::GET,
        "/v1/admin/console",
        None,
        Some(ADMIN_TOKEN),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    snapshot["dashboard"]["online_nodes"]
        .as_u64()
        .expect("online node count")
}

fn process_resource_snapshot() -> (u64, u64) {
    let status = fs::read_to_string("/proc/self/status").expect("read process status");
    let rss_kib = status
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))
        .and_then(|value| value.split_whitespace().next())
        .and_then(|value| value.parse::<u64>().ok())
        .expect("process RSS");
    let file_descriptors = u64::try_from(
        fs::read_dir("/proc/self/fd")
            .expect("read process descriptors")
            .count(),
    )
    .expect("descriptor count fits u64");
    (rss_kib, file_descriptors)
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
