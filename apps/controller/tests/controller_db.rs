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
use tokio::{
    net::{TcpStream, UdpSocket},
    sync::watch,
    time::timeout,
};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};
use tower::ServiceExt;
use uuid::Uuid;
use xs_controller::config::ControllerConfig;
use xs_core::{CandidateAdvertisement, EndpointCandidate, EndpointCandidateKind};
use xs_protocol::{
    CREDENTIAL_LENGTH, DiscoveryRequest, verify_credential, verify_discovery_response,
};

const ADMIN_TOKEN: &str = "integration-admin-token-with-32-characters";
const CONFIGURATION_DOMAIN: &[u8] = b"XS Nexus configuration v1";
const CONTROL_AUTHENTICATION_DOMAIN: &[u8] = b"XS Nexus control authentication v1";
const CANDIDATE_ADVERTISEMENT_DOMAIN: &[u8] = b"XS Nexus candidate advertisement v1";
type ControlSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

#[tokio::test]
async fn controller_registration_ipam_configuration_and_control_flow() {
    let discovery_socket = UdpSocket::bind(("127.0.0.1", 0))
        .await
        .expect("bind discovery socket");
    let discovery_address = discovery_socket.local_addr().expect("discovery address");
    let config = test_config(discovery_address);
    let (router, state) = xs_controller::build(&config)
        .await
        .expect("controller database initializes");
    reset_database(&state.pool).await;
    let (discovery_shutdown, discovery_shutdown_rx) = watch::channel(false);
    let discovery_state = state.clone();
    let discovery_server = tokio::spawn(async move {
        xs_controller::discovery::serve(discovery_socket, discovery_state, discovery_shutdown_rx)
            .await
    });

    let (status, ready) = request_json(&router, Method::GET, "/health/ready", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ready["database"], "ok");

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
    assert_overlapping_network_rejected(&router).await;

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
    verify_websocket_control(&router, &enrollment, &identity, discovery_address).await;
    assert_acl_policy_explain_revoke_and_cooldown(
        &router,
        &state.pool,
        network_id,
        &enrollment,
        &manual_enrollment,
    )
    .await;

    assert_control_audit_and_token_use(&state.pool, &token_id).await;

    discovery_shutdown
        .send(true)
        .expect("request discovery shutdown");
    discovery_server
        .await
        .expect("join discovery server")
        .expect("discovery server exits cleanly");
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

async fn assert_control_audit_and_token_use(pool: &sqlx::PgPool, token_id: &str) {
    let control_audits: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_events WHERE action = 'control.authenticate'",
    )
    .fetch_one(pool)
    .await
    .expect("control audit count");
    assert_eq!(control_audits, 1);

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
        admin_token_hash: Sha256::digest(ADMIN_TOKEN.as_bytes()).into(),
        credential_signing_key: SigningKey::from_bytes(&[21_u8; 32]),
        config_signing_key: SigningKey::from_bytes(&[22_u8; 32]),
        credential_ttl_seconds: 86_400,
        relays: Vec::new(),
    }
}

async fn reset_database(pool: &sqlx::PgPool) {
    sqlx::query(
        "TRUNCATE audit_events, configuration_versions, acl_rules,
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
    assert_eq!(authenticated["configuration"]["version"], 4);

    socket
        .send(Message::Text(
            json!({"type": "sync", "last_version": 4})
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
    assert_eq!(synchronized["version"], 4);

    advertise_candidates_and_verify(&mut socket, enrollment, identity, discovery_address).await;

    socket.close(None).await.expect("close websocket");
    server.abort();
}

async fn advertise_candidates_and_verify(
    socket: &mut ControlSocket,
    enrollment: &Value,
    identity: &SigningKey,
    discovery_address: SocketAddr,
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
    assert_eq!(configuration["configuration"]["version"], 5);
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

    verify_idempotent_advertisement(socket, advertisement, signature).await;
}

async fn verify_idempotent_advertisement(
    socket: &mut ControlSocket,
    advertisement: CandidateAdvertisement,
    signature: Signature,
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
    assert_eq!(repeated["configuration"]["version"], 5);
}
