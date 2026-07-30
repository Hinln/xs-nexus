use std::{
    env,
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
use xs_core::{
    AclAction, AclProtocol, AclRule, AclSelector, ConfigurationNode, ConfigurationPayload,
    EndpointCandidate, EndpointCandidateKind, EnrollResponse, PortRange, SignedConfiguration,
};
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
    candidates: Vec<SocketAddrV4>,
    credential_serial: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FixturePolicyMode {
    AllowAll,
    AclMatrix,
}

#[derive(Clone, Copy)]
struct FixturePolicyView {
    mode: FixturePolicyMode,
    receiver_view: bool,
}

type FixtureArguments = (
    PathBuf,
    SocketAddrV4,
    SocketAddrV4,
    Option<SocketAddrV4>,
    Option<SocketAddrV4>,
);

fn main() -> Result<(), Box<dyn Error>> {
    let (root, endpoint_a, endpoint_b, preferred_a, preferred_b) = arguments()?;
    let policy_mode = policy_mode()?;
    let node_a = create_node(
        &root,
        "node-a",
        "xsa0",
        Ipv4Addr::new(100, 127, 253, 1),
        endpoint_a,
        preferred_a,
        1,
    )?;
    let node_b = create_node(
        &root,
        "node-b",
        "xsb0",
        Ipv4Addr::new(100, 127, 253, 2),
        endpoint_b,
        preferred_b,
        2,
    )?;
    let credential_key = signing_key()?;
    let configuration_key = signing_key()?;
    let network_id = Uuid::new_v4();
    let now = Utc::now();
    let not_after = now + Duration::hours(2);
    let configuration_a = signed_configuration(
        network_id,
        now,
        not_after,
        &node_a,
        &node_b,
        &configuration_key,
        FixturePolicyView {
            mode: policy_mode,
            receiver_view: false,
        },
    )?;
    let configuration_b = if policy_mode == FixturePolicyMode::AclMatrix {
        signed_configuration(
            network_id,
            now,
            not_after,
            &node_a,
            &node_b,
            &configuration_key,
            FixturePolicyView {
                mode: policy_mode,
                receiver_view: true,
            },
        )?
    } else {
        configuration_a.clone()
    };

    persist_node(
        &node_a,
        network_id,
        not_after,
        &credential_key,
        &configuration_key,
        &configuration_a,
    )?;
    persist_node(
        &node_b,
        network_id,
        not_after,
        &credential_key,
        &configuration_key,
        &configuration_b,
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

fn policy_mode() -> Result<FixturePolicyMode, Box<dyn Error>> {
    match env::var("XS_FIXTURE_ACL_MODE").as_deref() {
        Err(env::VarError::NotPresent) | Ok("allow_all") => Ok(FixturePolicyMode::AllowAll),
        Ok("acl_matrix") => Ok(FixturePolicyMode::AclMatrix),
        _ => Err("invalid XS_FIXTURE_ACL_MODE".into()),
    }
}

fn arguments() -> Result<FixtureArguments, Box<dyn Error>> {
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
    let preferred_a = if let Some(value) = arguments.next() {
        Some(
            value
                .into_string()
                .map_err(|_| "node A preferred endpoint is not UTF-8")?
                .parse::<SocketAddrV4>()?,
        )
    } else {
        None
    };
    let preferred_b = if let Some(value) = arguments.next() {
        Some(
            value
                .into_string()
                .map_err(|_| "node B preferred endpoint is not UTF-8")?
                .parse::<SocketAddrV4>()?,
        )
    } else {
        None
    };
    if arguments.next().is_some()
        || preferred_a.is_some() != preferred_b.is_some()
        || !root.is_absolute()
        || endpoint_a == endpoint_b
        || endpoint_a.port() == 0
        || endpoint_b.port() == 0
    {
        return Err("invalid fixture arguments".into());
    }
    Ok((root, endpoint_a, endpoint_b, preferred_a, preferred_b))
}

fn create_node(
    root: &Path,
    name: &'static str,
    interface_name: &'static str,
    virtual_ip: Ipv4Addr,
    endpoint: SocketAddrV4,
    preferred_endpoint: Option<SocketAddrV4>,
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
        candidates: preferred_endpoint.into_iter().chain([endpoint]).collect(),
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
    policy_view: FixturePolicyView,
) -> Result<SignedConfiguration, Box<dyn Error>> {
    let nodes = [node_a, node_b]
        .into_iter()
        .map(|node| {
            let groups = match policy_view.mode {
                FixturePolicyMode::AllowAll => vec!["test-nodes".to_owned()],
                FixturePolicyMode::AclMatrix if node.name == "node-a" => {
                    vec!["clients".to_owned()]
                }
                FixturePolicyMode::AclMatrix => vec!["servers".to_owned()],
            };
            ConfigurationNode {
                node_id_base64: URL_SAFE_NO_PAD.encode(node_id(&node.identity.public_key())),
                identity_public_key_base64: URL_SAFE_NO_PAD.encode(node.identity.public_key()),
                virtual_ip: node.virtual_ip.to_string(),
                direct_endpoints: vec![node.endpoint.to_string()],
                candidates: node
                    .candidates
                    .iter()
                    .enumerate()
                    .map(|(index, endpoint)| EndpointCandidate {
                        kind: EndpointCandidateKind::Static,
                        endpoint: (*endpoint).into(),
                        priority: 200_u32.saturating_sub(u32::try_from(index).unwrap_or(u32::MAX)),
                        expires_at: credential_not_after,
                    })
                    .collect(),
                credential_serial: node.credential_serial,
                credential_not_after,
                role_bitmap: 1,
                groups,
                tags: vec!["linux".to_owned()],
            }
        })
        .collect();
    let payload = ConfigurationPayload {
        schema_version: 1,
        network_id,
        version: 1,
        policy_version: 1,
        generated_at,
        address_pool: ADDRESS_POOL.to_owned(),
        discovery_endpoints: Vec::new(),
        nodes,
        relays: Vec::new(),
        policies: fixture_policies(policy_view),
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

fn fixture_policies(policy_view: FixturePolicyView) -> Vec<AclRule> {
    if policy_view.mode == FixturePolicyMode::AllowAll {
        return vec![rule(
            "allow-test-network",
            1,
            AclAction::Allow,
            "test-nodes",
            "test-nodes",
            AclProtocol::Any,
            Vec::new(),
        )];
    }
    let mut rules = acl_deny_rules(policy_view.receiver_view);
    rules.extend(acl_allow_rules());
    rules
}

fn acl_deny_rules(receiver_view: bool) -> Vec<AclRule> {
    let mut rules = vec![
        rule(
            "deny-blocked-tcp",
            500,
            AclAction::Deny,
            "clients",
            "servers",
            AclProtocol::Tcp,
            vec![PortRange {
                start: 43112,
                end: 43112,
            }],
        ),
        rule(
            "deny-blocked-udp",
            490,
            AclAction::Deny,
            "clients",
            "servers",
            AclProtocol::Udp,
            vec![PortRange {
                start: 43114,
                end: 43114,
            }],
        ),
    ];
    if receiver_view {
        rules.push(rule(
            "deny-receiver-only-udp",
            480,
            AclAction::Deny,
            "clients",
            "servers",
            AclProtocol::Udp,
            vec![PortRange {
                start: 43115,
                end: 43115,
            }],
        ));
    }
    rules
}

fn acl_allow_rules() -> Vec<AclRule> {
    vec![
        rule(
            "allow-client-server-icmp",
            400,
            AclAction::Allow,
            "clients",
            "servers",
            AclProtocol::Icmp,
            Vec::new(),
        ),
        rule(
            "allow-server-client-icmp",
            390,
            AclAction::Allow,
            "servers",
            "clients",
            AclProtocol::Icmp,
            Vec::new(),
        ),
        rule(
            "allow-client-server-tcp",
            300,
            AclAction::Allow,
            "clients",
            "servers",
            AclProtocol::Tcp,
            vec![PortRange {
                start: 43111,
                end: 43111,
            }],
        ),
        rule(
            "allow-server-client-tcp",
            290,
            AclAction::Allow,
            "servers",
            "clients",
            AclProtocol::Tcp,
            Vec::new(),
        ),
        rule(
            "allow-client-server-udp",
            200,
            AclAction::Allow,
            "clients",
            "servers",
            AclProtocol::Udp,
            vec![
                PortRange {
                    start: 43113,
                    end: 43113,
                },
                PortRange {
                    start: 43115,
                    end: 43115,
                },
            ],
        ),
        rule(
            "allow-server-client-udp",
            190,
            AclAction::Allow,
            "servers",
            "clients",
            AclProtocol::Udp,
            Vec::new(),
        ),
    ]
}

fn rule(
    id: &str,
    priority: u32,
    action: AclAction,
    source_group: &str,
    destination_group: &str,
    protocol: AclProtocol,
    destination_ports: Vec<PortRange>,
) -> AclRule {
    AclRule {
        id: id.to_owned(),
        priority,
        action,
        sources: vec![AclSelector::Group {
            name: source_group.to_owned(),
        }],
        destinations: vec![AclSelector::Group {
            name: destination_group.to_owned(),
        }],
        protocol,
        destination_ports,
    }
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
