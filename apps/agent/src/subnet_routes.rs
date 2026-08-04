use std::time::{Duration, Instant};
#[cfg(target_os = "linux")]
use std::{
    collections::{BTreeSet, HashMap},
    net::Ipv4Addr,
};

use chrono::{Duration as ChronoDuration, Utc};
#[cfg(target_os = "linux")]
use futures_util::TryStreamExt as _;
use ipnet::Ipv4Net;
#[cfg(target_os = "linux")]
use rtnetlink::{
    RouteMessageBuilder, new_connection,
    packet_route::{
        link::LinkAttribute,
        route::{RouteAddress, RouteAttribute, RouteProtocol, RouteScope},
    },
};
#[cfg(target_os = "linux")]
use xs_core::validate_subnet_route_suggestion;
use xs_core::{SubnetRouteAdvertisement, SubnetRouteSuggestion};

use crate::{
    error::{AgentError, Result},
    state::NodeState,
};

const ADVERTISEMENT_LIFETIME: ChronoDuration = ChronoDuration::minutes(10);
#[cfg(not(feature = "privileged-network-tests"))]
const REFRESH_INTERVAL: Duration = Duration::from_mins(4);
#[cfg(feature = "privileged-network-tests")]
const REFRESH_INTERVAL: Duration = Duration::from_secs(2);

pub struct SubnetRouteDiscovery {
    network_id: uuid::Uuid,
    node_id_base64: String,
    address_pool: String,
    project_interface_name: String,
    generation: u64,
    next_refresh: Instant,
}

impl SubnetRouteDiscovery {
    /// Creates local route discovery state without publishing any route.
    ///
    /// # Errors
    ///
    /// Returns a state error when the signed virtual address pool is malformed.
    pub fn new(state: &NodeState, project_interface_name: String) -> Result<Self> {
        state
            .configuration_payload
            .address_pool
            .parse::<Ipv4Net>()
            .map_err(|_| AgentError::ControllerTrust)?;
        Ok(Self {
            network_id: state.network_id,
            node_id_base64: state.node_id_base64.clone(),
            address_pool: state.configuration_payload.address_pool.clone(),
            project_interface_name,
            generation: state.subnet_route_generation,
            next_refresh: Instant::now(),
        })
    }

    #[must_use]
    pub fn refresh_due(&self, now: Instant) -> bool {
        now >= self.next_refresh
    }

    /// Enumerates safe directly connected private IPv4 routes and emits suggestions only.
    ///
    /// # Errors
    ///
    /// Returns a network error when Netlink enumeration fails.
    pub async fn refresh(&mut self) -> Result<SubnetRouteAdvertisement> {
        let suggestions =
            collect_suggestions(&self.address_pool, &self.project_interface_name).await?;
        let now = Utc::now();
        let clock_generation = u64::try_from(now.timestamp_millis()).unwrap_or(1).max(1);
        self.generation = self.generation.saturating_add(1).max(clock_generation);
        self.next_refresh = Instant::now() + REFRESH_INTERVAL;
        Ok(SubnetRouteAdvertisement {
            schema_version: 1,
            network_id: self.network_id,
            node_id_base64: self.node_id_base64.clone(),
            generation: self.generation,
            generated_at: now,
            expires_at: now + ADVERTISEMENT_LIFETIME,
            suggestions,
        })
    }
}

#[cfg(target_os = "linux")]
async fn collect_suggestions(
    address_pool: &str,
    project_interface_name: &str,
) -> Result<Vec<SubnetRouteSuggestion>> {
    let (connection, handle, _) = new_connection().map_err(|_| AgentError::Network)?;
    let connection = tokio::spawn(connection);
    let mut interface_names = HashMap::new();
    let mut links = handle.link().get().execute();
    while let Some(link) = links.try_next().await.map_err(|_| AgentError::Network)? {
        if let Some(name) = link
            .attributes
            .iter()
            .find_map(|attribute| match attribute {
                LinkAttribute::IfName(name) => Some(name.clone()),
                _ => None,
            })
        {
            interface_names.insert(link.header.index, name);
        }
    }

    let mut routes = handle
        .route()
        .get(RouteMessageBuilder::<Ipv4Addr>::new().build())
        .execute();
    let mut suggestions = BTreeSet::new();
    while let Some(route) = routes.try_next().await.map_err(|_| AgentError::Network)? {
        if route.header.destination_prefix_length == 0
            || route.header.protocol != RouteProtocol::Kernel
            || route.header.scope != RouteScope::Link
            || route_table(&route) != 254
        {
            continue;
        }
        let destination = route
            .attributes
            .iter()
            .find_map(|attribute| match attribute {
                RouteAttribute::Destination(RouteAddress::Inet(address)) => Some(*address),
                _ => None,
            });
        let output_interface = route
            .attributes
            .iter()
            .find_map(|attribute| match attribute {
                RouteAttribute::Oif(index) => Some(*index),
                _ => None,
            });
        let (Some(destination), Some(output_interface)) = (destination, output_interface) else {
            continue;
        };
        let Some(interface_name) = interface_names.get(&output_interface) else {
            continue;
        };
        if !suggestible_interface(interface_name, project_interface_name) {
            continue;
        }
        let prefix = Ipv4Net::new(destination, route.header.destination_prefix_length)
            .map_err(|_| AgentError::Network)?;
        if !prefix.network().is_private()
            || validate_subnet_route_suggestion(&prefix.to_string(), interface_name, address_pool)
                .is_err()
        {
            continue;
        }
        suggestions.insert((prefix.to_string(), interface_name.clone()));
    }
    connection.abort();
    Ok(suggestions
        .into_iter()
        .map(|(prefix, interface_name)| SubnetRouteSuggestion {
            prefix,
            interface_name: interface_name.clone(),
        })
        .collect())
}

#[cfg(not(target_os = "linux"))]
#[allow(clippy::unused_async)] // Keeps SubnetRouteDiscovery's refresh API uniform across targets.
async fn collect_suggestions(
    _address_pool: &str,
    _project_interface_name: &str,
) -> Result<Vec<SubnetRouteSuggestion>> {
    Ok(Vec::new())
}

#[cfg(target_os = "linux")]
fn route_table(route: &rtnetlink::packet_route::route::RouteMessage) -> u32 {
    route
        .attributes
        .iter()
        .find_map(|attribute| match attribute {
            RouteAttribute::Table(table) => Some(*table),
            _ => None,
        })
        .unwrap_or_else(|| u32::from(route.header.table))
}

#[cfg(any(target_os = "linux", test))]
fn suggestible_interface(name: &str, project_interface_name: &str) -> bool {
    name != project_interface_name
        && ![
            "lo",
            "docker",
            "br-",
            "veth",
            "cni",
            "flannel",
            "virbr",
            "podman",
            "tailscale",
            "zt",
        ]
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::suggestible_interface;

    #[test]
    fn discovery_excludes_project_loopback_and_container_interfaces() {
        assert!(!suggestible_interface("xsn0", "xsn0"));
        assert!(!suggestible_interface("lo", "xsn0"));
        assert!(!suggestible_interface("docker0", "xsn0"));
        assert!(!suggestible_interface("br-abcd", "xsn0"));
        assert!(!suggestible_interface("veth1234", "xsn0"));
        assert!(suggestible_interface("eth0", "xsn0"));
        assert!(suggestible_interface("ens4", "xsn0"));
    }
}
