#![cfg(target_os = "linux")]

use std::{
    net::{Ipv4Addr, SocketAddr},
    os::unix::fs::OpenOptionsExt as _,
    path::Path,
    sync::Arc,
    time::Duration,
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use ed25519_dalek::SigningKey;
use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use tokio::sync::{RwLock, oneshot, watch};
use xs_agent::{
    config::AgentConfig,
    control::run_control_loop,
    enrollment::enroll,
    health::AgentHealth,
    state::NodeState,
    storage::{Identity, read_json},
};
use xs_controller::config::ControllerConfig;
use xs_core::{CandidateAdvertisement, EndpointCandidate, EndpointCandidateKind};

const ADMIN_TOKEN: &str = "agent-integration-admin-token-32-characters";

#[tokio::test]
async fn agent_enrolls_authenticates_and_applies_new_configuration() {
    let controller_config = controller_config();
    let (router, controller_state) = xs_controller::build(&controller_config)
        .await
        .expect("controller database initializes");
    reset_database(&controller_state.pool).await;

    let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .expect("bind controller listener");
    let address = listener.local_addr().expect("controller address");
    let (server_shutdown_tx, server_shutdown_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = server_shutdown_rx.await;
            })
            .await
            .expect("controller server");
    });

    let temporary = tempfile::tempdir().expect("temporary agent directory");
    let config = AgentConfig {
        controller_url: format!("http://{address}/"),
        node_name: "agent-integration-node".to_owned(),
        device_type: "linux".to_owned(),
        state_directory: temporary.path().join("state"),
        runtime_directory: temporary.path().join("run"),
        interface_name: "xsn0".to_owned(),
        mtu: 1280,
        control_sync_interval_seconds: 5,
    };
    config.validate().expect("agent config validates");

    let client = Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("HTTP client");
    let network = post_admin(
        &client,
        &config.controller_url,
        "/v1/admin/networks",
        json!({
            "name": "agent-integration-network",
            "address_pool": "100.88.20.0/24",
            "reserved_addresses": 16
        }),
    )
    .await;
    let network_id = network["id"].as_str().expect("network id");
    let token = create_token(&client, &config.controller_url, network_id).await;
    let token_path = temporary.path().join("enrollment.token");
    write_private_token(&token_path, &token);

    let initial_state = enroll(&config, &token_path)
        .await
        .expect("agent enrollment succeeds");
    assert!(!token_path.exists());
    let initial_version = initial_state.configuration.version;
    let identity = Arc::new(
        Identity::load_or_create(&config.identity_path()).expect("load persisted identity"),
    );
    initial_state
        .validate(&identity, &config.controller_url)
        .expect("persisted state validates");

    let shared_state = Arc::new(RwLock::new(initial_state));
    let health = Arc::new(AgentHealth::new());
    let (candidate_tx, candidate_rx) = watch::channel(None);
    let (control_shutdown_tx, control_shutdown_rx) = watch::channel(false);
    let control = tokio::spawn(run_control_loop(
        config.clone(),
        Arc::clone(&identity),
        Arc::clone(&shared_state),
        Arc::clone(&health),
        candidate_rx,
        control_shutdown_rx,
    ));
    wait_until_connected(&health).await;

    enroll_peer(&client, &config.controller_url, network_id).await;
    wait_for_new_configuration(&shared_state, initial_version).await;
    assert_updated_state(&config, &identity, &shared_state).await;

    advertise_local_candidate(&candidate_tx, &shared_state).await;

    control_shutdown_tx
        .send(true)
        .expect("request control shutdown");
    tokio::time::timeout(Duration::from_secs(3), control)
        .await
        .expect("control task stops")
        .expect("control task joins");
    let _ = server_shutdown_tx.send(());
    server.await.expect("server joins");
    controller_state.pool.close().await;
}

async fn advertise_local_candidate(
    candidate_tx: &watch::Sender<Option<CandidateAdvertisement>>,
    shared_state: &RwLock<NodeState>,
) {
    let now = Utc::now();
    let local_state = shared_state.read().await.clone();
    let advertisement = CandidateAdvertisement {
        schema_version: 1,
        network_id: local_state.network_id,
        node_id_base64: local_state.node_id_base64.clone(),
        generation: local_state.candidate_generation.saturating_add(1),
        generated_at: now,
        expires_at: now + chrono::Duration::minutes(10),
        candidates: vec![EndpointCandidate {
            kind: EndpointCandidateKind::Mapped,
            endpoint: "198.51.100.42:42001".parse().expect("candidate endpoint"),
            priority: 100,
            expires_at: now + chrono::Duration::minutes(10),
        }],
    };
    candidate_tx
        .send(Some(advertisement))
        .expect("publish candidate advertisement");
    wait_for_new_configuration(shared_state, local_state.configuration.version).await;
    assert!(
        shared_state
            .read()
            .await
            .configuration_payload
            .nodes
            .iter()
            .find(|node| node.node_id_base64 == local_state.node_id_base64)
            .expect("local node in configuration")
            .candidates
            .iter()
            .any(|candidate| candidate.endpoint.to_string() == "198.51.100.42:42001")
    );
}

fn controller_config() -> ControllerConfig {
    ControllerConfig {
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        discovery_listen: None,
        discovery_public_endpoint: None,
        database_url: std::env::var("XS_TEST_DATABASE_URL")
            .expect("XS_TEST_DATABASE_URL is required"),
        database_schema: std::env::var("XS_TEST_AGENT_DATABASE_SCHEMA")
            .unwrap_or_else(|_| "xs_nexus_agent_test".to_owned()),
        admin_token_hash: Sha256::digest(ADMIN_TOKEN.as_bytes()).into(),
        credential_signing_key: SigningKey::from_bytes(&[21_u8; 32]),
        config_signing_key: SigningKey::from_bytes(&[22_u8; 32]),
        credential_ttl_seconds: 86_400,
        relays: Vec::new(),
    }
}

async fn reset_database(pool: &sqlx::PgPool) {
    sqlx::query(
        "TRUNCATE audit_events, configuration_versions, ip_leases, nodes,
                  enrollment_tokens, networks
         RESTART IDENTITY CASCADE",
    )
    .execute(pool)
    .await
    .expect("truncate agent test tables");
    sqlx::query("ALTER SEQUENCE credential_serial RESTART WITH 1")
        .execute(pool)
        .await
        .expect("reset credential serial");
}

async fn post_admin(client: &Client, base: &str, path: &str, body: Value) -> Value {
    let response = client
        .post(format!("{base}{}", path.trim_start_matches('/')))
        .bearer_auth(ADMIN_TOKEN)
        .json(&body)
        .send()
        .await
        .expect("admin request");
    assert_eq!(response.status(), StatusCode::CREATED);
    response.json().await.expect("admin response JSON")
}

async fn create_token(client: &Client, base: &str, network_id: &str) -> String {
    post_admin(
        client,
        base,
        "/v1/admin/enrollment-tokens",
        json!({
            "network_id": network_id,
            "expires_in_seconds": 3600,
            "max_uses": 1,
            "default_role_bitmap": 1,
            "default_tags": ["linux"],
            "requested_virtual_ip": null
        }),
    )
    .await["token"]
        .as_str()
        .expect("enrollment token")
        .to_owned()
}

async fn enroll_peer(client: &Client, base: &str, network_id: &str) {
    let token = create_token(client, base, network_id).await;
    let identity = SigningKey::from_bytes(&[31_u8; 32]);
    let response = client
        .post(format!("{base}v1/enroll"))
        .json(&json!({
            "token": token,
            "name": "agent-integration-peer",
            "device_type": "linux",
            "identity_public_key_base64": URL_SAFE_NO_PAD.encode(
                identity.verifying_key().to_bytes()
            )
        }))
        .send()
        .await
        .expect("enroll second node");
    assert_eq!(response.status(), StatusCode::CREATED);
}

async fn assert_updated_state(
    config: &AgentConfig,
    identity: &Identity,
    shared_state: &RwLock<NodeState>,
) {
    let updated = shared_state.read().await.clone();
    assert_eq!(updated.configuration_payload.nodes.len(), 2);
    assert!(
        updated
            .configuration_payload
            .nodes
            .iter()
            .any(|node| node.virtual_ip == Ipv4Addr::new(100, 88, 20, 17).to_string())
    );
    let persisted: NodeState = read_json(&config.node_state_path()).expect("read agent state");
    assert_eq!(
        persisted.configuration.version,
        updated.configuration.version
    );
    persisted
        .validate(identity, &config.controller_url)
        .expect("updated state validates");
}

fn write_private_token(path: &Path, token: &str) {
    use std::io::Write as _;

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .expect("create enrollment token file");
    file.write_all(token.as_bytes())
        .expect("write enrollment token");
    file.sync_all().expect("sync enrollment token");
}

async fn wait_until_connected(health: &AgentHealth) {
    tokio::time::timeout(Duration::from_secs(8), async {
        while !health.controller_connected() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("Agent connects to Controller");
}

async fn wait_for_new_configuration(state: &RwLock<NodeState>, initial_version: u64) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if state.read().await.configuration.version > initial_version {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("Agent applies a newer Controller configuration");
}
