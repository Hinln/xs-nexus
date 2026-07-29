use std::{
    error::Error,
    net::{Ipv4Addr, SocketAddrV4},
    path::{Path, PathBuf},
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{Duration, Utc};
use ed25519_dalek::{Signer as _, SigningKey};
use getrandom::fill;
use serde::Serialize;
use uuid::Uuid;
use xs_agent::{
    state::NodeState,
    storage::{Identity, write_json},
};
use xs_core::{ConfigurationNode, ConfigurationPayload, EnrollResponse, SignedConfiguration};
use xs_protocol::{CredentialClaims, controller_key_id, node_id, role_set_digest, sign_credential};

const CONTROLLER_URL: &str = "http://127.0.0.1:9/";
const ADDRESS_POOL: &str = "100.127.253.0/24";
const CONFIGURATION_DOMAIN: &[u8] = b"XS Nexus configuration v1";

#[derive(Serialize)]
struct FixtureAgentConfig {
    controller_url: String,
    node_name: String,
    device_type: String,
    state_directory: PathBuf,
    runtime_directory: PathBuf,
    interface_name: String,
    mtu: u16,
    control_sync_interval_seconds: u64,
}

#[derive(Serialize)]
struct FixtureManifest {
    a_config: PathBuf,
    b_config: PathBuf,
    a_virtual_ip: Ipv4Addr,
    b_virtual_ip: Ipv4Addr,
}

struct NodeFixture<'a> {
    name: &'a str,
    interface_name: &'a str,
    root: PathBuf,
    identity: Identity,
    virtual_ip: Ipv4Addr,
    endpoint: SocketAddrV4,
    credential_serial: u64,
}

fn main() -> Result<(), Box<dyn Error>> {
    let (root, endpoint_a, endpoint_b) = arguments()?;
    let node_a = create_node(
        &root,
        "node-a",
        "xsa0",
        Ipv4Addr::new(100, 127, 253, 1),
        endpoint_a,
        1,
    )?;
    let node_b = create_node(
        &root,
        "node-b",
        "xsb0",
        Ipv4Addr::new(100, 127, 253, 2),
        endpoint_b,
        2,
    )?;
    let credential_key = signing_key()?;
    let configuration_key = signing_key()?;
    let network_id = Uuid::new_v4();
    let now = Utc::now();
    let not_after = now + Duration::hours(2);
    let configuration = signed_configuration(
        network_id,
        now,
        not_after,
        &node_a,
        &node_b,
        &configuration_key,
    )?;

    persist_node(
        &node_a,
        network_id,
        not_after,
        &credential_key,
        &configuration_key,
        &configuration,
    )?;
    persist_node(
        &node_b,
        network_id,
        not_after,
        &credential_key,
        &configuration_key,
        &configuration,
    )?;

    let manifest = FixtureManifest {
        a_config: node_a.root.join("agent.json"),
        b_config: node_b.root.join("agent.json"),
        a_virtual_ip: node_a.virtual_ip,
        b_virtual_ip: node_b.virtual_ip,
    };
    serde_json::to_writer(std::io::stdout().lock(), &manifest)?;
    Ok(())
}

fn arguments() -> Result<(PathBuf, SocketAddrV4, SocketAddrV4), Box<dyn Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let root = PathBuf::from(arguments.next().ok_or("missing fixture root")?);
    let endpoint_a = arguments
        .next()
        .ok_or("missing node A endpoint")?
        .into_string()
        .map_err(|_| "node A endpoint is not UTF-8")?
        .parse::<SocketAddrV4>()?;
    let endpoint_b = arguments
        .next()
        .ok_or("missing node B endpoint")?
        .into_string()
        .map_err(|_| "node B endpoint is not UTF-8")?
        .parse::<SocketAddrV4>()?;
    if arguments.next().is_some()
        || !root.is_absolute()
        || endpoint_a == endpoint_b
        || endpoint_a.port() == 0
        || endpoint_b.port() == 0
    {
        return Err("invalid fixture arguments".into());
    }
    Ok((root, endpoint_a, endpoint_b))
}

fn create_node(
    root: &Path,
    name: &'static str,
    interface_name: &'static str,
    virtual_ip: Ipv4Addr,
    endpoint: SocketAddrV4,
    credential_serial: u64,
) -> Result<NodeFixture<'static>, Box<dyn Error>> {
    let node_root = root.join(name);
    let state_directory = node_root.join("state");
    let identity = Identity::load_or_create(&state_directory.join("identity.key"))?;
    Ok(NodeFixture {
        name,
        interface_name,
        root: node_root,
        identity,
        virtual_ip,
        endpoint,
        credential_serial,
    })
}

fn signed_configuration(
    network_id: Uuid,
    generated_at: chrono::DateTime<Utc>,
    credential_not_after: chrono::DateTime<Utc>,
    node_a: &NodeFixture<'_>,
    node_b: &NodeFixture<'_>,
    configuration_key: &SigningKey,
) -> Result<SignedConfiguration, Box<dyn Error>> {
    let nodes = [node_a, node_b]
        .into_iter()
        .map(|node| ConfigurationNode {
            node_id_base64: URL_SAFE_NO_PAD.encode(node_id(&node.identity.public_key())),
            identity_public_key_base64: URL_SAFE_NO_PAD.encode(node.identity.public_key()),
            virtual_ip: node.virtual_ip.to_string(),
            direct_endpoints: vec![node.endpoint.to_string()],
            credential_serial: node.credential_serial,
            credential_not_after,
            role_bitmap: 1,
            tags: vec!["linux".to_owned()],
        })
        .collect();
    let payload = ConfigurationPayload {
        schema_version: 1,
        network_id,
        version: 1,
        generated_at,
        address_pool: ADDRESS_POOL.to_owned(),
        nodes,
        relays: Vec::new(),
        policies: Vec::new(),
    };
    let payload_bytes = serde_json::to_vec(&payload)?;
    let mut signing_input = Vec::with_capacity(CONFIGURATION_DOMAIN.len() + payload_bytes.len());
    signing_input.extend_from_slice(CONFIGURATION_DOMAIN);
    signing_input.extend_from_slice(&payload_bytes);
    let signature = configuration_key.sign(&signing_input);
    Ok(SignedConfiguration {
        version: 1,
        payload_base64: URL_SAFE_NO_PAD.encode(payload_bytes),
        signature_base64: URL_SAFE_NO_PAD.encode(signature.to_bytes()),
        signer_key_id: controller_key_id(&configuration_key.verifying_key()),
    })
}

fn persist_node(
    node: &NodeFixture<'_>,
    network_id: Uuid,
    credential_not_after: chrono::DateTime<Utc>,
    credential_key: &SigningKey,
    configuration_key: &SigningKey,
    configuration: &SignedConfiguration,
) -> Result<(), Box<dyn Error>> {
    let now = u64::try_from(Utc::now().timestamp())?;
    let credential = sign_credential(
        CredentialClaims {
            network_id: *network_id.as_bytes(),
            identity_public_key: node.identity.public_key(),
            virtual_ipv4: node.virtual_ip,
            serial: node.credential_serial,
            not_before: now.saturating_sub(60),
            not_after: u64::try_from(credential_not_after.timestamp())?,
            role_bitmap: 1,
            role_set_digest: role_set_digest(1, &["linux".to_owned()])?,
        },
        credential_key,
    );
    let response = EnrollResponse {
        network_id,
        node_id_base64: URL_SAFE_NO_PAD.encode(node_id(&node.identity.public_key())),
        virtual_ip: node.virtual_ip.to_string(),
        credential_base64: URL_SAFE_NO_PAD.encode(credential),
        credential_key_id: controller_key_id(&credential_key.verifying_key()),
        credential_signing_public_key_base64: URL_SAFE_NO_PAD
            .encode(credential_key.verifying_key().to_bytes()),
        configuration_signing_public_key_base64: URL_SAFE_NO_PAD
            .encode(configuration_key.verifying_key().to_bytes()),
        configuration: configuration.clone(),
    };
    let state = NodeState::from_enrollment(response, &node.identity, CONTROLLER_URL)?;
    let state_directory = node.root.join("state");
    let runtime_directory = node.root.join("run");
    write_json(&state_directory.join("node-state.json"), &state)?;
    write_json(
        &node.root.join("agent.json"),
        &FixtureAgentConfig {
            controller_url: CONTROLLER_URL.to_owned(),
            node_name: node.name.to_owned(),
            device_type: "linux".to_owned(),
            state_directory,
            runtime_directory,
            interface_name: node.interface_name.to_owned(),
            mtu: 1280,
            control_sync_interval_seconds: 5,
        },
    )?;
    Ok(())
}

fn signing_key() -> Result<SigningKey, Box<dyn Error>> {
    let mut seed = [0_u8; 32];
    fill(&mut seed)?;
    Ok(SigningKey::from_bytes(&seed))
}
