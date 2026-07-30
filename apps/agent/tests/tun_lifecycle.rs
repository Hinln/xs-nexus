#![cfg(target_os = "linux")]

use std::{
    net::IpAddr,
    net::Ipv4Addr,
    path::Path,
    process::{Command, Stdio},
    time::Duration,
};

use chrono::Utc;
use futures_util::TryStreamExt as _;
use ipnet::Ipv4Net;
use rtnetlink::{
    Handle, LinkDummy, RouteMessageBuilder, new_connection,
    packet_route::{
        address::AddressAttribute,
        link::{LinkAttribute, LinkFlags},
        route::{RouteAddress, RouteAttribute, RouteMessage},
    },
};
use tokio::task::JoinHandle;
use uuid::Uuid;
use xs_agent::{
    network::{NetworkPlan, TunNetwork},
    state::NodeState,
};
use xs_core::{
    ConfigurationNode, ConfigurationPayload, ConfigurationSubnetRoute, EndpointCandidate,
    EndpointCandidateKind, SignedConfiguration, SubnetRouteMode,
};

#[tokio::test]
async fn tun_lifecycle_is_scoped_and_recovers_after_drop() {
    let plan = NetworkPlan::new(
        "xstest0".to_owned(),
        1280,
        Ipv4Addr::new(100, 127, 250, 16),
        "100.127.250.0/24".parse().expect("test address pool"),
    )
    .expect("safe network plan");
    let temporary = tempfile::tempdir().expect("temporary network state");
    let manifest = temporary.path().join("network-manifest.json");
    let (handle, connection) = netlink();
    let default_routes = default_route_fingerprints(&handle).await;
    assert_virtual_pool_conflict_rejected(&handle, &plan, &manifest).await;

    let mut network = TunNetwork::create(plan.clone(), &manifest)
        .await
        .expect("create TUN network");
    assert!(manifest.is_file());
    assert_link(&handle, network.interface_index(), &plan).await;
    assert_address(&handle, network.interface_index(), plan.virtual_ip()).await;
    assert_route(&handle, network.interface_index(), &plan).await;
    assert_eq!(default_routes, default_route_fingerprints(&handle).await);

    let (subnet, mut route_state) = assert_client_route_lifecycle(&mut network, &handle).await;
    assert_gateway_route_lifecycle(&mut network, &handle, &plan, subnet, &mut route_state).await;

    assert_subnet_route_conflict_rejected(&mut network, &handle, subnet, &mut route_state).await;

    network.shutdown().await.expect("clean network shutdown");
    wait_for_absent_link(&handle, plan.interface_name()).await;
    assert!(!manifest.exists());
    assert_eq!(default_routes, default_route_fingerprints(&handle).await);
    assert!(!route_exists(&handle, 0, subnet).await);

    assert_stale_gateway_recovery(&handle, &plan, &manifest, &default_routes, subnet).await;
    connection.abort();
}

async fn assert_virtual_pool_conflict_rejected(
    handle: &Handle,
    plan: &NetworkPlan,
    manifest: &Path,
) {
    handle
        .link()
        .add(LinkDummy::new("xsconflict0").up().build())
        .execute()
        .await
        .expect("create conflicting dummy interface");
    let conflict_index = link_index(handle, "xsconflict0").await;
    handle
        .address()
        .add(
            conflict_index,
            IpAddr::V4(Ipv4Addr::new(100, 127, 250, 129)),
            25,
        )
        .execute()
        .await
        .expect("install conflicting connected route");
    assert!(
        TunNetwork::create(plan.clone(), manifest).await.is_err(),
        "overlapping system route must reject TUN creation"
    );
    assert!(!manifest.exists());
    assert_eq!(
        link_index_optional(handle, plan.interface_name()).await,
        None
    );
    remove_dummy_link(handle, conflict_index, "xsconflict0").await;
}

async fn assert_client_route_lifecycle(
    network: &mut TunNetwork,
    handle: &Handle,
) -> (Ipv4Net, NodeState) {
    let subnet = "192.168.203.0/24".parse().expect("approved subnet");
    let mut route_state = subnet_route_state(subnet, "local-node", true);
    network
        .reconcile_subnet_routes(&route_state)
        .await
        .expect("install approved subnet route");
    assert!(route_exists(handle, network.interface_index(), subnet).await);
    route_state.configuration_payload.nodes[1].candidates[0].expires_at =
        Utc::now() - chrono::Duration::seconds(1);
    network
        .reconcile_subnet_routes(&route_state)
        .await
        .expect("expire unavailable gateway route");
    assert!(!route_exists(handle, network.interface_index(), subnet).await);
    route_state.configuration_payload.nodes[1].candidates[0].expires_at =
        Utc::now() + chrono::Duration::minutes(10);
    (subnet, route_state)
}

async fn assert_gateway_route_lifecycle(
    network: &mut TunNetwork,
    handle: &Handle,
    plan: &NetworkPlan,
    subnet: Ipv4Net,
    route_state: &mut NodeState,
) {
    let lan_index = create_gateway_lan(handle).await;
    "xslan0".clone_into(&mut route_state.configuration_payload.subnet_routes[0].interface_name);
    "gateway-node".clone_into(&mut route_state.node_id_base64);
    let project_forwarding_before = forwarding_value(plan.interface_name());
    let lan_forwarding_before = forwarding_value("xslan0");
    network
        .reconcile_subnet_routes(route_state)
        .await
        .expect("enable routed gateway forwarding");
    assert!(!route_exists(handle, network.interface_index(), subnet).await);
    assert_eq!(forwarding_value(plan.interface_name()), 1);
    assert_eq!(forwarding_value("xslan0"), 1);
    assert!(!nft_table_present("xs_nexus_xstest0"));

    route_state.configuration_payload.subnet_routes[0].mode = SubnetRouteMode::Nat;
    network
        .reconcile_subnet_routes(route_state)
        .await
        .expect("enable scoped gateway NAT");
    let ruleset = nft_table_output("xs_nexus_xstest0");
    assert!(ruleset.contains("iifname \"xstest0\""));
    assert!(ruleset.contains("oifname \"xslan0\""));
    assert!(ruleset.contains("ip daddr 192.168.203.0/24"));
    assert!(ruleset.contains("masquerade"));

    let approved_route = route_state.configuration_payload.subnet_routes[0].clone();
    route_state.configuration_payload.subnet_routes.clear();
    network
        .reconcile_subnet_routes(route_state)
        .await
        .expect("pause gateway route and restore forwarding");
    assert_eq!(
        forwarding_value(plan.interface_name()),
        project_forwarding_before
    );
    assert_eq!(forwarding_value("xslan0"), lan_forwarding_before);
    assert!(!nft_table_present("xs_nexus_xstest0"));
    remove_dummy_link(handle, lan_index, "xslan0").await;

    "local-node".clone_into(&mut route_state.node_id_base64);
    route_state
        .configuration_payload
        .subnet_routes
        .push(approved_route);
    network
        .reconcile_subnet_routes(route_state)
        .await
        .expect("restore client route");
    assert!(route_exists(handle, network.interface_index(), subnet).await);
}

async fn assert_subnet_route_conflict_rejected(
    network: &mut TunNetwork,
    handle: &Handle,
    subnet: Ipv4Net,
    route_state: &mut NodeState,
) {
    handle
        .link()
        .add(LinkDummy::new("xsrouteconf0").up().build())
        .execute()
        .await
        .expect("create subnet conflict interface");
    let conflict_index = link_index(handle, "xsrouteconf0").await;
    handle
        .address()
        .add(
            conflict_index,
            IpAddr::V4(Ipv4Addr::new(192, 168, 204, 1)),
            24,
        )
        .execute()
        .await
        .expect("install conflicting subnet route");
    let conflicting_subnet: Ipv4Net = "192.168.204.0/24".parse().expect("conflicting subnet");
    route_state.configuration_payload.subnet_routes[0].prefix = conflicting_subnet.to_string();
    assert!(network.reconcile_subnet_routes(route_state).await.is_err());
    assert!(route_exists(handle, network.interface_index(), subnet).await);
    assert!(!route_exists(handle, network.interface_index(), conflicting_subnet).await);
    remove_dummy_link(handle, conflict_index, "xsrouteconf0").await;
}

async fn assert_stale_gateway_recovery(
    handle: &Handle,
    plan: &NetworkPlan,
    manifest: &Path,
    default_routes: &[String],
    subnet: Ipv4Net,
) {
    let lan_index = create_gateway_lan(handle).await;
    let lan_forwarding_before = forwarding_value("xslan0");
    let mut dropped = TunNetwork::create(plan.clone(), manifest)
        .await
        .expect("recreate TUN network");
    let mut stale_state = subnet_route_state(subnet, "gateway-node", true);
    "xslan0".clone_into(&mut stale_state.configuration_payload.subnet_routes[0].interface_name);
    stale_state.configuration_payload.subnet_routes[0].mode = SubnetRouteMode::Nat;
    dropped
        .reconcile_subnet_routes(&stale_state)
        .await
        .expect("enable resources before simulated crash");
    assert_eq!(forwarding_value("xslan0"), 1);
    assert!(nft_table_present("xs_nexus_xstest0"));
    drop(dropped);
    wait_for_absent_link(handle, plan.interface_name()).await;
    assert!(manifest.is_file());
    TunNetwork::recover_stale(plan, manifest)
        .await
        .expect("recover stale manifest");
    assert!(!manifest.exists());
    assert_eq!(forwarding_value("xslan0"), lan_forwarding_before);
    assert!(!nft_table_present("xs_nexus_xstest0"));
    assert_eq!(default_routes, default_route_fingerprints(handle).await);
    remove_dummy_link(handle, lan_index, "xslan0").await;
}

async fn create_gateway_lan(handle: &Handle) -> u32 {
    handle
        .link()
        .add(LinkDummy::new("xslan0").up().build())
        .execute()
        .await
        .expect("create gateway LAN interface");
    let lan_index = link_index(handle, "xslan0").await;
    handle
        .address()
        .add(lan_index, IpAddr::V4(Ipv4Addr::new(192, 168, 203, 1)), 24)
        .execute()
        .await
        .expect("install gateway LAN route");
    lan_index
}

async fn remove_dummy_link(handle: &Handle, index: u32, interface_name: &str) {
    handle
        .link()
        .del(index)
        .execute()
        .await
        .expect("remove test dummy interface");
    wait_for_absent_link(handle, interface_name).await;
}

fn forwarding_value(interface_name: &str) -> u8 {
    std::fs::read_to_string(format!(
        "/proc/sys/net/ipv4/conf/{interface_name}/forwarding"
    ))
    .expect("read forwarding sysctl")
    .trim()
    .parse()
    .expect("parse forwarding sysctl")
}

fn nft_table_present(table_name: &str) -> bool {
    Command::new("nft")
        .args(["list", "table", "ip", table_name])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("execute nft inspection")
        .success()
}

fn nft_table_output(table_name: &str) -> String {
    let output = Command::new("nft")
        .args(["list", "table", "ip", table_name])
        .output()
        .expect("execute nft inspection");
    assert!(output.status.success());
    String::from_utf8(output.stdout).expect("nft output is UTF-8")
}

fn subnet_route_state(prefix: Ipv4Net, local_node_id: &str, gateway_online: bool) -> NodeState {
    let now = Utc::now();
    let expires_at = now + chrono::Duration::hours(1);
    let gateway_candidates = if gateway_online {
        vec![EndpointCandidate {
            kind: EndpointCandidateKind::Mapped,
            endpoint: "198.51.100.20:42000".parse().expect("candidate endpoint"),
            priority: 100,
            expires_at,
        }]
    } else {
        Vec::new()
    };
    let nodes = vec![
        ConfigurationNode {
            node_id_base64: "local-node".to_owned(),
            identity_public_key_base64: "local-key".to_owned(),
            virtual_ip: "100.127.250.16".to_owned(),
            direct_endpoints: Vec::new(),
            candidates: Vec::new(),
            credential_serial: 1,
            credential_not_after: expires_at,
            role_bitmap: 1,
            groups: Vec::new(),
            tags: vec!["client".to_owned()],
        },
        ConfigurationNode {
            node_id_base64: "gateway-node".to_owned(),
            identity_public_key_base64: "gateway-key".to_owned(),
            virtual_ip: "100.127.250.17".to_owned(),
            direct_endpoints: Vec::new(),
            candidates: gateway_candidates,
            credential_serial: 2,
            credential_not_after: expires_at,
            role_bitmap: 1,
            groups: Vec::new(),
            tags: vec!["gateway".to_owned()],
        },
    ];
    NodeState {
        schema_version: 1,
        controller_url: "https://controller.example/".to_owned(),
        network_id: Uuid::from_u128(1),
        node_id_base64: local_node_id.to_owned(),
        virtual_ip: Ipv4Addr::new(100, 127, 250, 16),
        credential_base64: "credential".to_owned(),
        credential_key_id: 1,
        credential_signing_public_key_base64: "credential-key".to_owned(),
        configuration_signing_public_key_base64: "configuration-key".to_owned(),
        configuration: SignedConfiguration {
            version: 1,
            payload_base64: "payload".to_owned(),
            signature_base64: "signature".to_owned(),
            signer_key_id: 1,
        },
        configuration_payload: ConfigurationPayload {
            schema_version: 1,
            network_id: Uuid::from_u128(1),
            version: 1,
            policy_version: 1,
            generated_at: now,
            address_pool: "100.127.250.0/24".to_owned(),
            discovery_endpoints: Vec::new(),
            nodes,
            relays: Vec::new(),
            policies: Vec::new(),
            subnet_routes: vec![ConfigurationSubnetRoute {
                route_id: "route-1".to_owned(),
                prefix: prefix.to_string(),
                gateway_node_id_base64: "gateway-node".to_owned(),
                mode: SubnetRouteMode::Routed,
                interface_name: "eth0".to_owned(),
                priority: 100,
            }],
        },
        configuration_sha256: "configuration-sha256".to_owned(),
        credential_serial: 1,
        candidate_generation: 0,
        subnet_route_generation: 0,
    }
}

async fn link_index(handle: &Handle, name: &str) -> u32 {
    link_index_optional(handle, name)
        .await
        .expect("expected network interface")
}

async fn link_index_optional(handle: &Handle, name: &str) -> Option<u32> {
    let mut links = handle.link().get().execute();
    while let Some(link) = links.try_next().await.expect("query links") {
        if link
            .attributes
            .iter()
            .any(|attribute| matches!(attribute, LinkAttribute::IfName(value) if value == name))
        {
            return Some(link.header.index);
        }
    }
    None
}

fn netlink() -> (Handle, JoinHandle<()>) {
    let (connection, handle, _) = new_connection().expect("open Netlink connection");
    (handle, tokio::spawn(connection))
}

async fn assert_link(handle: &Handle, index: u32, plan: &NetworkPlan) {
    let mut links = handle.link().get().match_index(index).execute();
    let link = links
        .try_next()
        .await
        .expect("query link")
        .expect("configured link");
    assert!(link.header.flags.contains(LinkFlags::Up));
    assert!(link.attributes.iter().any(
        |attribute| matches!(attribute, LinkAttribute::Mtu(mtu) if *mtu == u32::from(plan.mtu()))
    ));
    assert!(link.attributes.iter().any(
        |attribute| matches!(attribute, LinkAttribute::IfName(name) if name == plan.interface_name())
    ));
}

async fn assert_address(handle: &Handle, index: u32, virtual_ip: Ipv4Addr) {
    let mut addresses = handle
        .address()
        .get()
        .set_link_index_filter(index)
        .execute();
    while let Some(address) = addresses.try_next().await.expect("query addresses") {
        if address.header.prefix_len == 32
            && address.attributes.iter().any(|attribute| {
                matches!(
                    attribute,
                    AddressAttribute::Address(IpAddr::V4(value))
                        | AddressAttribute::Local(IpAddr::V4(value))
                        if *value == virtual_ip
                )
            })
        {
            return;
        }
    }
    panic!("expected /32 Agent address was not installed");
}

async fn assert_route(handle: &Handle, index: u32, plan: &NetworkPlan) {
    let mut routes = ipv4_routes(handle);
    while let Some(route) = routes.try_next().await.expect("query routes") {
        if route_matches(&route, index, plan) {
            return;
        }
    }
    panic!("expected Agent pool route was not installed");
}

fn route_matches(route: &RouteMessage, index: u32, plan: &NetworkPlan) -> bool {
    route.header.destination_prefix_length == plan.address_pool().prefix_len()
        && route.attributes.iter().any(|attribute| {
            matches!(
                attribute,
                RouteAttribute::Destination(RouteAddress::Inet(address))
                    if *address == plan.address_pool().network()
            )
        })
        && route
            .attributes
            .iter()
            .any(|attribute| matches!(attribute, RouteAttribute::Oif(value) if *value == index))
}

async fn route_exists(handle: &Handle, index: u32, prefix: Ipv4Net) -> bool {
    let mut routes = ipv4_routes(handle);
    while let Some(route) = routes.try_next().await.expect("query subnet routes") {
        let matches_prefix = route.header.destination_prefix_length == prefix.prefix_len()
            && route.attributes.iter().any(|attribute| {
                matches!(
                    attribute,
                    RouteAttribute::Destination(RouteAddress::Inet(address))
                        if *address == prefix.network()
                )
            });
        let matches_interface = index == 0
            || route.attributes.iter().any(
                |attribute| matches!(attribute, RouteAttribute::Oif(value) if *value == index),
            );
        if matches_prefix && matches_interface {
            return true;
        }
    }
    false
}

async fn default_route_fingerprints(handle: &Handle) -> Vec<String> {
    let mut fingerprints = Vec::new();
    let mut routes = ipv4_routes(handle);
    while let Some(route) = routes.try_next().await.expect("query default routes") {
        if route.header.destination_prefix_length == 0
            && !route
                .attributes
                .iter()
                .any(|attribute| matches!(attribute, RouteAttribute::Destination(_)))
        {
            fingerprints.push(format!("{route:?}"));
        }
    }
    fingerprints.sort_unstable();
    fingerprints
}

fn ipv4_routes(
    handle: &Handle,
) -> impl futures_util::TryStream<Ok = RouteMessage, Error = rtnetlink::Error> + Unpin {
    handle
        .route()
        .get(RouteMessageBuilder::<Ipv4Addr>::new().build())
        .execute()
}

async fn wait_for_absent_link(handle: &Handle, name: &str) {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let mut links = handle.link().get().execute();
            let mut found = false;
            while let Some(link) = links.try_next().await.expect("query link removal") {
                found = link.attributes.iter().any(
                    |attribute| matches!(attribute, LinkAttribute::IfName(value) if value == name),
                );
                if found {
                    break;
                }
            }
            if !found {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("TUN interface is removed");
}

#[test]
fn privileged_test_must_run_inside_a_network_namespace() {
    let host = std::env::var("XS_HOST_NETWORK_NAMESPACE")
        .expect("test script must provide the host network namespace");
    let current = std::fs::read_link(Path::new("/proc/self/ns/net"))
        .expect("read current network namespace")
        .to_string_lossy()
        .into_owned();
    assert_ne!(host, current);
}
