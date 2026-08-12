use std::{
    net::{Ipv4Addr, SocketAddr},
    str::FromStr,
    time::Duration,
};

use axum::{
    Router,
    body::Body,
    http::{Method, Request, StatusCode, header},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use ed25519_dalek::{Signature, Signer, SigningKey};
use futures_util::{SinkExt, StreamExt};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::Connection;
use tokio::{
    net::{TcpStream, UdpSocket},
    sync::watch,
    time::timeout,
};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};
use tower::ServiceExt;
use uuid::Uuid;
use xs_controller::config::{ControllerConfig, MigrationConfig};
use xs_core::{
    AgentPathKind, AgentPeerTelemetry, AgentRuntimeReport, AgentTelemetryReport, AgentUpdateState,
    CandidateAdvertisement, ConfigurationRelay, EndpointCandidate, EndpointCandidateKind,
    RelayTelemetryMetrics, RelayTelemetryReport, SignedRelayTelemetryReport,
    SubnetRouteAdvertisement, SubnetRouteSuggestion, UpdateChannel,
    agent_runtime_report_signing_input, agent_telemetry_report_signing_input,
    relay_telemetry_report_signing_input,
};
use xs_protocol::{
    CREDENTIAL_LENGTH, DiscoveryRequest, verify_credential, verify_discovery_response,
};
use zeroize::Zeroizing;

const ADMIN_TOKEN: &str = "integration-admin-token-with-32-characters";
const CONSOLE_PASSWORD: &str = "integration-console-password-42";
const CONFIGURATION_DOMAIN: &[u8] = b"XS Nexus configuration v1";
const CONTROL_AUTHENTICATION_DOMAIN: &[u8] = b"XS Nexus control authentication v1";
const CANDIDATE_ADVERTISEMENT_DOMAIN: &[u8] = b"XS Nexus candidate advertisement v1";
const SUBNET_ROUTE_ADVERTISEMENT_DOMAIN: &[u8] = b"XS Nexus subnet route advertisement v1";
type ControlSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn controller_registration_ipam_configuration_and_control_flow() {
    let discovery_socket = UdpSocket::bind(("127.0.0.1", 0))
        .await
        .expect("bind discovery socket");
    let discovery_address = discovery_socket.local_addr().expect("discovery address");
    let config = test_config(discovery_address);
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
    assert_console_authentication_and_permissions(&router, &state.pool).await;
    let (discovery_shutdown, discovery_shutdown_rx) = watch::channel(false);
    let discovery_state = state.clone();
    let discovery_server = tokio::spawn(async move {
        xs_controller::discovery::serve(discovery_socket, discovery_state, discovery_shutdown_rx)
            .await
    });

    let (status, ready) = request_json(&router, Method::GET, "/health/ready", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ready["database"], "ok");
    let (status, version) = request_json(&router, Method::GET, "/v1/version", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(version["product"], "xs-nexus");
    assert_eq!(version["component"], "xs-controller");
    assert_eq!(version["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(version["commit"], xs_core::BUILD_GIT_COMMIT);
    assert_eq!(version["protocol_version"], "XSP/1");
    assert_relay_telemetry(&router, &state.pool).await;

    let network_request = json!({
        "name": "integration-network",
        "address_pool": "100.88.0.0/24",
        "reserved_addresses": 16
    });
    let (status, _) = request_json(
        &router,
        Method::POST,
        "/v1/admin/networks",
        Some(network_request.clone()),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, network) = request_json(
        &router,
        Method::POST,
        "/v1/admin/networks",
        Some(network_request),
        Some(ADMIN_TOKEN),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(network["config_version"], 1);
    let network_id = network["id"].as_str().expect("network id");
    assert_network_persistence_guards(&router, &state.pool, &config, network_id).await;

    let (token_id, token) = create_token(&router, network_id, 1).await;
    assert_token_is_hash_only(&state.pool, &config.database_schema).await;

    let identity = SigningKey::from_bytes(&[11_u8; 32]);
    let enrollment = enroll(
        &router,
        &token,
        "node-a",
        &identity.verifying_key().to_bytes(),
    )
    .await;
    assert_eq!(enrollment.0, StatusCode::CREATED);
    let enrollment = enrollment.1;
    assert_eq!(enrollment["virtual_ip"], "100.88.0.16");
    verify_enrollment_artifacts(&state, &enrollment, discovery_address);
    verify_udp_discovery(&state, &enrollment, &identity, discovery_address).await;

    let (_, duplicate_token) = create_token(&router, network_id, 1).await;
    let duplicate = enroll(
        &router,
        &duplicate_token,
        "node-a-duplicate",
        &identity.verifying_key().to_bytes(),
    )
    .await;
    assert_eq!(duplicate.0, StatusCode::CONFLICT);
    assert_eq!(duplicate.1["error"]["code"], "resource_conflict");

    assert_expired_token_rejected(&router, &state.pool, network_id).await;
    assert_concurrent_token_and_ipam(&router, &state.pool, network_id).await;
    let manual_enrollment = assert_manual_ip_assignment(&router, network_id).await;

    assert_audit_is_append_only_and_redacted(&state.pool, &token).await;
    verify_websocket_control(
        &router,
        &state.pool,
        &enrollment,
        &identity,
        discovery_address,
    )
    .await;
    assert_acl_policy_explain_revoke_and_cooldown(
        &router,
        &state.pool,
        network_id,
        &enrollment,
        &manual_enrollment,
    )
    .await;
    assert_subnet_route_approval_lifecycle(
        &router,
        &state.pool,
        network_id,
        &enrollment,
        &identity,
    )
    .await;
    assert_node_update_channel_and_console(&router, &state.pool, network_id, &enrollment).await;

    assert_control_audit_and_token_use(&state.pool, &token_id).await;

    discovery_shutdown
        .send(true)
        .expect("request discovery shutdown");
    discovery_server
        .await
        .expect("join discovery server")
        .expect("discovery server exits cleanly");
}

async fn assert_network_persistence_guards(
    router: &Router,
    pool: &sqlx::PgPool,
    config: &ControllerConfig,
    network_id: &str,
) {
    assert_overlapping_network_rejected(router).await;
    assert_database_pool_recovers_after_backend_termination(router, pool, config, network_id).await;
    assert_update_release_and_rollout(router, pool, network_id).await;
}

async fn assert_update_release_and_rollout(router: &Router, pool: &sqlx::PgPool, network_id: &str) {
    let fixture = update_release_fixture();
    let release_id = assert_update_release_import(router, &fixture).await;
    assert_update_policy_lifecycle(router, pool, network_id, &release_id, &fixture).await;
    assert_update_release_revocation(router, pool, network_id, &release_id).await;
}

struct UpdateReleaseFixture {
    manifest: &'static str,
    signature: [u8; 64],
    request: Value,
}

fn update_release_fixture() -> UpdateReleaseFixture {
    let manifest = concat!(
        "schema_version=2\n",
        "product=xs-nexus\n",
        "version=0.2.0\n",
        "source_commit=0123456789abcdef0123456789abcdef01234567\n",
        "source_date_epoch=1700000000\n",
        "protocol_version=XSP/1\n",
        "platform=linux\n",
        "architecture=x86_64\n",
        "target=x86_64-unknown-linux-gnu\n",
        "archive=xs-nexus-0.2.0-x86_64-unknown-linux-gnu.tar.gz\n",
        "archive_size=4096\n",
        "archive_sha256=0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\n",
    );
    let signing_key = SigningKey::from_bytes(&[23_u8; 32]);
    let signature = signing_key.sign(manifest.as_bytes()).to_bytes();
    let request = json!({
        "manifest_base64": URL_SAFE_NO_PAD.encode(manifest),
        "signature_base64": URL_SAFE_NO_PAD.encode(signature),
        "archive_url": "https://updates.example.test/linux/xs-nexus-0.2.0-x86_64-unknown-linux-gnu.tar.gz"
    });
    UpdateReleaseFixture {
        manifest,
        signature,
        request,
    }
}

async fn assert_update_release_import(router: &Router, fixture: &UpdateReleaseFixture) -> String {
    let (unauthorized_status, _) = request_json(
        router,
        Method::POST,
        "/v1/admin/update-releases",
        Some(fixture.request.clone()),
        None,
    )
    .await;
    assert_eq!(unauthorized_status, StatusCode::UNAUTHORIZED);

    let mut invalid_signature = fixture.request.clone();
    invalid_signature["signature_base64"] = json!(URL_SAFE_NO_PAD.encode([0_u8; 64]));
    let (invalid_status, invalid_body) = request_json(
        router,
        Method::POST,
        "/v1/admin/update-releases",
        Some(invalid_signature),
        Some(ADMIN_TOKEN),
    )
    .await;
    assert_eq!(invalid_status, StatusCode::BAD_REQUEST);
    assert_eq!(invalid_body["error"]["code"], "invalid_request");

    let mut wrong_url = fixture.request.clone();
    wrong_url["archive_url"] = json!("https://updates.example.test/linux/wrong.tar.gz");
    let (wrong_url_status, _) = request_json(
        router,
        Method::POST,
        "/v1/admin/update-releases",
        Some(wrong_url),
        Some(ADMIN_TOKEN),
    )
    .await;
    assert_eq!(wrong_url_status, StatusCode::BAD_REQUEST);

    let (created_status, release) = request_json(
        router,
        Method::POST,
        "/v1/admin/update-releases",
        Some(fixture.request.clone()),
        Some(ADMIN_TOKEN),
    )
    .await;
    assert_eq!(created_status, StatusCode::CREATED);
    assert_eq!(release["version"], "0.2.0");
    assert_eq!(release["platform"], "linux");
    assert_eq!(release["architecture"], "x86_64");
    assert_eq!(release["archive_size"], 4096);
    assert!(release["revoked_at"].is_null());
    assert!(release["revocation_reason"].is_null());
    assert!(release.get("manifest_base64").is_none());
    assert!(release.get("signature_base64").is_none());
    let release_id = release["id"].as_str().expect("release id").to_owned();

    let (duplicate_status, duplicate_body) = request_json(
        router,
        Method::POST,
        "/v1/admin/update-releases",
        Some(fixture.request.clone()),
        Some(ADMIN_TOKEN),
    )
    .await;
    assert_eq!(duplicate_status, StatusCode::CONFLICT);
    assert_eq!(duplicate_body["error"]["code"], "resource_conflict");

    let (list_status, releases) = request_json(
        router,
        Method::GET,
        "/v1/admin/update-releases",
        None,
        Some(ADMIN_TOKEN),
    )
    .await;
    assert_eq!(list_status, StatusCode::OK);
    assert_eq!(releases.as_array().map(Vec::len), Some(1));
    release_id
}

async fn assert_update_policy_lifecycle(
    router: &Router,
    pool: &sqlx::PgPool,
    network_id: &str,
    release_id: &str,
    fixture: &UpdateReleaseFixture,
) {
    let policy_path =
        format!("/v1/admin/networks/{network_id}/update-policies/stable/linux/x86_64");
    let initial_policy = json!({
        "expected_generation": 0,
        "release_id": release_id,
        "minimum_version": "0.1.0",
        "rollout_basis_points": 2500,
        "paused": false
    });
    let (policy_status, policy) = request_json(
        router,
        Method::PUT,
        &policy_path,
        Some(initial_policy.clone()),
        Some(ADMIN_TOKEN),
    )
    .await;
    assert_eq!(policy_status, StatusCode::OK);
    assert_eq!(policy["generation"], 1);
    assert_eq!(policy["channel"], "stable");
    assert_eq!(policy["rollout_basis_points"], 2500);
    assert_eq!(policy["paused"], false);

    let (stale_status, stale_body) = request_json(
        router,
        Method::PUT,
        &policy_path,
        Some(initial_policy),
        Some(ADMIN_TOKEN),
    )
    .await;
    assert_eq!(stale_status, StatusCode::CONFLICT);
    assert_eq!(stale_body["error"]["code"], "resource_conflict");

    let (invalid_minimum_status, _) = request_json(
        router,
        Method::PUT,
        &policy_path,
        Some(json!({
            "expected_generation": 1,
            "release_id": release_id,
            "minimum_version": "0.3.0",
            "rollout_basis_points": 2500,
            "paused": false
        })),
        Some(ADMIN_TOKEN),
    )
    .await;
    assert_eq!(invalid_minimum_status, StatusCode::BAD_REQUEST);

    let (paused_status, paused) = request_json(
        router,
        Method::PUT,
        &policy_path,
        Some(json!({
            "expected_generation": 1,
            "release_id": release_id,
            "minimum_version": "0.1.0",
            "rollout_basis_points": 2500,
            "paused": true
        })),
        Some(ADMIN_TOKEN),
    )
    .await;
    assert_eq!(paused_status, StatusCode::OK);
    assert_eq!(paused["generation"], 2);
    assert_eq!(paused["paused"], true);

    let policy_list_path = format!("/v1/admin/networks/{network_id}/update-policies");
    let (policies_status, policies) = request_json(
        router,
        Method::GET,
        &policy_list_path,
        None,
        Some(ADMIN_TOKEN),
    )
    .await;
    assert_eq!(policies_status, StatusCode::OK);
    assert_eq!(policies.as_array().map(Vec::len), Some(1));
    assert_eq!(policies[0]["generation"], 2);

    assert_update_persistence_and_audit(pool, fixture).await;
}

async fn assert_update_persistence_and_audit(pool: &sqlx::PgPool, fixture: &UpdateReleaseFixture) {
    let stored: (Vec<u8>, Vec<u8>, i32, bool) = sqlx::query_as(
        "SELECT r.manifest, r.signature, p.rollout_basis_points, p.paused
         FROM update_releases r
         JOIN update_rollout_policies p ON p.release_id = r.id",
    )
    .fetch_one(pool)
    .await
    .expect("stored signed rollout");
    assert_eq!(stored.0, fixture.manifest.as_bytes());
    assert_eq!(stored.1, fixture.signature);
    assert_eq!(stored.2, 2500);
    assert!(stored.3);

    let audit_metadata: Vec<String> = sqlx::query_scalar(
        "SELECT metadata::text FROM audit_events
         WHERE action IN ('update.release.create', 'update.policy.replace')
         ORDER BY occurred_at",
    )
    .fetch_all(pool)
    .await
    .expect("update audit metadata");
    assert_eq!(audit_metadata.len(), 3);
    for metadata in audit_metadata {
        assert!(!metadata.contains(&URL_SAFE_NO_PAD.encode(fixture.manifest)));
        assert!(!metadata.contains(&URL_SAFE_NO_PAD.encode(fixture.signature)));
        assert!(!metadata.contains("updates.example.test"));
    }
}

async fn assert_update_release_revocation(
    router: &Router,
    pool: &sqlx::PgPool,
    network_id: &str,
    release_id: &str,
) {
    let path = format!("/v1/admin/update-releases/{release_id}/revoke");
    let (unauthorized_status, _) = request_json(
        router,
        Method::POST,
        &path,
        Some(json!({ "reason": "security_issue" })),
        None,
    )
    .await;
    assert_eq!(unauthorized_status, StatusCode::UNAUTHORIZED);

    let (invalid_status, _) = request_json(
        router,
        Method::POST,
        &path,
        Some(json!({ "reason": "free-form reason" })),
        Some(ADMIN_TOKEN),
    )
    .await;
    assert_eq!(invalid_status, StatusCode::BAD_REQUEST);

    let (revoked_status, revoked) = request_json(
        router,
        Method::POST,
        &path,
        Some(json!({ "reason": "security_issue" })),
        Some(ADMIN_TOKEN),
    )
    .await;
    assert_eq!(revoked_status, StatusCode::OK);
    assert_eq!(revoked["id"], release_id);
    assert_eq!(revoked["revocation_reason"], "security_issue");
    assert!(revoked["revoked_at"].is_string());

    let (duplicate_status, _) = request_json(
        router,
        Method::POST,
        &path,
        Some(json!({ "reason": "withdrawn" })),
        Some(ADMIN_TOKEN),
    )
    .await;
    assert_eq!(duplicate_status, StatusCode::CONFLICT);

    let policy_path =
        format!("/v1/admin/networks/{network_id}/update-policies/stable/linux/x86_64");
    let (policy_status, _) = request_json(
        router,
        Method::PUT,
        &policy_path,
        Some(json!({
            "expected_generation": 3,
            "release_id": release_id,
            "minimum_version": "0.1.0",
            "rollout_basis_points": 10000,
            "paused": false
        })),
        Some(ADMIN_TOKEN),
    )
    .await;
    assert_eq!(policy_status, StatusCode::BAD_REQUEST);

    let stored: (bool, String, bool, i64) = sqlx::query_as(
        "SELECT r.revoked_at IS NOT NULL, r.revocation_reason, p.paused, p.generation
         FROM update_releases r
         JOIN update_rollout_policies p ON p.release_id = r.id
         WHERE r.id = $1",
    )
    .bind(Uuid::parse_str(release_id).expect("release id"))
    .fetch_one(pool)
    .await
    .expect("revoked release state");
    assert_eq!(stored, (true, "security_issue".to_owned(), true, 3));

    let audit_metadata: String = sqlx::query_scalar(
        "SELECT metadata::text FROM audit_events
         WHERE action = 'update.release.revoke' AND target_id = $1",
    )
    .bind(release_id)
    .fetch_one(pool)
    .await
    .expect("release revocation audit");
    assert!(audit_metadata.contains("security_issue"));
    assert!(!audit_metadata.contains("signature"));
    assert!(!audit_metadata.contains("updates.example.test"));
}

async fn assert_database_pool_recovers_after_backend_termination(
    router: &Router,
    pool: &sqlx::PgPool,
    config: &ControllerConfig,
    network_id: &str,
) {
    let mut target = pool.acquire().await.expect("acquire target connection");
    let terminated_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *target)
        .await
        .expect("read target backend pid");

    let mut terminator = sqlx::postgres::PgConnection::connect(&config.database_url)
        .await
        .expect("connect independent database terminator");
    let terminated: bool = sqlx::query_scalar("SELECT pg_terminate_backend($1)")
        .bind(terminated_pid)
        .fetch_one(&mut terminator)
        .await
        .expect("terminate only the borrowed test-pool backend");
    assert!(terminated, "PostgreSQL accepted backend termination");

    let target_failure = sqlx::query_scalar::<_, i32>("SELECT pg_backend_pid()")
        .fetch_one(&mut *target)
        .await;
    assert!(
        target_failure.is_err(),
        "the explicitly terminated pool connection must become unusable"
    );
    drop(target);

    let recovery = timeout(Duration::from_secs(5), async {
        loop {
            let (ready_status, ready) =
                request_json(router, Method::GET, "/health/ready", None, None).await;
            let (networks_status, networks) = request_json(
                router,
                Method::GET,
                "/v1/admin/networks",
                None,
                Some(ADMIN_TOKEN),
            )
            .await;
            let network_preserved = networks.as_array().is_some_and(|items| {
                items
                    .iter()
                    .any(|network| network["id"].as_str() == Some(network_id))
            });
            if ready_status == StatusCode::OK
                && ready["database"] == "ok"
                && networks_status == StatusCode::OK
                && network_preserved
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;
    assert!(
        recovery.is_ok(),
        "database pool did not recover within 5 seconds"
    );

    let replacement_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(pool)
        .await
        .expect("query through recovered pool");
    assert_ne!(
        replacement_pid, terminated_pid,
        "the terminated PostgreSQL backend must not be reused"
    );
}

async fn assert_overlapping_network_rejected(router: &Router) {
    let (status, response) = request_json(
        router,
        Method::POST,
        "/v1/admin/networks",
        Some(json!({
            "name": "overlapping-network",
            "address_pool": "100.88.0.128/25",
            "reserved_addresses": 16
        })),
        Some(ADMIN_TOKEN),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(response["error"]["code"], "resource_conflict");
}

async fn assert_expired_token_rejected(router: &Router, pool: &sqlx::PgPool, network_id: &str) {
    let (expired_id, expired_token) = create_token(router, network_id, 1).await;
    sqlx::query(
        "UPDATE enrollment_tokens SET expires_at = now() - interval '1 second' WHERE id = $1",
    )
    .bind(uuid::Uuid::from_str(&expired_id).expect("expired token id"))
    .execute(pool)
    .await
    .expect("expire token");
    let public_key = SigningKey::from_bytes(&[12_u8; 32])
        .verifying_key()
        .to_bytes();
    let expired = enroll(router, &expired_token, "node-expired", &public_key).await;
    assert_eq!(expired.0, StatusCode::UNAUTHORIZED);
}

async fn assert_concurrent_token_and_ipam(router: &Router, pool: &sqlx::PgPool, network_id: &str) {
    let (_, concurrent_token) = create_token(router, network_id, 1).await;
    let first_public_key = SigningKey::from_bytes(&[13_u8; 32])
        .verifying_key()
        .to_bytes();
    let second_public_key = SigningKey::from_bytes(&[14_u8; 32])
        .verifying_key()
        .to_bytes();
    let first = enroll(
        router,
        &concurrent_token,
        "node-concurrent-a",
        &first_public_key,
    );
    let second = enroll(
        router,
        &concurrent_token,
        "node-concurrent-b",
        &second_public_key,
    );
    let (first, second) = tokio::join!(first, second);
    let statuses = [first.0, second.0];
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == StatusCode::CREATED)
            .count(),
        1
    );
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == StatusCode::UNAUTHORIZED)
            .count(),
        1
    );

    let distinct_addresses: i64 =
        sqlx::query_scalar("SELECT count(DISTINCT virtual_ip) FROM nodes WHERE revoked_at IS NULL")
            .fetch_one(pool)
            .await
            .expect("count addresses");
    let active_nodes: i64 =
        sqlx::query_scalar("SELECT count(*) FROM nodes WHERE revoked_at IS NULL")
            .fetch_one(pool)
            .await
            .expect("count nodes");
    assert_eq!(distinct_addresses, active_nodes);
    assert_eq!(active_nodes, 2);

    let config_version: i64 =
        sqlx::query_scalar("SELECT config_version FROM networks WHERE id = $1")
            .bind(uuid::Uuid::from_str(network_id).expect("network id"))
            .fetch_one(pool)
            .await
            .expect("config version");
    assert_eq!(config_version, 3);
}

async fn assert_manual_ip_assignment(router: &Router, network_id: &str) -> Value {
    let (status, response) = request_json(
        router,
        Method::POST,
        "/v1/admin/enrollment-tokens",
        Some(json!({
            "network_id": network_id,
            "expires_in_seconds": 3600,
            "max_uses": 1,
            "default_role_bitmap": 1,
            "default_tags": ["linux"],
            "requested_virtual_ip": "100.88.0.30"
        })),
        Some(ADMIN_TOKEN),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let token = response["token"].as_str().expect("manual IP token");
    let public_key = SigningKey::from_bytes(&[15_u8; 32])
        .verifying_key()
        .to_bytes();
    let enrollment = enroll(router, token, "node-manual-ip", &public_key).await;
    assert_eq!(enrollment.0, StatusCode::CREATED);
    assert_eq!(enrollment.1["virtual_ip"], "100.88.0.30");
    assert_eq!(enrollment.1["configuration"]["version"], 4);
    enrollment.1
}

async fn assert_acl_policy_explain_revoke_and_cooldown(
    router: &Router,
    pool: &sqlx::PgPool,
    network_id: &str,
    client: &Value,
    server: &Value,
) {
    let client_node_id = client["node_id_base64"].as_str().expect("client node id");
    let server_node_id = server["node_id_base64"].as_str().expect("server node id");
    replace_acl_policy(router, network_id, client_node_id, server_node_id).await;
    assert_acl_explanations_and_payload(router, pool, network_id, client_node_id, server_node_id)
        .await;
    assert_revoke_and_cooldown_reuse(router, pool, network_id, server_node_id).await;
}

async fn replace_acl_policy(
    router: &Router,
    network_id: &str,
    client_node_id: &str,
    server_node_id: &str,
) {
    let path = format!("/v1/admin/networks/{network_id}/acl");
    let policy = json!({
        "expected_policy_version": 1,
        "groups": [
            {"name": "clients", "node_ids_base64": [client_node_id]},
            {"name": "servers", "node_ids_base64": [server_node_id]}
        ],
        "rules": [
            {
                "id": "allow-https",
                "priority": 100,
                "action": "allow",
                "sources": [{"type": "group", "name": "clients"}],
                "destinations": [{"type": "group", "name": "servers"}],
                "protocol": "tcp",
                "destination_ports": [{"start": 443, "end": 443}]
            },
            {
                "id": "allow-ping",
                "priority": 300,
                "action": "allow",
                "sources": [{"type": "node", "node_id_base64": client_node_id}],
                "destinations": [{"type": "node", "node_id_base64": server_node_id}],
                "protocol": "icmp"
            },
            {
                "id": "deny-postgresql",
                "priority": 200,
                "action": "deny",
                "sources": [{"type": "tag", "name": "linux"}],
                "destinations": [{"type": "group", "name": "servers"}],
                "protocol": "tcp",
                "destination_ports": [{"start": 5432, "end": 5432}]
            }
        ]
    });
    let (status, unauthorized) =
        request_json(router, Method::PUT, &path, Some(policy.clone()), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(unauthorized["error"]["code"], "unauthorized");

    let (status, replaced) = request_json(
        router,
        Method::PUT,
        &path,
        Some(policy.clone()),
        Some(ADMIN_TOKEN),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replaced["policy_version"], 2);
    assert_eq!(replaced["configuration_version"], 6);

    let (status, stale) =
        request_json(router, Method::PUT, &path, Some(policy), Some(ADMIN_TOKEN)).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(stale["error"]["code"], "resource_conflict");
}

async fn assert_acl_explanations_and_payload(
    router: &Router,
    pool: &sqlx::PgPool,
    network_id: &str,
    client_node_id: &str,
    server_node_id: &str,
) {
    assert_acl_explain(
        router,
        network_id,
        client_node_id,
        server_node_id,
        "tcp",
        Some(443),
        true,
        Some("allow-https"),
    )
    .await;
    assert_acl_explain(
        router,
        network_id,
        client_node_id,
        server_node_id,
        "tcp",
        Some(5432),
        false,
        Some("deny-postgresql"),
    )
    .await;
    assert_acl_explain(
        router,
        network_id,
        client_node_id,
        server_node_id,
        "udp",
        Some(53),
        false,
        None,
    )
    .await;

    let latest_payload: Vec<u8> = sqlx::query_scalar(
        "SELECT payload FROM configuration_versions
         WHERE network_id = $1 ORDER BY version DESC LIMIT 1",
    )
    .bind(Uuid::from_str(network_id).expect("network id"))
    .fetch_one(pool)
    .await
    .expect("latest configuration payload");
    let latest_payload: Value =
        serde_json::from_slice(&latest_payload).expect("configuration payload JSON");
    assert_eq!(latest_payload["policy_version"], 2);
    assert_eq!(latest_payload["policies"][0]["id"], "allow-ping");
    let server_entry = latest_payload["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .find(|node| node["node_id_base64"] == server_node_id)
        .expect("server node");
    assert_eq!(server_entry["groups"][0], "servers");
}

async fn assert_revoke_and_cooldown_reuse(
    router: &Router,
    pool: &sqlx::PgPool,
    network_id: &str,
    server_node_id: &str,
) {
    let revoke_path = format!("/v1/admin/networks/{network_id}/nodes/{server_node_id}/revoke");
    let (status, revoked) = request_json(
        router,
        Method::POST,
        &revoke_path,
        Some(json!({"ip_cooldown_seconds": 60})),
        Some(ADMIN_TOKEN),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(revoked["virtual_ip"], "100.88.0.30");
    assert_eq!(revoked["configuration_version"], 7);

    let (status, cooling) = request_json(
        router,
        Method::POST,
        "/v1/admin/enrollment-tokens",
        Some(json!({
            "network_id": network_id,
            "expires_in_seconds": 3600,
            "requested_virtual_ip": "100.88.0.30"
        })),
        Some(ADMIN_TOKEN),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(cooling["error"]["code"], "resource_conflict");

    sqlx::query(
        "UPDATE ip_leases
         SET cooldown_until = now() - interval '1 second'
         WHERE network_id = $1 AND virtual_ip = '100.88.0.30'::inet",
    )
    .bind(Uuid::from_str(network_id).expect("network id"))
    .execute(pool)
    .await
    .expect("expire address cooldown");
    let (status, token) = request_json(
        router,
        Method::POST,
        "/v1/admin/enrollment-tokens",
        Some(json!({
            "network_id": network_id,
            "expires_in_seconds": 3600,
            "requested_virtual_ip": "100.88.0.30"
        })),
        Some(ADMIN_TOKEN),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let replacement_identity = SigningKey::from_bytes(&[16_u8; 32]);
    let replacement = enroll(
        router,
        token["token"].as_str().expect("replacement token"),
        "node-replacement",
        &replacement_identity.verifying_key().to_bytes(),
    )
    .await;
    assert_eq!(replacement.0, StatusCode::CREATED);
    assert_eq!(replacement.1["virtual_ip"], "100.88.0.30");
    assert_eq!(replacement.1["configuration"]["version"], 8);

    let active_lease: bool = sqlx::query_scalar(
        "SELECT state = 'active' AND cooldown_until IS NULL
         FROM ip_leases
         WHERE network_id = $1 AND virtual_ip = '100.88.0.30'::inet",
    )
    .bind(Uuid::from_str(network_id).expect("network id"))
    .fetch_one(pool)
    .await
    .expect("reused active lease");
    assert!(active_lease);
}

async fn assert_subnet_route_approval_lifecycle(
    router: &Router,
    pool: &sqlx::PgPool,
    network_id: &str,
    enrollment: &Value,
    identity: &SigningKey,
) {
    advertise_subnet_route_suggestions(router, network_id, enrollment, identity).await;
    assert_subnet_route_rejections(router, network_id, enrollment).await;
    assert_subnet_route_transitions(router, pool, network_id, enrollment).await;
}

async fn advertise_subnet_route_suggestions(
    router: &Router,
    network_id: &str,
    enrollment: &Value,
    identity: &SigningKey,
) {
    let (mut socket, server) = authenticated_control_socket(router, enrollment, identity).await;
    let now = Utc::now();
    let advertisement = SubnetRouteAdvertisement {
        schema_version: 1,
        network_id: Uuid::from_str(network_id).expect("network id"),
        node_id_base64: enrollment["node_id_base64"]
            .as_str()
            .expect("gateway node id")
            .to_owned(),
        generation: 1,
        generated_at: now,
        expires_at: now + chrono::Duration::minutes(10),
        suggestions: vec![
            SubnetRouteSuggestion {
                prefix: "192.168.0.0/16".to_owned(),
                interface_name: "eth0".to_owned(),
            },
            SubnetRouteSuggestion {
                prefix: "192.168.50.0/24".to_owned(),
                interface_name: "eth0".to_owned(),
            },
        ],
    };
    let payload = serde_json::to_vec(&advertisement).expect("serialize route advertisement");
    let mut signing_input =
        Vec::with_capacity(SUBNET_ROUTE_ADVERTISEMENT_DOMAIN.len() + payload.len());
    signing_input.extend_from_slice(SUBNET_ROUTE_ADVERTISEMENT_DOMAIN);
    signing_input.extend_from_slice(&payload);
    let signature = identity.sign(&signing_input);
    socket
        .send(Message::Text(
            json!({
                "type": "advertise_subnet_routes",
                "advertisement": advertisement,
                "signature_base64": URL_SAFE_NO_PAD.encode(signature.to_bytes())
            })
            .to_string()
            .into(),
        ))
        .await
        .expect("send subnet route advertisement");
    let response = socket
        .next()
        .await
        .expect("route advertisement response")
        .expect("valid route advertisement response");
    let Message::Text(response) = response else {
        panic!("expected route advertisement text response");
    };
    let response: Value = serde_json::from_str(&response).expect("route response JSON");
    assert_eq!(response["type"], "configuration");
    assert_eq!(response["configuration"]["version"], 8);
    socket
        .close(None)
        .await
        .expect("close route control socket");
    server.abort();
}

async fn assert_subnet_route_rejections(router: &Router, network_id: &str, enrollment: &Value) {
    let suggestions_path = format!("/v1/admin/networks/{network_id}/subnet-route-suggestions");
    let (status, suggestions) = request_json(
        router,
        Method::GET,
        &suggestions_path,
        None,
        Some(ADMIN_TOKEN),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        suggestions[0]["suggestions"]
            .as_array()
            .expect("suggestions")
            .len(),
        2
    );

    let routes_path = format!("/v1/admin/networks/{network_id}/subnet-routes");
    let (status, unapproved) = request_json(
        router,
        Method::PUT,
        &routes_path,
        Some(json!({
            "expected_configuration_version": 8,
            "routes": [{
                "route_id": "unapproved",
                "gateway_node_id_base64": enrollment["node_id_base64"],
                "prefix": "192.168.60.0/24",
                "interface_name": "eth0",
                "mode": "routed",
                "priority": 100
            }]
        })),
        Some(ADMIN_TOKEN),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(unapproved["error"]["code"], "invalid_request");

    let (status, overlap) = request_json(
        router,
        Method::PUT,
        &routes_path,
        Some(json!({
            "expected_configuration_version": 8,
            "routes": [
                {
                    "route_id": "lan-broad",
                    "gateway_node_id_base64": enrollment["node_id_base64"],
                    "prefix": "192.168.0.0/16",
                    "interface_name": "eth0",
                    "mode": "routed",
                    "priority": 90
                },
                {
                    "route_id": "lan-primary",
                    "gateway_node_id_base64": enrollment["node_id_base64"],
                    "prefix": "192.168.50.0/24",
                    "interface_name": "eth0",
                    "mode": "routed",
                    "priority": 100
                }
            ]
        })),
        Some(ADMIN_TOKEN),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(overlap["error"]["code"], "invalid_request");
}

async fn assert_subnet_route_transitions(
    router: &Router,
    pool: &sqlx::PgPool,
    network_id: &str,
    enrollment: &Value,
) {
    let routes_path = format!("/v1/admin/networks/{network_id}/subnet-routes");
    let enabled = json!({
        "route_id": "lan-primary",
        "gateway_node_id_base64": enrollment["node_id_base64"],
        "prefix": "192.168.50.0/24",
        "interface_name": "eth0",
        "mode": "routed",
        "priority": 100,
        "enabled": true
    });
    let (status, approved) = request_json(
        router,
        Method::PUT,
        &routes_path,
        Some(json!({
            "expected_configuration_version": 8,
            "routes": [enabled.clone()]
        })),
        Some(ADMIN_TOKEN),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(approved["configuration_version"], 9);
    assert_eq!(approved["enabled_routes"], 1);
    assert_configuration_subnet_routes(pool, network_id, 1, "routed").await;

    let mut paused = enabled.clone();
    paused["enabled"] = Value::Bool(false);
    let (status, paused_response) = request_json(
        router,
        Method::PUT,
        &routes_path,
        Some(json!({
            "expected_configuration_version": 9,
            "routes": [paused]
        })),
        Some(ADMIN_TOKEN),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(paused_response["configuration_version"], 10);
    assert_eq!(paused_response["paused_routes"], 1);
    assert_configuration_subnet_routes(pool, network_id, 0, "routed").await;

    let mut nat_enabled = enabled;
    nat_enabled["mode"] = Value::String("nat".to_owned());
    let (status, resumed) = request_json(
        router,
        Method::PUT,
        &routes_path,
        Some(json!({
            "expected_configuration_version": 10,
            "routes": [nat_enabled]
        })),
        Some(ADMIN_TOKEN),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(resumed["configuration_version"], 11);
    assert_configuration_subnet_routes(pool, network_id, 1, "nat").await;

    let (status, revoked) = request_json(
        router,
        Method::PUT,
        &routes_path,
        Some(json!({
            "expected_configuration_version": 11,
            "routes": []
        })),
        Some(ADMIN_TOKEN),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(revoked["configuration_version"], 12);
    assert_configuration_subnet_routes(pool, network_id, 0, "nat").await;
    let stored_state: String = sqlx::query_scalar(
        "SELECT state FROM subnet_routes WHERE network_id = $1 AND route_id = 'lan-primary'",
    )
    .bind(Uuid::from_str(network_id).expect("network id"))
    .fetch_one(pool)
    .await
    .expect("stored route state");
    assert_eq!(stored_state, "revoked");
}

async fn authenticated_control_socket(
    router: &Router,
    enrollment: &Value,
    identity: &SigningKey,
) -> (ControlSocket, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind route test controller");
    let address = listener.local_addr().expect("route listener address");
    let application = router.clone();
    let server = tokio::spawn(async move {
        axum::serve(listener, application)
            .await
            .expect("serve route test controller");
    });
    let (mut socket, _) = connect_async(format!("ws://{address}/v1/control"))
        .await
        .expect("connect route control websocket");
    let challenge = socket
        .next()
        .await
        .expect("route challenge")
        .expect("valid route challenge");
    let Message::Text(challenge) = challenge else {
        panic!("expected route challenge text");
    };
    let challenge: Value = serde_json::from_str(&challenge).expect("route challenge JSON");
    let challenge = URL_SAFE_NO_PAD
        .decode(
            challenge["challenge_base64"]
                .as_str()
                .expect("route challenge value"),
        )
        .expect("decode route challenge");
    let node_id = URL_SAFE_NO_PAD
        .decode(enrollment["node_id_base64"].as_str().expect("node id"))
        .expect("decode node id");
    let mut authentication_input =
        Vec::with_capacity(CONTROL_AUTHENTICATION_DOMAIN.len() + challenge.len() + node_id.len());
    authentication_input.extend_from_slice(CONTROL_AUTHENTICATION_DOMAIN);
    authentication_input.extend_from_slice(&challenge);
    authentication_input.extend_from_slice(&node_id);
    let signature = identity.sign(&authentication_input);
    socket
        .send(Message::Text(
            json!({
                "type": "authenticate",
                "node_id_base64": enrollment["node_id_base64"],
                "credential_base64": enrollment["credential_base64"],
                "signature_base64": URL_SAFE_NO_PAD.encode(signature.to_bytes())
            })
            .to_string()
            .into(),
        ))
        .await
        .expect("authenticate route control socket");
    let authenticated = socket
        .next()
        .await
        .expect("route authenticated response")
        .expect("valid route authenticated response");
    let Message::Text(authenticated) = authenticated else {
        panic!("expected route authenticated text");
    };
    let authenticated: Value =
        serde_json::from_str(&authenticated).expect("route authenticated JSON");
    assert_eq!(authenticated["type"], "authenticated");
    assert_eq!(authenticated["configuration"]["version"], 8);
    (socket, server)
}

async fn assert_configuration_subnet_routes(
    pool: &sqlx::PgPool,
    network_id: &str,
    expected_count: usize,
    expected_mode: &str,
) {
    let payload: Vec<u8> = sqlx::query_scalar(
        "SELECT payload FROM configuration_versions
         WHERE network_id = $1 ORDER BY version DESC LIMIT 1",
    )
    .bind(Uuid::from_str(network_id).expect("network id"))
    .fetch_one(pool)
    .await
    .expect("latest route configuration");
    let payload: Value = serde_json::from_slice(&payload).expect("route configuration JSON");
    let routes = payload["subnet_routes"].as_array().expect("subnet routes");
    assert_eq!(routes.len(), expected_count);
    if let Some(route) = routes.first() {
        assert_eq!(route["prefix"], "192.168.50.0/24");
        assert_eq!(route["mode"], expected_mode);
    }
}

async fn assert_node_update_channel_and_console(
    router: &Router,
    pool: &sqlx::PgPool,
    network_id: &str,
    enrollment: &Value,
) {
    let node_id_base64 = enrollment["node_id_base64"].as_str().expect("node id");
    let path = format!("/v1/admin/networks/{network_id}/nodes/{node_id_base64}/update-channel");
    let request = json!({
        "expected_configuration_version": 12,
        "update_channel": "testing"
    });
    let (status, replaced) = request_json(
        router,
        Method::PUT,
        &path,
        Some(request.clone()),
        Some(ADMIN_TOKEN),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replaced["update_channel"], "testing");
    assert_eq!(replaced["configuration_version"], 13);

    let (status, conflict) =
        request_json(router, Method::PUT, &path, Some(request), Some(ADMIN_TOKEN)).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(conflict["error"]["code"], "resource_conflict");

    let payload: Vec<u8> = sqlx::query_scalar(
        "SELECT payload FROM configuration_versions
         WHERE network_id = $1 AND version = 13",
    )
    .bind(Uuid::from_str(network_id).expect("network id"))
    .fetch_one(pool)
    .await
    .expect("channel configuration payload");
    let payload: Value = serde_json::from_slice(&payload).expect("channel configuration JSON");
    let node = payload["nodes"]
        .as_array()
        .expect("configuration nodes")
        .iter()
        .find(|node| node["node_id_base64"] == node_id_base64)
        .expect("updated node");
    assert_eq!(node["update_channel"], "testing");

    let metadata: Value = sqlx::query_scalar(
        "SELECT metadata FROM audit_events
         WHERE action = 'node.update_channel.replace'
         ORDER BY id DESC LIMIT 1",
    )
    .fetch_one(pool)
    .await
    .expect("channel audit metadata");
    assert_eq!(metadata["node_id_base64"], node_id_base64);
    assert_eq!(metadata["update_channel"], "testing");
    assert_eq!(metadata["configuration_version"], 13);
    let encoded = serde_json::to_string(&metadata).expect("serialize audit metadata");
    assert!(!encoded.contains("signature"));
    assert!(!encoded.contains("private"));
    assert_console_snapshot(router).await;
}

async fn assert_control_audit_and_token_use(pool: &sqlx::PgPool, token_id: &str) {
    let control_audits: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_events WHERE action = 'control.authenticate'",
    )
    .fetch_one(pool)
    .await
    .expect("control audit count");
    assert_eq!(control_audits, 11);

    let stored_use_count: i32 =
        sqlx::query_scalar("SELECT use_count FROM enrollment_tokens WHERE id = $1")
            .bind(uuid::Uuid::from_str(token_id).expect("token id"))
            .fetch_one(pool)
            .await
            .expect("token use count");
    assert_eq!(stored_use_count, 1);
}

#[allow(clippy::too_many_arguments)]
async fn assert_acl_explain(
    router: &Router,
    network_id: &str,
    source_node_id: &str,
    destination_node_id: &str,
    protocol: &str,
    destination_port: Option<u16>,
    allowed: bool,
    matched_rule_id: Option<&str>,
) {
    let path = format!("/v1/admin/networks/{network_id}/acl/explain");
    let (status, response) = request_json(
        router,
        Method::POST,
        &path,
        Some(json!({
            "source_node_id_base64": source_node_id,
            "destination_node_id_base64": destination_node_id,
            "protocol": protocol,
            "destination_port": destination_port
        })),
        Some(ADMIN_TOKEN),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["policy_version"], 2);
    assert_eq!(response["decision"]["allowed"], allowed);
    assert_eq!(
        response["decision"]["matched_rule_id"].as_str(),
        matched_rule_id
    );
}

fn test_config(discovery_address: SocketAddr) -> ControllerConfig {
    let database_url =
        std::env::var("XS_TEST_DATABASE_URL").expect("XS_TEST_DATABASE_URL is required");
    let database_schema =
        std::env::var("XS_TEST_DATABASE_SCHEMA").unwrap_or_else(|_| "xs_nexus_test".to_owned());
    ControllerConfig {
        listen: SocketAddr::from(([127, 0, 0, 1], 0)),
        discovery_listen: Some(discovery_address),
        discovery_public_endpoint: Some(discovery_address),
        database_url,
        database_schema,
        database_expected_role: None,
        admin_token_hash: Sha256::digest(ADMIN_TOKEN.as_bytes()).into(),
        console_bootstrap_username: Some("admin".to_owned()),
        console_bootstrap_password: Some(Zeroizing::new(CONSOLE_PASSWORD.to_owned())),
        console_cookie_secure: true,
        console_session_ttl_seconds: 28_800,
        credential_signing_key: SigningKey::from_bytes(&[21_u8; 32]),
        config_signing_key: SigningKey::from_bytes(&[22_u8; 32]),
        update_signing_public_key: Some(SigningKey::from_bytes(&[23_u8; 32]).verifying_key()),
        linux_release_directory: None,
        windows_release_directory: None,
        credential_ttl_seconds: 86_400,
        max_nodes_per_network: 1000,
        max_control_sessions: 1000,
        configuration_send_concurrency: 64,
        relays: vec![ConfigurationRelay {
            relay_id_base64: URL_SAFE_NO_PAD.encode([41_u8; 16]),
            endpoint: "127.0.0.1:42001".parse().expect("Relay endpoint"),
            identity_public_key_base64: URL_SAFE_NO_PAD.encode(
                SigningKey::from_bytes(&[42_u8; 32])
                    .verifying_key()
                    .to_bytes(),
            ),
            priority: 100,
            expires_at: Utc::now() + chrono::Duration::hours(1),
        }],
    }
}

#[allow(clippy::too_many_lines)]
async fn assert_relay_telemetry(router: &Router, pool: &sqlx::PgPool) {
    let signing_key = SigningKey::from_bytes(&[42_u8; 32]);
    let relay_id = [41_u8; 16];
    let boot_id = [43_u8; 16];
    let generated_at = Utc::now();
    let report = RelayTelemetryReport {
        schema_version: 1,
        relay_id_base64: URL_SAFE_NO_PAD.encode(relay_id),
        boot_id_base64: URL_SAFE_NO_PAD.encode(boot_id),
        sequence: 1,
        generated_at,
        metrics: relay_metrics_fixture(),
    };
    let signed = sign_relay_report(report.clone(), &signing_key);
    let (status, response) = request_json(
        router,
        Method::POST,
        "/v1/relay-metrics",
        Some(serde_json::to_value(&signed).expect("Relay telemetry JSON")),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["accepted_sequence"], 1);

    let stored_sequence: i64 =
        sqlx::query_scalar("SELECT sequence FROM relay_telemetry_reports WHERE relay_id = $1")
            .bind(relay_id.as_slice())
            .fetch_one(pool)
            .await
            .expect("latest Relay telemetry");
    assert_eq!(stored_sequence, 1);
    let sample_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM relay_telemetry_samples WHERE relay_id = $1")
            .bind(relay_id.as_slice())
            .fetch_one(pool)
            .await
            .expect("Relay telemetry samples");
    assert_eq!(sample_count, 1);

    let (status, _) = request_json(
        router,
        Method::POST,
        "/v1/relay-metrics",
        Some(serde_json::to_value(&signed).expect("Relay telemetry JSON")),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    let mut invalid_signature = sign_relay_report(
        RelayTelemetryReport {
            sequence: 2,
            generated_at: generated_at + chrono::Duration::seconds(1),
            ..report.clone()
        },
        &signing_key,
    );
    invalid_signature.signature_base64 = URL_SAFE_NO_PAD.encode([0_u8; 64]);
    let (status, _) = request_json(
        router,
        Method::POST,
        "/v1/relay-metrics",
        Some(serde_json::to_value(invalid_signature).expect("Relay telemetry JSON")),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let mut rolled_back_metrics = report.metrics.clone();
    rolled_back_metrics.packets_received -= 1;
    let rollback = sign_relay_report(
        RelayTelemetryReport {
            sequence: 2,
            generated_at: generated_at + chrono::Duration::seconds(2),
            metrics: rolled_back_metrics,
            ..report.clone()
        },
        &signing_key,
    );
    let (status, _) = request_json(
        router,
        Method::POST,
        "/v1/relay-metrics",
        Some(serde_json::to_value(rollback).expect("Relay telemetry JSON")),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    let mut inconsistent_metrics = report.metrics.clone();
    inconsistent_metrics.packets_dropped = 2;
    let inconsistent = sign_relay_report(
        RelayTelemetryReport {
            sequence: 2,
            generated_at: generated_at + chrono::Duration::seconds(3),
            metrics: inconsistent_metrics,
            ..report.clone()
        },
        &signing_key,
    );
    let (status, _) = request_json(
        router,
        Method::POST,
        "/v1/relay-metrics",
        Some(serde_json::to_value(inconsistent).expect("Relay telemetry JSON")),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let stale = sign_relay_report(
        RelayTelemetryReport {
            sequence: 2,
            generated_at: generated_at - chrono::Duration::minutes(11),
            ..report.clone()
        },
        &signing_key,
    );
    let (status, _) = request_json(
        router,
        Method::POST,
        "/v1/relay-metrics",
        Some(serde_json::to_value(stale).expect("Relay telemetry JSON")),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let mut next_metrics = report.metrics.clone();
    next_metrics.packets_received += 10;
    next_metrics.bytes_received += 1_000;
    next_metrics.packets_forwarded += 8;
    next_metrics.bytes_forwarded += 800;
    next_metrics.forwarding_latency_samples += 8;
    next_metrics.forwarding_latency_microseconds_total += 400;
    let next = sign_relay_report(
        RelayTelemetryReport {
            sequence: 2,
            generated_at: generated_at + chrono::Duration::seconds(4),
            metrics: next_metrics,
            ..report
        },
        &signing_key,
    );
    let (status, response) = request_json(
        router,
        Method::POST,
        "/v1/relay-metrics",
        Some(serde_json::to_value(next).expect("Relay telemetry JSON")),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["accepted_sequence"], 2);

    let sample_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM relay_telemetry_samples WHERE relay_id = $1")
            .bind(relay_id.as_slice())
            .fetch_one(pool)
            .await
            .expect("Relay telemetry samples");
    assert_eq!(sample_count, 2);
    let detailed: (i64, i64, i64, i64, i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT registration_retries, registrations_rejected, invalid_drops,
                authentication_drops, replay_drops, rate_limit_drops, queue_drops,
                destination_drops, send_drops
         FROM relay_telemetry_reports WHERE relay_id = $1",
    )
    .bind(relay_id.as_slice())
    .fetch_one(pool)
    .await
    .expect("detailed Relay telemetry");
    assert_eq!(detailed, (0, 1, 1, 0, 0, 0, 0, 0, 0));
}

fn relay_metrics_fixture() -> RelayTelemetryMetrics {
    RelayTelemetryMetrics {
        active_leases: 2,
        packets_received: 10,
        bytes_received: 1_000,
        registrations_accepted: 2,
        registration_retries: 0,
        registrations_rejected: 1,
        packets_forwarded: 8,
        bytes_forwarded: 800,
        keepalives_accepted: 4,
        invalid_drops: 1,
        authentication_drops: 0,
        replay_drops: 0,
        rate_limit_drops: 0,
        queue_drops: 0,
        destination_drops: 0,
        send_drops: 0,
        packets_dropped: 1,
        io_errors: 0,
        forwarding_latency_samples: 8,
        forwarding_latency_microseconds_total: 400,
        forwarding_latency_microseconds_max: 80,
    }
}

fn sign_relay_report(
    report: RelayTelemetryReport,
    signing_key: &SigningKey,
) -> SignedRelayTelemetryReport {
    let input = relay_telemetry_report_signing_input(&report).expect("Relay signing input");
    SignedRelayTelemetryReport {
        report,
        signature_base64: URL_SAFE_NO_PAD.encode(signing_key.sign(&input).to_bytes()),
    }
}

async fn reset_database(pool: &sqlx::PgPool) {
    sqlx::query(
        "TRUNCATE relay_telemetry_samples, relay_telemetry_reports,
                  node_telemetry_samples, node_telemetry_reports,
                  update_rollout_policies, update_releases,
                  console_login_attempts, console_sessions, audit_events,
                  configuration_versions, subnet_routes,
                  node_subnet_route_advertisements, acl_rules,
                  node_group_memberships, node_groups, ip_leases, nodes,
                  enrollment_tokens, networks
         RESTART IDENTITY CASCADE",
    )
    .execute(pool)
    .await
    .expect("truncate project test tables");
    sqlx::query("ALTER SEQUENCE credential_serial RESTART WITH 1")
        .execute(pool)
        .await
        .expect("reset credential serial");
    sqlx::query("DELETE FROM console_users WHERE username != 'admin'")
        .execute(pool)
        .await
        .expect("remove non-bootstrap console users");
}

struct BrowserSession {
    cookie: String,
    csrf_token: String,
}

async fn assert_console_authentication_and_permissions(router: &Router, pool: &sqlx::PgPool) {
    assert_login_rate_limit(router).await;
    let mut administrator =
        login_console_user(router, "admin", CONSOLE_PASSWORD, "administrator").await;
    refresh_console_session(router, &mut administrator, "admin").await;
    create_auditor(router, &administrator).await;
    assert_administrator_csrf_and_network(router, &administrator).await;

    let auditor = login_console_user(
        router,
        "auditor",
        "integration-viewer-password-42",
        "auditor",
    )
    .await;
    assert_auditor_permissions(router, &auditor).await;
    assert_authentication_storage(pool).await;
    logout_console_user(router, &administrator).await;
}

async fn assert_login_rate_limit(router: &Router) {
    for _ in 0..5 {
        let (status, _, response) = browser_request(
            router,
            Method::POST,
            "/v1/auth/login",
            Some(json!({"username": "missing-user", "password": "incorrect-password"})),
            None,
            None,
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(response["error"]["code"], "unauthorized");
    }
    let (status, _, response) = browser_request(
        router,
        Method::POST,
        "/v1/auth/login",
        Some(json!({"username": "missing-user", "password": "incorrect-password"})),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(response["error"]["code"], "rate_limited");
}

async fn login_console_user(
    router: &Router,
    username: &str,
    password: &str,
    expected_role: &str,
) -> BrowserSession {
    let (status, headers, login) = browser_request(
        router,
        Method::POST,
        "/v1/auth/login",
        Some(json!({"username": username, "password": password})),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(login["user"]["username"], username);
    assert_eq!(login["user"]["role"], expected_role);
    let csrf_token = login["csrf_token"].as_str().expect("CSRF token").to_owned();
    assert!(csrf_token.len() >= 32);
    let set_cookie = headers
        .get(header::SET_COOKIE)
        .and_then(|value| value.to_str().ok())
        .expect("session cookie");
    assert!(set_cookie.contains("xs_nexus_session="));
    assert!(set_cookie.contains("HttpOnly"));
    assert!(set_cookie.contains("SameSite=Strict"));
    assert!(set_cookie.contains("Secure"));
    BrowserSession {
        cookie: set_cookie
            .split(';')
            .next()
            .expect("cookie pair")
            .to_owned(),
        csrf_token,
    }
}

async fn refresh_console_session(
    router: &Router,
    session: &mut BrowserSession,
    expected_username: &str,
) {
    let (status, _, response) = browser_request(
        router,
        Method::GET,
        "/v1/auth/session",
        None,
        Some(&session.cookie),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(response["user"]["username"], expected_username);
    let refreshed_csrf_token = response["csrf_token"].as_str().expect("stable CSRF token");
    assert_eq!(refreshed_csrf_token, session.csrf_token);
}

async fn create_auditor(router: &Router, administrator: &BrowserSession) {
    let (status, _, viewer) = browser_request(
        router,
        Method::POST,
        "/v1/admin/users",
        Some(json!({
            "username": "auditor",
            "display_name": "审计员",
            "password": "integration-viewer-password-42",
            "role": "auditor"
        })),
        Some(&administrator.cookie),
        Some(&administrator.csrf_token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(viewer["username"], "auditor");
    assert_eq!(viewer["role"], "auditor");
}

async fn assert_administrator_csrf_and_network(router: &Router, administrator: &BrowserSession) {
    let request = json!({
        "name": "console-network",
        "address_pool": "100.89.0.0/24",
        "reserved_addresses": 16
    });
    let (status, _, _) = browser_request(
        router,
        Method::POST,
        "/v1/admin/networks",
        Some(request.clone()),
        Some(&administrator.cookie),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, _, network) = browser_request(
        router,
        Method::POST,
        "/v1/admin/networks",
        Some(request),
        Some(&administrator.cookie),
        Some(&administrator.csrf_token),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(network["address_pool"], "100.89.0.0/24");
}

async fn assert_auditor_permissions(router: &Router, auditor: &BrowserSession) {
    let (status, _, networks) = browser_request(
        router,
        Method::GET,
        "/v1/admin/networks",
        None,
        Some(&auditor.cookie),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(networks.as_array().is_some_and(|items| !items.is_empty()));

    let (status, _, response) = browser_request(
        router,
        Method::POST,
        "/v1/admin/networks",
        Some(json!({
            "name": "auditor-must-fail",
            "address_pool": "100.90.0.0/24",
            "reserved_addresses": 16
        })),
        Some(&auditor.cookie),
        Some(&auditor.csrf_token),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(response["error"]["code"], "forbidden");
}

async fn assert_authentication_storage(pool: &sqlx::PgPool) {
    let password_hash: String =
        sqlx::query_scalar("SELECT password_hash FROM console_users WHERE username = 'admin'")
            .fetch_one(pool)
            .await
            .expect("stored password hash");
    assert!(password_hash.starts_with("$argon2id$"));
    assert!(!password_hash.contains(CONSOLE_PASSWORD));
    let token_hash_length: i32 = sqlx::query_scalar(
        "SELECT octet_length(token_hash) FROM console_sessions WHERE revoked_at IS NULL LIMIT 1",
    )
    .fetch_one(pool)
    .await
    .expect("stored session hash");
    assert_eq!(token_hash_length, 32);
}

async fn logout_console_user(router: &Router, session: &BrowserSession) {
    let (status, _, _) = browser_request(
        router,
        Method::POST,
        "/v1/auth/logout",
        None,
        Some(&session.cookie),
        Some(&session.csrf_token),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _, _) = browser_request(
        router,
        Method::GET,
        "/v1/auth/session",
        None,
        Some(&session.cookie),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

async fn assert_console_snapshot(router: &Router) {
    let (status, unauthorized) =
        request_json(router, Method::GET, "/v1/admin/console", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(unauthorized["error"]["code"], "unauthorized");

    let (status, snapshot) = request_json(
        router,
        Method::GET,
        "/v1/admin/console",
        None,
        Some(ADMIN_TOKEN),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(snapshot["collected_at"].is_string());
    assert_eq!(snapshot["dashboard"]["online_nodes"], 0);
    assert!(
        snapshot["dashboard"]["offline_nodes"]
            .as_u64()
            .is_some_and(|count| count > 0)
    );
    assert_eq!(
        snapshot["dashboard"]["direct_nodes"]["status"],
        "unavailable"
    );
    assert!(snapshot["dashboard"]["direct_nodes"]["value"].is_null());
    assert!(
        snapshot["networks"]
            .as_array()
            .is_some_and(|items| items.len() >= 2)
    );
    assert!(
        snapshot["nodes"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
    );
    assert!(
        snapshot["enrollment_tokens"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
    );
    assert!(
        snapshot["acl_rules"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
    );
    assert!(
        snapshot["subnet_routes"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
    );
    assert!(
        snapshot["audit_events"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
    );
    assert_eq!(snapshot["system"]["database"]["value"], "ok");
    assert_eq!(
        snapshot["system"]["update_management"]["status"],
        "available"
    );
    let reported_node = snapshot["nodes"]
        .as_array()
        .expect("console nodes")
        .iter()
        .find(|node| node["agent_version"]["status"] == "available")
        .expect("runtime-reported node");
    assert_eq!(reported_node["agent_version"]["value"], "0.1.0");
    assert_eq!(reported_node["architecture"]["value"], "x86_64");
    assert_eq!(reported_node["update_channel"]["value"], "testing");
    assert_eq!(reported_node["update_state"]["value"], "idle");
    assert!(reported_node["update_reported_at"].is_string());
    let encoded = serde_json::to_string(&snapshot).expect("serialize console snapshot");
    assert!(!encoded.contains("token_hash"));
    assert!(!encoded.contains("password_hash"));
    assert!(!encoded.contains(CONSOLE_PASSWORD));
    assert_observability_snapshot(router).await;
}

async fn assert_observability_snapshot(router: &Router) {
    let (status, unauthorized) =
        request_json(router, Method::GET, "/v1/admin/observability", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(unauthorized["error"]["code"], "unauthorized");

    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/v1/admin/observability")
                .header(header::AUTHORIZATION, format!("Bearer {ADMIN_TOKEN}"))
                .body(Body::empty())
                .expect("observability request"),
        )
        .await
        .expect("observability response");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok()),
        Some("no-store")
    );
    let snapshot: Value = serde_json::from_slice(
        &response
            .into_body()
            .collect()
            .await
            .expect("observability body")
            .to_bytes(),
    )
    .expect("observability JSON");
    assert_eq!(snapshot["schema_version"], 1);
    assert_eq!(snapshot["controller"]["status"], "ok");
    assert_eq!(snapshot["controller"]["active_control_sessions"], 0);
    assert_eq!(snapshot["controller"]["maximum_control_sessions"], 1000);
    assert_eq!(
        snapshot["controller"]["rejected_control_sessions_since_start"],
        0
    );
    assert_eq!(snapshot["controller"]["active_configuration_sends"], 0);
    assert_eq!(snapshot["controller"]["maximum_configuration_sends"], 64);
    assert_eq!(snapshot["controller"]["maximum_nodes_per_network"], 1000);
    assert_eq!(snapshot["database"]["status"], "ok");
    assert_eq!(snapshot["redis"]["status"], "not_applicable");
    assert!(
        snapshot["nodes"]["managed"]
            .as_u64()
            .is_some_and(|value| value > 0)
    );
    assert!(
        snapshot["nodes"]["largest_network_managed"]
            .as_u64()
            .is_some_and(|value| value > 0)
    );
    assert!(
        snapshot["nodes"]["fresh_telemetry"]
            .as_u64()
            .is_some_and(|value| value > 0)
    );
    assert_eq!(snapshot["nodes"]["acl_drops_24h"], 2);
    assert_eq!(snapshot["nodes"]["replay_drops_24h"], 3);
    assert_eq!(snapshot["relays"]["configured"], 1);
    assert_eq!(snapshot["relays"]["fresh"], 1);
    assert_eq!(snapshot["security"]["acl_drops_24h"], 2);
    assert_eq!(snapshot["security"]["replay_drops_24h"], 3);
    assert!(
        snapshot["security"]["management_auth_failures_24h"]
            .as_u64()
            .is_some_and(|value| value >= 6)
    );
    assert!(
        snapshot["audit"]["total_events"]
            .as_u64()
            .is_some_and(|value| value > 0)
    );
    let encoded = serde_json::to_string(&snapshot).expect("serialize observability snapshot");
    assert!(!encoded.contains("node_id_base64"));
    assert!(!encoded.contains("virtual_ip"));
    assert!(!encoded.contains("password"));
    assert!(!encoded.contains("token"));
}

async fn browser_request(
    router: &Router,
    method: Method,
    path: &str,
    body: Option<Value>,
    cookie: Option<&str>,
    csrf_token: Option<&str>,
) -> (StatusCode, axum::http::HeaderMap, Value) {
    let mut builder = Request::builder().method(method).uri(path);
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    if let Some(csrf_token) = csrf_token {
        builder = builder.header("x-csrf-token", csrf_token);
    }
    let body = if let Some(body) = body {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
        Body::from(serde_json::to_vec(&body).expect("serialize browser body"))
    } else {
        Body::empty()
    };
    let response = router
        .clone()
        .oneshot(builder.body(body).expect("browser request"))
        .await
        .expect("browser router response");
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("collect browser response")
        .to_bytes();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("browser JSON response")
    };
    (status, headers, value)
}

async fn create_token(router: &Router, network_id: &str, max_uses: u16) -> (String, String) {
    let (status, response) = request_json(
        router,
        Method::POST,
        "/v1/admin/enrollment-tokens",
        Some(json!({
            "network_id": network_id,
            "expires_in_seconds": 3600,
            "max_uses": max_uses,
            "default_role_bitmap": 1,
            "default_tags": ["linux"]
        })),
        Some(ADMIN_TOKEN),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    (
        response["id"].as_str().expect("token id").to_owned(),
        response["token"].as_str().expect("token").to_owned(),
    )
}

async fn enroll(
    router: &Router,
    token: &str,
    name: &str,
    public_key: &[u8; 32],
) -> (StatusCode, Value) {
    request_json(
        router,
        Method::POST,
        "/v1/enroll",
        Some(json!({
            "token": token,
            "name": name,
            "device_type": "linux",
            "identity_public_key_base64": URL_SAFE_NO_PAD.encode(public_key)
        })),
        None,
    )
    .await
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

async fn assert_token_is_hash_only(pool: &sqlx::PgPool, schema: &str) {
    let plaintext_columns: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.columns
         WHERE table_schema = $1 AND table_name = 'enrollment_tokens'
           AND column_name IN ('token', 'plaintext_token')",
    )
    .bind(schema)
    .fetch_one(pool)
    .await
    .expect("inspect token columns");
    assert_eq!(plaintext_columns, 0);

    let hash_length: i32 =
        sqlx::query_scalar("SELECT octet_length(token_hash) FROM enrollment_tokens LIMIT 1")
            .fetch_one(pool)
            .await
            .expect("token hash length");
    assert_eq!(hash_length, 32);
}

fn verify_enrollment_artifacts(
    state: &xs_controller::AppState,
    enrollment: &Value,
    discovery_address: SocketAddr,
) {
    let credential = URL_SAFE_NO_PAD
        .decode(
            enrollment["credential_base64"]
                .as_str()
                .expect("credential"),
        )
        .expect("decode credential");
    let verified = verify_credential(
        &credential,
        &state.credential_signing_key.verifying_key(),
        u64::try_from(chrono::Utc::now().timestamp()).expect("current time"),
    )
    .expect("credential verifies");
    assert_eq!(verified.virtual_ipv4.to_string(), "100.88.0.16");

    let configuration = &enrollment["configuration"];
    let payload = URL_SAFE_NO_PAD
        .decode(
            configuration["payload_base64"]
                .as_str()
                .expect("config payload"),
        )
        .expect("decode config");
    let signature = URL_SAFE_NO_PAD
        .decode(
            configuration["signature_base64"]
                .as_str()
                .expect("config signature"),
        )
        .expect("decode signature");
    let signature = Signature::from_bytes(
        &signature
            .try_into()
            .expect("configuration signature length"),
    );
    let mut input = Vec::with_capacity(CONFIGURATION_DOMAIN.len() + payload.len());
    input.extend_from_slice(CONFIGURATION_DOMAIN);
    input.extend_from_slice(&payload);
    state
        .config_signing_key
        .verifying_key()
        .verify_strict(&input, &signature)
        .expect("configuration signature verifies");

    let payload: Value = serde_json::from_slice(&payload).expect("configuration JSON");
    assert_eq!(payload["version"], 2);
    assert_eq!(payload["nodes"].as_array().expect("nodes").len(), 1);
    assert_eq!(
        payload["discovery_endpoints"][0],
        discovery_address.to_string()
    );
}

async fn verify_udp_discovery(
    state: &xs_controller::AppState,
    enrollment: &Value,
    identity: &SigningKey,
    discovery_address: SocketAddr,
) {
    let network_id = Uuid::from_str(enrollment["network_id"].as_str().expect("network id"))
        .expect("valid network id");
    let node_id: [u8; 16] = URL_SAFE_NO_PAD
        .decode(enrollment["node_id_base64"].as_str().expect("node id"))
        .expect("decode node id")
        .try_into()
        .expect("node id length");
    let credential: [u8; CREDENTIAL_LENGTH] = URL_SAFE_NO_PAD
        .decode(
            enrollment["credential_base64"]
                .as_str()
                .expect("credential"),
        )
        .expect("decode credential")
        .try_into()
        .expect("credential length");
    let now = u64::try_from(Utc::now().timestamp()).expect("current time");
    let request = DiscoveryRequest::new(
        *network_id.as_bytes(),
        node_id,
        [71_u8; 16],
        now,
        credential,
        identity,
    )
    .expect("build discovery request");
    let client = UdpSocket::bind(("127.0.0.1", 0))
        .await
        .expect("bind discovery client");
    let expected_observed = client.local_addr().expect("discovery client address");
    client
        .send_to(request.encoded(), discovery_address)
        .await
        .expect("send discovery request");
    let mut response = [0_u8; xs_protocol::DISCOVERY_RESPONSE_LENGTH];
    let (length, source) = timeout(Duration::from_secs(2), client.recv_from(&mut response))
        .await
        .expect("discovery response timeout")
        .expect("receive discovery response");
    assert_eq!(length, response.len());
    assert_eq!(source, discovery_address);
    let verified = verify_discovery_response(
        &response,
        &request,
        &state.config_signing_key.verifying_key(),
        u64::try_from(Utc::now().timestamp()).expect("current time"),
    )
    .expect("verify discovery response");
    assert_eq!(verified.observed_endpoint, expected_observed);

    let mut tampered = *request.encoded();
    tampered[tampered.len() - 1] ^= 1;
    client
        .send_to(&tampered, discovery_address)
        .await
        .expect("send tampered discovery request");
    assert!(
        timeout(Duration::from_millis(200), client.recv_from(&mut response))
            .await
            .is_err(),
        "tampered discovery request must be silently dropped"
    );
}

async fn assert_audit_is_append_only_and_redacted(pool: &sqlx::PgPool, token: &str) {
    let audit_count: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_events")
        .fetch_one(pool)
        .await
        .expect("audit count");
    assert!(audit_count >= 7);

    let token_leaked: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM audit_events
            WHERE metadata::text LIKE '%' || $1 || '%'
               OR actor_id LIKE '%' || $1 || '%'
         )",
    )
    .bind(token)
    .fetch_one(pool)
    .await
    .expect("audit token scan");
    assert!(!token_leaked);

    let mutation = sqlx::query(
        "UPDATE audit_events SET outcome = 'failure' WHERE id = (SELECT min(id) FROM audit_events)",
    )
    .execute(pool)
    .await;
    assert!(mutation.is_err());
}

async fn verify_websocket_control(
    router: &Router,
    pool: &sqlx::PgPool,
    enrollment: &Value,
    identity: &SigningKey,
    discovery_address: SocketAddr,
) {
    Box::pin(verify_websocket_control_inner(
        router,
        pool,
        enrollment,
        identity,
        discovery_address,
    ))
    .await;
}

async fn verify_websocket_control_inner(
    router: &Router,
    pool: &sqlx::PgPool,
    enrollment: &Value,
    identity: &SigningKey,
    discovery_address: SocketAddr,
) {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("bind test controller");
    let address = listener.local_addr().expect("listener address");
    let application = router.clone();
    let server = tokio::spawn(async move {
        axum::serve(listener, application)
            .await
            .expect("serve test controller");
    });

    let (mut socket, initial_version) = authenticate_websocket(address, enrollment, identity).await;
    let (mut observer, observer_version) =
        authenticate_websocket(address, enrollment, identity).await;
    assert_eq!(observer_version, initial_version);

    socket
        .send(Message::Text(
            json!({"type": "sync", "last_version": initial_version})
                .to_string()
                .into(),
        ))
        .await
        .expect("send sync");
    let synchronized = socket
        .next()
        .await
        .expect("sync response")
        .expect("valid sync response");
    let Message::Text(synchronized) = synchronized else {
        panic!("expected sync text");
    };
    let synchronized: Value = serde_json::from_str(&synchronized).expect("sync JSON");
    assert_eq!(synchronized["type"], "up_to_date");
    assert_eq!(synchronized["version"], initial_version);

    let updated_version = initial_version
        .checked_add(1)
        .expect("configuration version");
    advertise_candidates_and_verify(
        &mut socket,
        enrollment,
        identity,
        discovery_address,
        updated_version,
    )
    .await;
    let broadcast = timeout(Duration::from_secs(2), observer.next())
        .await
        .expect("configuration broadcast timeout")
        .expect("observer message")
        .expect("valid observer message");
    let Message::Text(broadcast) = broadcast else {
        panic!("expected configuration broadcast text");
    };
    let broadcast: Value = serde_json::from_str(&broadcast).expect("broadcast JSON");
    assert_eq!(broadcast["type"], "configuration");
    assert_eq!(broadcast["configuration"]["version"], updated_version);
    assert_console_online_count(router, 1).await;
    let runtime_report = runtime_report_fixture(enrollment, Utc::now());
    send_signed_runtime_report(&mut socket, &runtime_report, identity).await;
    let runtime_response = receive_update_directive(&mut socket, updated_version).await;
    assert_eq!(runtime_response["type"], "update_directive");
    send_initial_telemetry(pool, enrollment, identity, &mut socket).await;
    Box::pin(assert_telemetry_report_rejections(
        pool, address, enrollment, identity,
    ))
    .await;
    Box::pin(assert_runtime_report_rejections(
        address, enrollment, identity,
    ))
    .await;

    socket.close(None).await.expect("close websocket");
    observer
        .close(None)
        .await
        .expect("close observer websocket");
    timeout(Duration::from_secs(2), async {
        loop {
            if console_online_count(router).await == 0 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("control disconnect updates console presence");
    server.abort();
}

async fn send_initial_telemetry(
    pool: &sqlx::PgPool,
    enrollment: &Value,
    identity: &SigningKey,
    socket: &mut ControlSocket,
) {
    let generated_at = Utc::now();
    let mut legacy = telemetry_report_fixture(pool, enrollment, 1, generated_at).await;
    legacy.schema_version = 1;
    if let Some(peer) = legacy.peers.first_mut() {
        peer.tx_packets_total = 1;
        peer.tx_bytes_total = 10;
        legacy.tx_bytes_total = 10;
    }
    let signing_input =
        agent_telemetry_report_signing_input(&legacy).expect("legacy telemetry signing input");
    let mut legacy_value = serde_json::to_value(&legacy).expect("legacy telemetry JSON");
    let legacy_object = legacy_value
        .as_object_mut()
        .expect("legacy telemetry object");
    legacy_object.remove("acl_drops_total");
    legacy_object.remove("replay_drops_total");
    send_telemetry_report_value(
        socket,
        legacy_value,
        URL_SAFE_NO_PAD.encode(identity.sign(&signing_input).to_bytes()),
    )
    .await;
    let response = socket
        .next()
        .await
        .expect("telemetry response")
        .expect("valid telemetry response");
    let Message::Text(response) = response else {
        panic!("expected telemetry response text");
    };
    let response: Value = serde_json::from_str(&response).expect("telemetry response JSON");
    assert_eq!(response["type"], "telemetry_accepted");
    assert_eq!(response["sequence"], 1);

    let mut report = telemetry_report_fixture(
        pool,
        enrollment,
        2,
        generated_at + chrono::Duration::milliseconds(1),
    )
    .await;
    report.tx_bytes_total = 10;
    report.acl_drops_total = 2;
    report.replay_drops_total = 3;
    if let Some(peer) = report.peers.first_mut() {
        peer.tx_packets_total = 1;
        peer.tx_bytes_total = 10;
    }
    send_signed_telemetry_report(socket, &report, identity).await;
    let response = socket
        .next()
        .await
        .expect("telemetry response")
        .expect("valid telemetry response");
    let Message::Text(response) = response else {
        panic!("expected telemetry response text");
    };
    let response: Value = serde_json::from_str(&response).expect("telemetry response JSON");
    assert_eq!(response["type"], "telemetry_accepted");
    assert_eq!(response["sequence"], 2);

    let latest: (i64, i64) = sqlx::query_as(
        "SELECT acl_drops_total, replay_drops_total
         FROM node_telemetry_reports WHERE node_id = $1",
    )
    .bind(
        URL_SAFE_NO_PAD
            .decode(enrollment["node_id_base64"].as_str().expect("node id"))
            .expect("node id base64"),
    )
    .fetch_one(pool)
    .await
    .expect("latest observability counters");
    assert_eq!(latest, (2, 3));
    let sample_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM node_telemetry_samples
         WHERE node_id = $1 AND acl_drops_total = 2 AND replay_drops_total = 3",
    )
    .bind(
        URL_SAFE_NO_PAD
            .decode(enrollment["node_id_base64"].as_str().expect("node id"))
            .expect("node id base64"),
    )
    .fetch_one(pool)
    .await
    .expect("observability counter sample");
    assert_eq!(sample_count, 1);
}

async fn assert_telemetry_report_rejections(
    pool: &sqlx::PgPool,
    address: SocketAddr,
    enrollment: &Value,
    identity: &SigningKey,
) {
    let mut accepted = telemetry_report_fixture(pool, enrollment, 2, Utc::now()).await;
    accepted.tx_bytes_total = 10;
    accepted.acl_drops_total = 2;
    accepted.replay_drops_total = 3;
    if let Some(peer) = accepted.peers.first_mut() {
        peer.tx_packets_total = 1;
        peer.tx_bytes_total = 10;
    }
    let (mut replay, _) = authenticate_websocket(address, enrollment, identity).await;
    send_signed_telemetry_report(&mut replay, &accepted, identity).await;
    assert_control_error_and_close(&mut replay, "telemetry_report_rejected").await;

    let stale = telemetry_report_fixture(
        pool,
        enrollment,
        3,
        Utc::now() - chrono::Duration::minutes(11),
    )
    .await;
    let (mut stale_socket, _) = authenticate_websocket(address, enrollment, identity).await;
    send_signed_telemetry_report(&mut stale_socket, &stale, identity).await;
    assert_control_error_and_close(&mut stale_socket, "telemetry_report_rejected").await;

    let invalid = telemetry_report_fixture(pool, enrollment, 3, Utc::now()).await;
    let (mut invalid_signature, _) = authenticate_websocket(address, enrollment, identity).await;
    send_telemetry_report_value(
        &mut invalid_signature,
        serde_json::to_value(invalid).expect("telemetry JSON"),
        URL_SAFE_NO_PAD.encode([0_u8; 64]),
    )
    .await;
    assert_control_error_and_close(&mut invalid_signature, "telemetry_report_rejected").await;

    let mut inconsistent = telemetry_report_fixture(pool, enrollment, 3, Utc::now()).await;
    inconsistent.handshake_successes_total = 1;
    let (mut inconsistent_socket, _) = authenticate_websocket(address, enrollment, identity).await;
    send_signed_telemetry_report(&mut inconsistent_socket, &inconsistent, identity).await;
    assert_control_error_and_close(&mut inconsistent_socket, "telemetry_report_rejected").await;

    let mut peer_rollback = telemetry_report_fixture(pool, enrollment, 3, Utc::now()).await;
    peer_rollback.acl_drops_total = 2;
    peer_rollback.replay_drops_total = 3;
    if let Some(peer) = peer_rollback.peers.first_mut() {
        peer.tx_bytes_total = 9;
        peer_rollback.tx_bytes_total = 10;
    }
    let (mut peer_rollback_socket, _) = authenticate_websocket(address, enrollment, identity).await;
    send_signed_telemetry_report(&mut peer_rollback_socket, &peer_rollback, identity).await;
    assert_control_error_and_close(&mut peer_rollback_socket, "telemetry_report_rejected").await;
}

async fn telemetry_report_fixture(
    pool: &sqlx::PgPool,
    enrollment: &Value,
    sequence: u64,
    generated_at: chrono::DateTime<Utc>,
) -> AgentTelemetryReport {
    let network_id = Uuid::from_str(enrollment["network_id"].as_str().expect("network id"))
        .expect("valid network id");
    let local_node_id = URL_SAFE_NO_PAD
        .decode(enrollment["node_id_base64"].as_str().expect("node id"))
        .expect("node id base64");
    let peer_ids = sqlx::query_scalar::<_, Vec<u8>>(
        "SELECT node_id FROM nodes
         WHERE network_id = $1 AND node_id <> $2 AND revoked_at IS NULL
         ORDER BY node_id",
    )
    .bind(network_id)
    .bind(local_node_id)
    .fetch_all(pool)
    .await
    .expect("load telemetry peers");
    AgentTelemetryReport {
        schema_version: 2,
        network_id,
        node_id_base64: enrollment["node_id_base64"]
            .as_str()
            .expect("node id")
            .to_owned(),
        boot_id_base64: "AQEBAQEBAQEBAQEBAQEBAQ".to_owned(),
        sequence,
        generated_at,
        tx_bytes_total: 0,
        rx_bytes_total: 0,
        handshake_attempts_total: 0,
        handshake_successes_total: 0,
        acl_drops_total: 0,
        replay_drops_total: 0,
        latency_samples_total: 0,
        latency_microseconds_total: 0,
        peers: peer_ids
            .into_iter()
            .map(|node_id| AgentPeerTelemetry {
                peer_node_id_base64: URL_SAFE_NO_PAD.encode(node_id),
                path: AgentPathKind::Disconnected,
                relay_id_base64: None,
                session_established: false,
                last_latency_microseconds: None,
                tx_packets_total: 0,
                tx_bytes_total: 0,
                rx_packets_total: 0,
                rx_bytes_total: 0,
                handshake_attempts_total: 0,
                handshake_successes_total: 0,
                latency_samples_total: 0,
                latency_microseconds_total: 0,
            })
            .collect(),
    }
}

async fn send_signed_telemetry_report(
    socket: &mut ControlSocket,
    report: &AgentTelemetryReport,
    identity: &SigningKey,
) {
    let input = agent_telemetry_report_signing_input(report).expect("telemetry signing input");
    send_telemetry_report_value(
        socket,
        serde_json::to_value(report).expect("telemetry JSON"),
        URL_SAFE_NO_PAD.encode(identity.sign(&input).to_bytes()),
    )
    .await;
}

async fn send_telemetry_report_value(
    socket: &mut ControlSocket,
    report: Value,
    signature_base64: String,
) {
    socket
        .send(Message::Text(
            json!({
                "type": "report_telemetry",
                "report": report,
                "signature_base64": signature_base64
            })
            .to_string()
            .into(),
        ))
        .await
        .expect("send telemetry report");
}

async fn receive_update_directive(socket: &mut ControlSocket, expected_version: u64) -> Value {
    timeout(Duration::from_secs(2), async {
        loop {
            let response = socket
                .next()
                .await
                .expect("runtime response")
                .expect("valid runtime response");
            let Message::Text(response) = response else {
                panic!("expected runtime response text");
            };
            let response: Value = serde_json::from_str(&response).expect("runtime response JSON");
            if response["type"] == "update_directive" {
                break response;
            }
            assert_eq!(response["type"], "configuration");
            assert_eq!(response["configuration"]["version"], expected_version);
        }
    })
    .await
    .expect("runtime response timeout")
}

async fn assert_runtime_report_rejections(
    address: SocketAddr,
    enrollment: &Value,
    identity: &SigningKey,
) {
    let report = runtime_report_fixture(enrollment, Utc::now());
    let (mut malformed, _) = authenticate_websocket(address, enrollment, identity).await;
    let mut malformed_value = serde_json::to_value(&report).expect("runtime report JSON");
    malformed_value["unexpected"] = json!(true);
    send_runtime_report_value(
        &mut malformed,
        malformed_value,
        URL_SAFE_NO_PAD.encode(identity.sign(b"not-the-report").to_bytes()),
    )
    .await;
    assert_control_error_and_close(&mut malformed, "invalid_control_message").await;

    let stale_report =
        runtime_report_fixture(enrollment, Utc::now() - chrono::Duration::minutes(10));
    let (mut stale, _) = authenticate_websocket(address, enrollment, identity).await;
    send_signed_runtime_report(&mut stale, &stale_report, identity).await;
    assert_control_error_and_close(&mut stale, "runtime_report_rejected").await;

    let (mut invalid_signature, _) = authenticate_websocket(address, enrollment, identity).await;
    send_runtime_report_value(
        &mut invalid_signature,
        serde_json::to_value(report).expect("runtime report JSON"),
        URL_SAFE_NO_PAD.encode([0_u8; 64]),
    )
    .await;
    assert_control_error_and_close(&mut invalid_signature, "runtime_report_rejected").await;
}

fn runtime_report_fixture(
    enrollment: &Value,
    generated_at: chrono::DateTime<Utc>,
) -> AgentRuntimeReport {
    AgentRuntimeReport {
        schema_version: 1,
        network_id: Uuid::from_str(enrollment["network_id"].as_str().expect("network id"))
            .expect("valid network id"),
        node_id_base64: enrollment["node_id_base64"]
            .as_str()
            .expect("node id")
            .to_owned(),
        agent_version: "0.1.0".parse().expect("Agent version"),
        platform: "linux".to_owned(),
        architecture: "x86_64".to_owned(),
        update_channel: UpdateChannel::Stable,
        update_state: AgentUpdateState::Idle,
        observed_release_id: None,
        last_error_code: None,
        generated_at,
    }
}

async fn send_signed_runtime_report(
    socket: &mut ControlSocket,
    report: &AgentRuntimeReport,
    identity: &SigningKey,
) {
    let signing_input = agent_runtime_report_signing_input(report).expect("runtime signing input");
    send_runtime_report_value(
        socket,
        serde_json::to_value(report).expect("runtime report JSON"),
        URL_SAFE_NO_PAD.encode(identity.sign(&signing_input).to_bytes()),
    )
    .await;
}

async fn send_runtime_report_value(
    socket: &mut ControlSocket,
    report: Value,
    signature_base64: String,
) {
    socket
        .send(Message::Text(
            json!({
                "type": "report_runtime",
                "report": report,
                "signature_base64": signature_base64
            })
            .to_string()
            .into(),
        ))
        .await
        .expect("send runtime report");
}

async fn assert_control_error_and_close(socket: &mut ControlSocket, expected_code: &str) {
    let response = socket
        .next()
        .await
        .expect("control error response")
        .expect("valid control error response");
    let Message::Text(response) = response else {
        panic!("expected control error text");
    };
    let response: Value = serde_json::from_str(&response).expect("control error JSON");
    assert_eq!(response["type"], "error");
    assert_eq!(response["code"], expected_code);
    assert!(matches!(
        socket.next().await,
        Some(Ok(Message::Close(_))) | None
    ));
}

async fn authenticate_websocket(
    address: SocketAddr,
    enrollment: &Value,
    identity: &SigningKey,
) -> (ControlSocket, u64) {
    let (mut socket, _) = connect_async(format!("ws://{address}/v1/control"))
        .await
        .expect("connect control websocket");
    let challenge = socket
        .next()
        .await
        .expect("challenge message")
        .expect("valid challenge");
    let Message::Text(challenge) = challenge else {
        panic!("expected text challenge");
    };
    let challenge: Value = serde_json::from_str(&challenge).expect("challenge JSON");
    let challenge = URL_SAFE_NO_PAD
        .decode(
            challenge["challenge_base64"]
                .as_str()
                .expect("challenge value"),
        )
        .expect("decode challenge");
    let node_id = URL_SAFE_NO_PAD
        .decode(enrollment["node_id_base64"].as_str().expect("node id"))
        .expect("decode node id");
    let mut input =
        Vec::with_capacity(CONTROL_AUTHENTICATION_DOMAIN.len() + challenge.len() + node_id.len());
    input.extend_from_slice(CONTROL_AUTHENTICATION_DOMAIN);
    input.extend_from_slice(&challenge);
    input.extend_from_slice(&node_id);
    let signature = identity.sign(&input);
    socket
        .send(Message::Text(
            json!({
                "type": "authenticate",
                "node_id_base64": enrollment["node_id_base64"],
                "credential_base64": enrollment["credential_base64"],
                "signature_base64": URL_SAFE_NO_PAD.encode(signature.to_bytes())
            })
            .to_string()
            .into(),
        ))
        .await
        .expect("send authentication");
    let authenticated = socket
        .next()
        .await
        .expect("authenticated message")
        .expect("valid authenticated message");
    let Message::Text(authenticated) = authenticated else {
        panic!("expected authenticated text");
    };
    let authenticated: Value = serde_json::from_str(&authenticated).expect("authenticated JSON");
    assert_eq!(authenticated["type"], "authenticated");
    let version = authenticated["configuration"]["version"]
        .as_u64()
        .expect("configuration version");
    (socket, version)
}

async fn assert_console_online_count(router: &Router, expected: u64) {
    timeout(Duration::from_secs(2), async {
        loop {
            if console_online_count(router).await == expected {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("console online count converges");
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

async fn advertise_candidates_and_verify(
    socket: &mut ControlSocket,
    enrollment: &Value,
    identity: &SigningKey,
    discovery_address: SocketAddr,
    expected_version: u64,
) {
    let now = Utc::now();
    let advertisement = CandidateAdvertisement {
        schema_version: 1,
        network_id: Uuid::from_str(enrollment["network_id"].as_str().expect("network id"))
            .expect("valid network id"),
        node_id_base64: enrollment["node_id_base64"]
            .as_str()
            .expect("node id")
            .to_owned(),
        generation: 1,
        generated_at: now,
        expires_at: now + chrono::Duration::minutes(10),
        candidates: vec![
            EndpointCandidate {
                kind: EndpointCandidateKind::Mapped,
                endpoint: SocketAddr::from((Ipv4Addr::new(198, 51, 100, 27), 42001)),
                priority: 200,
                expires_at: now + chrono::Duration::minutes(9),
            },
            EndpointCandidate {
                kind: EndpointCandidateKind::Local,
                endpoint: SocketAddr::from((Ipv4Addr::new(192, 168, 50, 9), 42001)),
                priority: 100,
                expires_at: now + chrono::Duration::minutes(9),
            },
        ],
    };
    let payload = serde_json::to_vec(&advertisement).expect("serialize candidate advertisement");
    let mut signing_input =
        Vec::with_capacity(CANDIDATE_ADVERTISEMENT_DOMAIN.len() + payload.len());
    signing_input.extend_from_slice(CANDIDATE_ADVERTISEMENT_DOMAIN);
    signing_input.extend_from_slice(&payload);
    let signature = identity.sign(&signing_input);
    socket
        .send(Message::Text(
            json!({
                "type": "advertise_candidates",
                "advertisement": advertisement.clone(),
                "signature_base64": URL_SAFE_NO_PAD.encode(signature.to_bytes())
            })
            .to_string()
            .into(),
        ))
        .await
        .expect("advertise candidates");
    let configuration = socket
        .next()
        .await
        .expect("configuration response")
        .expect("valid configuration response");
    let Message::Text(configuration) = configuration else {
        panic!("expected configuration text");
    };
    let configuration: Value =
        serde_json::from_str(&configuration).expect("configuration response JSON");
    assert_eq!(configuration["type"], "configuration");
    assert_eq!(configuration["configuration"]["version"], expected_version);
    let configuration_payload = URL_SAFE_NO_PAD
        .decode(
            configuration["configuration"]["payload_base64"]
                .as_str()
                .expect("configuration payload"),
        )
        .expect("decode configuration payload");
    let configuration_payload: Value =
        serde_json::from_slice(&configuration_payload).expect("configuration payload JSON");
    assert_eq!(
        configuration_payload["discovery_endpoints"][0],
        discovery_address.to_string()
    );
    let advertised_node = configuration_payload["nodes"]
        .as_array()
        .expect("configuration nodes")
        .iter()
        .find(|node| node["node_id_base64"] == enrollment["node_id_base64"])
        .expect("advertising node");
    assert_eq!(
        advertised_node["candidates"]
            .as_array()
            .expect("candidates")
            .len(),
        2
    );
    assert_eq!(advertised_node["candidates"][0]["priority"], 200);
    assert_eq!(advertised_node["candidates"][0]["kind"], "mapped");

    verify_idempotent_advertisement(socket, advertisement, signature, expected_version).await;
}

async fn verify_idempotent_advertisement(
    socket: &mut ControlSocket,
    advertisement: CandidateAdvertisement,
    signature: Signature,
    expected_version: u64,
) {
    socket
        .send(Message::Text(
            json!({
                "type": "advertise_candidates",
                "advertisement": advertisement,
                "signature_base64": URL_SAFE_NO_PAD.encode(signature.to_bytes())
            })
            .to_string()
            .into(),
        ))
        .await
        .expect("repeat identical candidate advertisement");
    let repeated = socket
        .next()
        .await
        .expect("idempotent configuration response")
        .expect("valid idempotent configuration response");
    let Message::Text(repeated) = repeated else {
        panic!("expected idempotent configuration text");
    };
    let repeated: Value = serde_json::from_str(&repeated).expect("idempotent response JSON");
    assert_eq!(repeated["type"], "configuration");
    assert_eq!(repeated["configuration"]["version"], expected_version);
}
