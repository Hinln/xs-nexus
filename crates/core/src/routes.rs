use std::{
    collections::{HashMap, HashSet},
    net::Ipv4Addr,
};

use chrono::{DateTime, Utc};
use ipnet::Ipv4Net;

use crate::{ConfigurationNode, ConfigurationPayload, EndpointCandidateKind, SubnetRouteMode};

const MAX_SUBNET_ROUTES: usize = 256;
const MAX_ROUTE_ID_LENGTH: usize = 64;
const MAX_INTERFACE_NAME_LENGTH: usize = 15;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubnetRouteValidationError {
    RouteCount,
    InvalidRoute,
    InvalidGateway,
    DuplicateRoute,
    RouteOrder,
    OverlappingRoute,
    VirtualPoolOverlap,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResolvedSubnetRoute<'a> {
    pub route_id: &'a str,
    pub prefix: Ipv4Net,
    pub gateway_node_id_base64: &'a str,
    pub gateway_virtual_ip: Ipv4Addr,
    pub mode: SubnetRouteMode,
    pub interface_name: &'a str,
}

#[derive(Clone, Debug)]
struct CompiledSubnetRoute {
    route_id: String,
    prefix: Ipv4Net,
    gateway_node_id_base64: String,
    gateway_virtual_ip: Ipv4Addr,
    mode: SubnetRouteMode,
    interface_name: String,
    priority: u32,
}

#[derive(Clone, Debug)]
pub struct SubnetRoutePolicy {
    routes: Vec<CompiledSubnetRoute>,
    gateway_nodes: HashMap<String, ConfigurationNode>,
}

impl SubnetRoutePolicy {
    /// Compiles and validates approved subnet routes from a signed configuration.
    ///
    /// # Errors
    ///
    /// Returns a validation error for unsafe, ambiguous, overlapping, or non-canonical routes.
    pub fn compile(
        configuration: &ConfigurationPayload,
    ) -> Result<Self, SubnetRouteValidationError> {
        if configuration.subnet_routes.len() > MAX_SUBNET_ROUTES {
            return Err(SubnetRouteValidationError::RouteCount);
        }
        let virtual_pool = configuration
            .address_pool
            .parse::<Ipv4Net>()
            .map_err(|_| SubnetRouteValidationError::VirtualPoolOverlap)?;
        let mut gateway_nodes = HashMap::with_capacity(configuration.nodes.len());
        for node in &configuration.nodes {
            if gateway_nodes
                .insert(node.node_id_base64.clone(), node.clone())
                .is_some()
            {
                return Err(SubnetRouteValidationError::InvalidGateway);
            }
        }

        let mut route_ids = HashSet::with_capacity(configuration.subnet_routes.len());
        let mut compiled = Vec::with_capacity(configuration.subnet_routes.len());
        for route in &configuration.subnet_routes {
            if !valid_route_id(&route.route_id)
                || !route_ids.insert(route.route_id.as_str())
                || route.priority == 0
                || !valid_interface_name(&route.interface_name)
            {
                return Err(SubnetRouteValidationError::InvalidRoute);
            }
            let prefix = route
                .prefix
                .parse::<Ipv4Net>()
                .map_err(|_| SubnetRouteValidationError::InvalidRoute)?;
            if route.prefix != prefix.to_string() || !safe_subnet_prefix(prefix) {
                return Err(SubnetRouteValidationError::InvalidRoute);
            }
            if networks_overlap(prefix, virtual_pool) {
                return Err(SubnetRouteValidationError::VirtualPoolOverlap);
            }
            let gateway = gateway_nodes
                .get(&route.gateway_node_id_base64)
                .ok_or(SubnetRouteValidationError::InvalidGateway)?;
            let gateway_virtual_ip = gateway
                .virtual_ip
                .parse::<Ipv4Addr>()
                .map_err(|_| SubnetRouteValidationError::InvalidGateway)?;
            compiled.push(CompiledSubnetRoute {
                route_id: route.route_id.clone(),
                prefix,
                gateway_node_id_base64: route.gateway_node_id_base64.clone(),
                gateway_virtual_ip,
                mode: route.mode,
                interface_name: route.interface_name.clone(),
                priority: route.priority,
            });
        }

        validate_order_and_overlap(&compiled)?;
        Ok(Self {
            routes: compiled,
            gateway_nodes,
        })
    }

    #[must_use]
    pub fn resolve_destination(
        &self,
        destination: Ipv4Addr,
        now: DateTime<Utc>,
    ) -> Option<ResolvedSubnetRoute<'_>> {
        self.routes
            .iter()
            .filter(|route| route.prefix.contains(&destination))
            .find(|route| self.gateway_available(&route.gateway_node_id_base64, now))
            .map(resolved_route)
    }

    #[must_use]
    pub fn active_client_prefixes(
        &self,
        local_node_id_base64: &str,
        now: DateTime<Utc>,
    ) -> Vec<Ipv4Net> {
        let mut prefixes = Vec::new();
        let mut index = 0;
        while index < self.routes.len() {
            let prefix = self.routes[index].prefix;
            let mut end = index + 1;
            while end < self.routes.len() && self.routes[end].prefix == prefix {
                end += 1;
            }
            let candidates = &self.routes[index..end];
            let local_is_gateway = candidates
                .iter()
                .any(|route| route.gateway_node_id_base64 == local_node_id_base64);
            let active = candidates
                .iter()
                .any(|route| self.gateway_available(&route.gateway_node_id_base64, now));
            if !local_is_gateway && active {
                prefixes.push(prefix);
            }
            index = end;
        }
        prefixes
    }

    #[must_use]
    pub fn source_owned_by_gateway(&self, source: Ipv4Addr, gateway_node_id_base64: &str) -> bool {
        self.routes.iter().any(|route| {
            route.gateway_node_id_base64 == gateway_node_id_base64 && route.prefix.contains(&source)
        })
    }

    #[must_use]
    pub fn permits_routed_session(
        &self,
        local_node_id_base64: &str,
        peer_node_id_base64: &str,
    ) -> bool {
        self.routes.iter().any(|route| {
            route.gateway_node_id_base64 == local_node_id_base64
                || route.gateway_node_id_base64 == peer_node_id_base64
        })
    }

    #[must_use]
    pub fn local_gateway_route(
        &self,
        local_node_id_base64: &str,
        destination: Ipv4Addr,
    ) -> Option<ResolvedSubnetRoute<'_>> {
        self.routes
            .iter()
            .find(|route| {
                route.gateway_node_id_base64 == local_node_id_base64
                    && route.prefix.contains(&destination)
            })
            .map(resolved_route)
    }

    #[must_use]
    pub fn local_gateway_routes(&self, local_node_id_base64: &str) -> Vec<ResolvedSubnetRoute<'_>> {
        self.routes
            .iter()
            .filter(|route| route.gateway_node_id_base64 == local_node_id_base64)
            .map(resolved_route)
            .collect()
    }

    #[must_use]
    pub fn contains_prefix(&self, prefix: Ipv4Net) -> bool {
        self.routes.iter().any(|route| route.prefix == prefix)
    }

    fn gateway_available(&self, node_id_base64: &str, now: DateTime<Utc>) -> bool {
        self.gateway_nodes.get(node_id_base64).is_some_and(|node| {
            !node.direct_endpoints.is_empty()
                || node.candidates.iter().any(|candidate| {
                    candidate.kind != EndpointCandidateKind::Relay && candidate.expires_at > now
                })
        })
    }
}

/// Validates one locally discovered subnet before it can be advertised for approval.
///
/// # Errors
///
/// Returns a validation error for unsafe, non-canonical, or virtual-pool-overlapping scopes.
pub fn validate_subnet_route_suggestion(
    prefix: &str,
    interface_name: &str,
    address_pool: &str,
) -> Result<Ipv4Net, SubnetRouteValidationError> {
    let parsed_prefix = prefix
        .parse::<Ipv4Net>()
        .map_err(|_| SubnetRouteValidationError::InvalidRoute)?;
    let address_pool = address_pool
        .parse::<Ipv4Net>()
        .map_err(|_| SubnetRouteValidationError::VirtualPoolOverlap)?;
    if prefix != parsed_prefix.to_string()
        || !safe_subnet_prefix(parsed_prefix)
        || !valid_interface_name(interface_name)
    {
        return Err(SubnetRouteValidationError::InvalidRoute);
    }
    if networks_overlap(parsed_prefix, address_pool) {
        return Err(SubnetRouteValidationError::VirtualPoolOverlap);
    }
    Ok(parsed_prefix)
}

fn resolved_route(route: &CompiledSubnetRoute) -> ResolvedSubnetRoute<'_> {
    ResolvedSubnetRoute {
        route_id: &route.route_id,
        prefix: route.prefix,
        gateway_node_id_base64: &route.gateway_node_id_base64,
        gateway_virtual_ip: route.gateway_virtual_ip,
        mode: route.mode,
        interface_name: &route.interface_name,
    }
}

fn validate_order_and_overlap(
    routes: &[CompiledSubnetRoute],
) -> Result<(), SubnetRouteValidationError> {
    for pair in routes.windows(2) {
        let previous = &pair[0];
        let current = &pair[1];
        if previous.prefix.network() > current.prefix.network()
            || (previous.prefix.network() == current.prefix.network()
                && previous.prefix.prefix_len() > current.prefix.prefix_len())
            || (previous.prefix == current.prefix && previous.priority < current.priority)
            || (previous.prefix == current.prefix
                && previous.priority == current.priority
                && previous.route_id >= current.route_id)
        {
            return Err(SubnetRouteValidationError::RouteOrder);
        }
    }
    for (index, left) in routes.iter().enumerate() {
        for right in &routes[index + 1..] {
            if left.route_id == right.route_id {
                return Err(SubnetRouteValidationError::DuplicateRoute);
            }
            if left.prefix == right.prefix {
                if left.priority == right.priority {
                    return Err(SubnetRouteValidationError::OverlappingRoute);
                }
                continue;
            }
            if networks_overlap(left.prefix, right.prefix) {
                return Err(SubnetRouteValidationError::OverlappingRoute);
            }
        }
    }
    Ok(())
}

fn safe_subnet_prefix(prefix: Ipv4Net) -> bool {
    if !(1..=30).contains(&prefix.prefix_len()) {
        return false;
    }
    [
        "0.0.0.0/8",
        "127.0.0.0/8",
        "169.254.0.0/16",
        "224.0.0.0/4",
        "240.0.0.0/4",
    ]
    .into_iter()
    .map(|value| value.parse::<Ipv4Net>().expect("constant IPv4 prefix"))
    .all(|reserved| !networks_overlap(prefix, reserved))
}

fn valid_route_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ROUTE_ID_LENGTH
        && value.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || (index > 0 && matches!(byte, b'.' | b'_' | b'-'))
        })
}

fn valid_interface_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_INTERFACE_NAME_LENGTH
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'))
}

fn networks_overlap(left: Ipv4Net, right: Ipv4Net) -> bool {
    left.contains(&right.network()) || right.contains(&left.network())
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, SocketAddr};

    use chrono::{Duration, Utc};
    use uuid::Uuid;

    use crate::{
        ConfigurationNode, ConfigurationPayload, ConfigurationSubnetRoute, EndpointCandidate,
        EndpointCandidateKind, SubnetRouteMode,
    };

    use super::{SubnetRoutePolicy, SubnetRouteValidationError};

    fn node(id: &str, virtual_ip: &str, expires_at: chrono::DateTime<Utc>) -> ConfigurationNode {
        ConfigurationNode {
            node_id_base64: id.to_owned(),
            identity_public_key_base64: format!("{id}-key"),
            virtual_ip: virtual_ip.to_owned(),
            direct_endpoints: Vec::new(),
            candidates: vec![EndpointCandidate {
                kind: EndpointCandidateKind::Local,
                endpoint: "192.0.2.10:42000"
                    .parse::<SocketAddr>()
                    .expect("candidate endpoint"),
                priority: 100,
                expires_at,
            }],
            credential_serial: 1,
            credential_not_after: expires_at + Duration::hours(1),
            role_bitmap: 0,
            groups: Vec::new(),
            tags: Vec::new(),
        }
    }

    fn route(id: &str, gateway: &str, priority: u32) -> ConfigurationSubnetRoute {
        ConfigurationSubnetRoute {
            route_id: id.to_owned(),
            prefix: "192.168.50.0/24".to_owned(),
            gateway_node_id_base64: gateway.to_owned(),
            mode: SubnetRouteMode::Routed,
            interface_name: "eth0".to_owned(),
            priority,
        }
    }

    fn configuration(routes: Vec<ConfigurationSubnetRoute>) -> ConfigurationPayload {
        let now = Utc::now();
        ConfigurationPayload {
            schema_version: 1,
            network_id: Uuid::nil(),
            version: 1,
            policy_version: 1,
            generated_at: now,
            address_pool: "100.88.0.0/16".to_owned(),
            discovery_endpoints: Vec::new(),
            nodes: vec![
                node("gateway-primary", "100.88.0.16", now + Duration::minutes(5)),
                node(
                    "gateway-secondary",
                    "100.88.0.17",
                    now + Duration::minutes(5),
                ),
                node("client", "100.88.0.18", now + Duration::minutes(5)),
            ],
            relays: Vec::new(),
            policies: Vec::new(),
            subnet_routes: routes,
        }
    }

    #[test]
    fn rejects_default_reserved_virtual_and_partial_overlap() {
        let mut unsafe_default = route("default", "gateway-primary", 100);
        unsafe_default.prefix = "0.0.0.0/0".to_owned();
        assert_eq!(
            SubnetRoutePolicy::compile(&configuration(vec![unsafe_default]))
                .expect_err("default route must fail"),
            SubnetRouteValidationError::InvalidRoute
        );

        let mut virtual_overlap = route("virtual", "gateway-primary", 100);
        virtual_overlap.prefix = "100.88.10.0/24".to_owned();
        assert_eq!(
            SubnetRoutePolicy::compile(&configuration(vec![virtual_overlap]))
                .expect_err("virtual pool overlap must fail"),
            SubnetRouteValidationError::VirtualPoolOverlap
        );

        let mut broader = route("broader", "gateway-primary", 100);
        broader.prefix = "192.168.0.0/16".to_owned();
        assert_eq!(
            SubnetRoutePolicy::compile(&configuration(vec![
                broader,
                route("narrower", "gateway-secondary", 90),
            ]))
            .expect_err("partial overlap must fail"),
            SubnetRouteValidationError::OverlappingRoute
        );
    }

    #[test]
    fn same_prefix_uses_priority_and_expires_offline_gateway() {
        let now = Utc::now();
        let mut configuration = configuration(vec![
            route("primary", "gateway-primary", 200),
            route("secondary", "gateway-secondary", 100),
        ]);
        configuration.nodes[0].candidates[0].expires_at = now - Duration::seconds(1);
        let policy = SubnetRoutePolicy::compile(&configuration).expect("valid failover routes");
        let destination = "192.168.50.25".parse::<Ipv4Addr>().expect("destination");
        let selected = policy
            .resolve_destination(destination, now)
            .expect("secondary route");
        assert_eq!(selected.route_id, "secondary");
        assert_eq!(selected.gateway_virtual_ip, Ipv4Addr::new(100, 88, 0, 17));
        assert_eq!(
            policy.active_client_prefixes("client", now),
            vec!["192.168.50.0/24".parse().expect("prefix")]
        );
        assert!(
            policy
                .active_client_prefixes("gateway-primary", now)
                .is_empty()
        );
    }

    #[test]
    fn equal_priority_for_same_prefix_is_rejected() {
        assert_eq!(
            SubnetRoutePolicy::compile(&configuration(vec![
                route("gateway-a", "gateway-primary", 100),
                route("gateway-b", "gateway-secondary", 100),
            ]))
            .expect_err("ambiguous priority must fail"),
            SubnetRouteValidationError::OverlappingRoute
        );
    }
}
