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
    ConfigurationSubnetRoute, EndpointCandidate, EndpointCandidateKind, EnrollResponse, PortRange,
    SignedConfiguration, SubnetRouteMode,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    c_config: Option<PathBuf>,
    a_virtual_ip: Ipv4Addr,
    b_virtual_ip: Ipv4Addr,
    #[serde(skip_serializing_if = "Option::is_none")]
    c_virtual_ip: Option<Ipv4Addr>,
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
    AclGate,
    SubnetNone,
    SubnetRouted,
    SubnetNat,
}

#[derive(Clone, Copy)]
struct FixturePolicyView {
    mode: FixturePolicyMode,
    receiver_view: bool,
}

struct FixtureConfigurationContext<'a> {
    network_id: Uuid,
    generated_at: chrono::DateTime<Utc>,
    credential_not_after: chrono::DateTime<Utc>,
    candidate_not_after: chrono::DateTime<Utc>,
    signing_key: &'a SigningKey,
}

struct FixturePersistenceContext<'a> {
    network_id: Uuid,
    credential_not_after: chrono::DateTime<Utc>,
    credential_key: &'a SigningKey,
    configuration_key: &'a SigningKey,
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
    let node_c = create_acl_node_c(&root, policy_mode, endpoint_a, endpoint_b)?;
    let credential_key = signing_key()?;
    let configuration_key = signing_key()?;
    let network_id = Uuid::new_v4();
    let now = Utc::now();
    let not_after = now + Duration::hours(2);
    let candidate_not_after = candidate_not_after(policy_mode, now, not_after)?;
    let configuration_context = FixtureConfigurationContext {
        network_id,
        generated_at: now,
        credential_not_after: not_after,
        candidate_not_after,
        signing_key: &configuration_key,
    };
    let mut nodes = vec![&node_a, &node_b];
    if let Some(node_c) = &node_c {
        nodes.push(node_c);
    }
    let (configuration_a, configuration_b) =
        signed_fixture_configurations(&configuration_context, &nodes, &node_b, policy_mode)?;
    let persistence = FixturePersistenceContext {
        network_id,
        credential_not_after: not_after,
        credential_key: &credential_key,
        configuration_key: &configuration_key,
    };
    persist_fixture_nodes(
        &persistence,
        &node_a,
        &node_b,
        node_c.as_ref(),
        &configuration_a,
        &configuration_b,
    )?;
    let manifest = fixture_manifest(&node_a, &node_b, node_c.as_ref());
    serde_json::to_writer(std::io::stdout().lock(), &manifest)?;
    Ok(())
}

fn create_acl_node_c(
    root: &Path,
    policy_mode: FixturePolicyMode,
    endpoint_a: SocketAddrV4,
    endpoint_b: SocketAddrV4,
) -> Result<Option<NodeFixture<'static>>, Box<dyn Error>> {
    acl_node_c_endpoint(policy_mode, endpoint_a, endpoint_b)?
        .map(|endpoint| {
            create_node(
                root,
                "node-c",
                "xsc0",
                Ipv4Addr::new(100, 127, 253, 3),
                endpoint,
                None,
                3,
            )
        })
        .transpose()
}

fn signed_fixture_configurations(
    context: &FixtureConfigurationContext<'_>,
    nodes: &[&NodeFixture<'_>],
    gateway: &NodeFixture<'_>,
    mode: FixturePolicyMode,
) -> Result<(SignedConfiguration, SignedConfiguration), Box<dyn Error>> {
    let configuration_a = signed_configuration(
        context,
        nodes,
        gateway,
        FixturePolicyView {
            mode,
            receiver_view: false,
        },
    )?;
    let configuration_b = if matches!(
        mode,
        FixturePolicyMode::AclMatrix | FixturePolicyMode::AclGate
    ) {
        signed_configuration(
            context,
            nodes,
            gateway,
            FixturePolicyView {
                mode,
                receiver_view: true,
            },
        )?
    } else {
        configuration_a.clone()
    };
    Ok((configuration_a, configuration_b))
}

fn persist_fixture_nodes(
    context: &FixturePersistenceContext<'_>,
    node_a: &NodeFixture<'_>,
    node_b: &NodeFixture<'_>,
    node_c: Option<&NodeFixture<'_>>,
    configuration_a: &SignedConfiguration,
    configuration_b: &SignedConfiguration,
) -> Result<(), Box<dyn Error>> {
    persist_node(
        node_a,
        context.network_id,
        context.credential_not_after,
        context.credential_key,
        context.configuration_key,
        configuration_a,
    )?;
    persist_node(
        node_b,
        context.network_id,
        context.credential_not_after,
        context.credential_key,
        context.configuration_key,
        configuration_b,
    )?;
    if let Some(node_c) = node_c {
        persist_node(
            node_c,
            context.network_id,
            context.credential_not_after,
            context.credential_key,
            context.configuration_key,
            configuration_a,
        )?;
    }
    Ok(())
}

fn fixture_manifest(
    node_a: &NodeFixture<'_>,
    node_b: &NodeFixture<'_>,
    node_c: Option<&NodeFixture<'_>>,
) -> FixtureManifest {
    FixtureManifest {
        a_config: node_a.root.join("agent.json"),
        b_config: node_b.root.join("agent.json"),
        c_config: node_c.map(|node| node.root.join("agent.json")),
        a_virtual_ip: node_a.virtual_ip,
        b_virtual_ip: node_b.virtual_ip,
        c_virtual_ip: node_c.map(|node| node.virtual_ip),
    }
}

fn policy_mode() -> Result<FixturePolicyMode, Box<dyn Error>> {
    match env::var("XS_FIXTURE_ACL_MODE").as_deref() {
        Err(env::VarError::NotPresent) | Ok("allow_all") => Ok(FixturePolicyMode::AllowAll),
        Ok("acl_matrix") => Ok(FixturePolicyMode::AclMatrix),
        Ok("acl_gate") => Ok(FixturePolicyMode::AclGate),
        Ok("subnet_none") => Ok(FixturePolicyMode::SubnetNone),
        Ok("subnet_routed") => Ok(FixturePolicyMode::SubnetRouted),
        Ok("subnet_nat") => Ok(FixturePolicyMode::SubnetNat),
        _ => Err("invalid XS_FIXTURE_ACL_MODE".into()),
    }
}

fn acl_node_c_endpoint(
    policy_mode: FixturePolicyMode,
    endpoint_a: SocketAddrV4,
    endpoint_b: SocketAddrV4,
) -> Result<Option<SocketAddrV4>, Box<dyn Error>> {
    if policy_mode != FixturePolicyMode::AclGate {
        if env::var_os("XS_FIXTURE_NODE_C_ENDPOINT").is_some() {
            return Err("XS_FIXTURE_NODE_C_ENDPOINT requires acl_gate mode".into());
        }
        return Ok(None);
    }
    let endpoint = env::var("XS_FIXTURE_NODE_C_ENDPOINT")?.parse::<SocketAddrV4>()?;
    if endpoint.port() == 0 || endpoint == endpoint_a || endpoint == endpoint_b {
        return Err("invalid node C endpoint".into());
    }
    Ok(Some(endpoint))
}

fn candidate_not_after(
    policy_mode: FixturePolicyMode,
    now: chrono::DateTime<Utc>,
    default: chrono::DateTime<Utc>,
) -> Result<chrono::DateTime<Utc>, Box<dyn Error>> {
    if !matches!(
        policy_mode,
        FixturePolicyMode::SubnetNone
            | FixturePolicyMode::SubnetRouted
            | FixturePolicyMode::SubnetNat
    ) {
        return Ok(default);
    }
    let seconds = env::var("XS_FIXTURE_CANDIDATE_TTL_SECONDS")
        .unwrap_or_else(|_| "30".to_owned())
        .parse::<i64>()?;
    if !(5..=300).contains(&seconds) {
        return Err("invalid XS_FIXTURE_CANDIDATE_TTL_SECONDS".into());
    }
    Ok(now + Duration::seconds(seconds))
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
    context: &FixtureConfigurationContext<'_>,
    nodes: &[&NodeFixture<'_>],
    gateway: &NodeFixture<'_>,
    policy_view: FixturePolicyView,
) -> Result<SignedConfiguration, Box<dyn Error>> {
    let nodes = nodes
        .iter()
        .copied()
        .map(|node| {
            let groups = match policy_view.mode {
                FixturePolicyMode::AllowAll => vec!["test-nodes".to_owned()],
                FixturePolicyMode::AclMatrix
                | FixturePolicyMode::SubnetNone
                | FixturePolicyMode::SubnetRouted
                | FixturePolicyMode::SubnetNat
                    if node.name == "node-a" =>
                {
                    vec!["clients".to_owned()]
                }
                FixturePolicyMode::AclMatrix => vec!["servers".to_owned()],
                FixturePolicyMode::AclGate if node.name == "node-a" => {
                    vec!["acl-clients".to_owned()]
                }
                FixturePolicyMode::AclGate if node.name == "node-b" => {
                    vec!["acl-servers".to_owned()]
                }
                FixturePolicyMode::AclGate => vec!["acl-denied".to_owned()],
                FixturePolicyMode::SubnetNone
                | FixturePolicyMode::SubnetRouted
                | FixturePolicyMode::SubnetNat => vec!["gateways".to_owned()],
            };
            let subnet_mode = matches!(
                policy_view.mode,
                FixturePolicyMode::SubnetNone
                    | FixturePolicyMode::SubnetRouted
                    | FixturePolicyMode::SubnetNat
            );
            ConfigurationNode {
                node_id_base64: URL_SAFE_NO_PAD.encode(node_id(&node.identity.public_key())),
                identity_public_key_base64: URL_SAFE_NO_PAD.encode(node.identity.public_key()),
                virtual_ip: node.virtual_ip.to_string(),
                direct_endpoints: if subnet_mode {
                    Vec::new()
                } else {
                    vec![node.endpoint.to_string()]
                },
                candidates: node
                    .candidates
                    .iter()
                    .enumerate()
                    .map(|(index, endpoint)| EndpointCandidate {
                        kind: EndpointCandidateKind::Static,
                        endpoint: (*endpoint).into(),
                        priority: 200_u32.saturating_sub(u32::try_from(index).unwrap_or(u32::MAX)),
                        expires_at: context.candidate_not_after,
                    })
                    .collect(),
                credential_serial: node.credential_serial,
                credential_not_after: context.credential_not_after,
                role_bitmap: 1,
                groups,
                tags: vec!["linux".to_owned()],
                update_channel: None,
            }
        })
        .collect();
    let payload = ConfigurationPayload {
        schema_version: 1,
        network_id: context.network_id,
        version: 1,
        policy_version: 1,
        generated_at: context.generated_at,
        address_pool: ADDRESS_POOL.to_owned(),
        discovery_endpoints: Vec::new(),
        nodes,
        relays: Vec::new(),
        policies: fixture_policies(policy_view),
        subnet_routes: fixture_subnet_routes(policy_view, gateway),
    };
    let payload_bytes = serde_json::to_vec(&payload)?;
    let mut signing_input = Vec::with_capacity(CONFIGURATION_DOMAIN.len() + payload_bytes.len());
    signing_input.extend_from_slice(CONFIGURATION_DOMAIN);
    signing_input.extend_from_slice(&payload_bytes);
    let signature = context.signing_key.sign(&signing_input);
    Ok(SignedConfiguration {
        version: 1,
        payload_base64: URL_SAFE_NO_PAD.encode(payload_bytes),
        signature_base64: URL_SAFE_NO_PAD.encode(signature.to_bytes()),
        signer_key_id: controller_key_id(&context.signing_key.verifying_key()),
    })
}

fn fixture_policies(policy_view: FixturePolicyView) -> Vec<AclRule> {
    match policy_view.mode {
        FixturePolicyMode::AllowAll => vec![rule(
            "allow-test-network",
            1,
            AclAction::Allow,
            "test-nodes",
            "test-nodes",
            AclProtocol::Any,
            Vec::new(),
        )],
        FixturePolicyMode::AclMatrix => {
            let mut rules = acl_deny_rules(policy_view.receiver_view);
            rules.extend(acl_allow_rules());
            rules
        }
        FixturePolicyMode::AclGate => acl_gate_rules(policy_view.receiver_view),
        FixturePolicyMode::SubnetNone => Vec::new(),
        FixturePolicyMode::SubnetRouted | FixturePolicyMode::SubnetNat => vec![
            AclRule {
                id: "allow-client-subnet-icmp".to_owned(),
                priority: 230,
                action: AclAction::Allow,
                sources: vec![AclSelector::Group {
                    name: "clients".to_owned(),
                }],
                destinations: vec![AclSelector::Subnet {
                    cidr: "192.168.232.0/24".to_owned(),
                }],
                protocol: AclProtocol::Icmp,
                destination_ports: Vec::new(),
            },
            AclRule {
                id: "allow-client-subnet-tcp".to_owned(),
                priority: 220,
                action: AclAction::Allow,
                sources: vec![AclSelector::Group {
                    name: "clients".to_owned(),
                }],
                destinations: vec![AclSelector::Subnet {
                    cidr: "192.168.232.0/24".to_owned(),
                }],
                protocol: AclProtocol::Tcp,
                destination_ports: vec![
                    PortRange {
                        start: 43221,
                        end: 43221,
                    },
                    PortRange {
                        start: 43223,
                        end: 43223,
                    },
                ],
            },
            AclRule {
                id: "allow-subnet-client-icmp".to_owned(),
                priority: 210,
                action: AclAction::Allow,
                sources: vec![AclSelector::Subnet {
                    cidr: "192.168.232.0/24".to_owned(),
                }],
                destinations: vec![AclSelector::Group {
                    name: "clients".to_owned(),
                }],
                protocol: AclProtocol::Icmp,
                destination_ports: Vec::new(),
            },
            AclRule {
                id: "allow-subnet-client-tcp".to_owned(),
                priority: 200,
                action: AclAction::Allow,
                sources: vec![AclSelector::Subnet {
                    cidr: "192.168.232.0/24".to_owned(),
                }],
                destinations: vec![AclSelector::Group {
                    name: "clients".to_owned(),
                }],
                protocol: AclProtocol::Tcp,
                destination_ports: Vec::new(),
            },
        ],
    }
}

fn fixture_subnet_routes(
    policy_view: FixturePolicyView,
    gateway: &NodeFixture<'_>,
) -> Vec<ConfigurationSubnetRoute> {
    let mode = match policy_view.mode {
        FixturePolicyMode::SubnetRouted => SubnetRouteMode::Routed,
        FixturePolicyMode::SubnetNat => SubnetRouteMode::Nat,
        FixturePolicyMode::AllowAll
        | FixturePolicyMode::AclMatrix
        | FixturePolicyMode::AclGate
        | FixturePolicyMode::SubnetNone => return Vec::new(),
    };
    vec![ConfigurationSubnetRoute {
        route_id: "namespace-subnet-route".to_owned(),
        prefix: "192.168.232.0/24".to_owned(),
        gateway_node_id_base64: URL_SAFE_NO_PAD.encode(node_id(&gateway.identity.public_key())),
        mode,
        interface_name: "xsm32lanb".to_owned(),
        priority: 100,
    }]
}

fn acl_gate_rules(receiver_view: bool) -> Vec<AclRule> {
    let mut rules = acl_gate_receiver_denies(receiver_view);
    rules.extend(acl_gate_direction_denies());
    rules.extend(acl_gate_client_allows());
    rules.extend(acl_gate_server_allows());
    rules
}

fn acl_gate_receiver_denies(receiver_view: bool) -> Vec<AclRule> {
    if receiver_view {
        vec![
            rule(
                "deny-receiver-only-tcp",
                900,
                AclAction::Deny,
                "acl-clients",
                "acl-servers",
                AclProtocol::Tcp,
                vec![PortRange {
                    start: 43315,
                    end: 43315,
                }],
            ),
            rule(
                "deny-receiver-only-udp",
                890,
                AclAction::Deny,
                "acl-clients",
                "acl-servers",
                AclProtocol::Udp,
                vec![PortRange {
                    start: 43316,
                    end: 43316,
                }],
            ),
        ]
    } else {
        Vec::new()
    }
}

fn acl_gate_direction_denies() -> Vec<AclRule> {
    vec![
        rule(
            "deny-client-c",
            800,
            AclAction::Deny,
            "acl-clients",
            "acl-denied",
            AclProtocol::Any,
            Vec::new(),
        ),
        rule(
            "deny-c-server",
            790,
            AclAction::Deny,
            "acl-denied",
            "acl-servers",
            AclProtocol::Any,
            Vec::new(),
        ),
    ]
}

fn acl_gate_client_allows() -> Vec<AclRule> {
    vec![
        rule(
            "allow-client-server-icmp",
            700,
            AclAction::Allow,
            "acl-clients",
            "acl-servers",
            AclProtocol::Icmp,
            Vec::new(),
        ),
        rule(
            "allow-client-server-tcp",
            600,
            AclAction::Allow,
            "acl-clients",
            "acl-servers",
            AclProtocol::Tcp,
            vec![
                PortRange {
                    start: 43311,
                    end: 43311,
                },
                PortRange {
                    start: 43315,
                    end: 43315,
                },
            ],
        ),
        rule(
            "allow-client-server-udp",
            590,
            AclAction::Allow,
            "acl-clients",
            "acl-servers",
            AclProtocol::Udp,
            vec![
                PortRange {
                    start: 43313,
                    end: 43313,
                },
                PortRange {
                    start: 43316,
                    end: 43316,
                },
            ],
        ),
    ]
}

fn acl_gate_server_allows() -> Vec<AclRule> {
    vec![
        rule(
            "allow-server-client-icmp",
            500,
            AclAction::Allow,
            "acl-servers",
            "acl-clients",
            AclProtocol::Icmp,
            Vec::new(),
        ),
        rule(
            "allow-server-client-tcp",
            490,
            AclAction::Allow,
            "acl-servers",
            "acl-clients",
            AclProtocol::Tcp,
            Vec::new(),
        ),
        rule(
            "allow-server-client-udp",
            480,
            AclAction::Allow,
            "acl-servers",
            "acl-clients",
            AclProtocol::Udp,
            Vec::new(),
        ),
    ]
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
