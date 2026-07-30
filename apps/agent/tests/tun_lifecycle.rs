#![cfg(target_os = "linux")]

use std::{net::IpAddr, net::Ipv4Addr, path::Path, time::Duration};

use futures_util::TryStreamExt as _;
use rtnetlink::{
    Handle, LinkDummy, RouteMessageBuilder, new_connection,
    packet_route::{
        address::AddressAttribute,
        link::{LinkAttribute, LinkFlags},
        route::{RouteAddress, RouteAttribute, RouteMessage},
    },
};
use tokio::task::JoinHandle;
use xs_agent::network::{NetworkPlan, TunNetwork};

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
    handle
        .link()
        .add(LinkDummy::new("xsconflict0").up().build())
        .execute()
        .await
        .expect("create conflicting dummy interface");
    let conflict_index = link_index(&handle, "xsconflict0").await;
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
        TunNetwork::create(plan.clone(), &manifest).await.is_err(),
        "overlapping system route must reject TUN creation"
    );
    assert!(!manifest.exists());
    assert_eq!(
        link_index_optional(&handle, plan.interface_name()).await,
        None
    );
    handle
        .link()
        .del(conflict_index)
        .execute()
        .await
        .expect("remove conflicting dummy interface");
    wait_for_absent_link(&handle, "xsconflict0").await;

    let network = TunNetwork::create(plan.clone(), &manifest)
        .await
        .expect("create TUN network");
    assert!(manifest.is_file());
    assert_link(&handle, network.interface_index(), &plan).await;
    assert_address(&handle, network.interface_index(), plan.virtual_ip()).await;
    assert_route(&handle, network.interface_index(), &plan).await;
    assert_eq!(default_routes, default_route_fingerprints(&handle).await);

    network.shutdown().await.expect("clean network shutdown");
    wait_for_absent_link(&handle, plan.interface_name()).await;
    assert!(!manifest.exists());
    assert_eq!(default_routes, default_route_fingerprints(&handle).await);

    let dropped = TunNetwork::create(plan.clone(), &manifest)
        .await
        .expect("recreate TUN network");
    drop(dropped);
    wait_for_absent_link(&handle, plan.interface_name()).await;
    assert!(manifest.is_file());
    TunNetwork::recover_stale(&plan, &manifest)
        .await
        .expect("recover stale manifest");
    assert!(!manifest.exists());
    assert_eq!(default_routes, default_route_fingerprints(&handle).await);
    connection.abort();
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
