use std::{net::SocketAddr, path::PathBuf, time::Instant};

use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::SigningKey;
use futures_util::{StreamExt, stream};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tower::ServiceExt;
use xs_controller::config::ControllerConfig;

const ADMIN_TOKEN: &str = "scale-admin-token-with-at-least-32-characters";
const CHECKPOINTS: [usize; 3] = [100, 500, 1000];
const TOKEN_COUNT: usize = 20;
const CONCURRENCY: usize = 32;

#[tokio::test]
async fn controller_registers_and_queries_one_thousand_nodes() {
    let config = test_config();
    let (router, state) = xs_controller::build(&config)
        .await
        .expect("controller database initializes");
    reset_database(&state.pool).await;
    let tokens = create_scale_network_and_tokens(&router).await;

    let started = Instant::now();
    let mut previous = 0;
    let mut measurements = Vec::new();
    for checkpoint in CHECKPOINTS {
        measurements.push(
            register_and_measure(&router, &state.pool, &tokens, previous, checkpoint, started)
                .await,
        );
        previous = checkpoint;
    }

    let report = json!({
        "revision": env!("CARGO_PKG_VERSION"),
        "concurrency": CONCURRENCY,
        "token_count": TOKEN_COUNT,
        "measurements": measurements
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
    previous: usize,
    checkpoint: usize,
    started: Instant,
) -> Value {
    let batch_started = Instant::now();
    let results = stream::iter(previous..checkpoint)
        .map(|index| enroll_scale_node(router.clone(), tokens[index % TOKEN_COUNT].clone(), index))
        .buffer_unordered(CONCURRENCY)
        .collect::<Vec<_>>()
        .await;
    assert!(results.iter().all(|status| *status == StatusCode::CREATED));
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

    json!({
        "nodes": checkpoint,
        "batch_nodes": batch_nodes,
        "batch_elapsed_ms": batch_elapsed.as_secs_f64() * 1000.0,
        "batch_registrations_per_second": f64::from(batch_nodes) / batch_elapsed.as_secs_f64(),
        "cumulative_elapsed_ms": started.elapsed().as_secs_f64() * 1000.0,
        "console_snapshot_ms": snapshot_elapsed.as_secs_f64() * 1000.0,
        "database_count_query_ms": query_elapsed.as_secs_f64() * 1000.0,
        "active_nodes": active_nodes,
        "distinct_addresses": distinct_addresses
    })
}

async fn enroll_scale_node(router: Router, token: String, index: usize) -> StatusCode {
    let seed: [u8; 32] = Sha256::digest(index.to_be_bytes()).into();
    let public_key = SigningKey::from_bytes(&seed).verifying_key().to_bytes();
    request_json(
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
    .await
    .0
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
        admin_token_hash: Sha256::digest(ADMIN_TOKEN.as_bytes()).into(),
        console_bootstrap_username: None,
        console_bootstrap_password: None,
        console_cookie_secure: true,
        console_session_ttl_seconds: 28_800,
        credential_signing_key: SigningKey::from_bytes(&[31_u8; 32]),
        config_signing_key: SigningKey::from_bytes(&[32_u8; 32]),
        credential_ttl_seconds: 86_400,
        relays: Vec::new(),
    }
}

async fn reset_database(pool: &sqlx::PgPool) {
    sqlx::query(
        "TRUNCATE console_login_attempts, console_sessions, audit_events,
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
