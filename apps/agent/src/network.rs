use std::net::Ipv4Addr;

use ipnet::Ipv4Net;
use serde::{Deserialize, Serialize};

#[cfg(target_os = "linux")]
use crate::gateway::{ForwardingRecord, GatewayRoute};

use crate::{
    config::AgentConfig,
    error::{AgentError, Result},
    state::NodeState,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NetworkPlan {
    interface_name: String,
    mtu: u16,
    virtual_ip: Ipv4Addr,
    address_pool: Ipv4Net,
}

impl NetworkPlan {
    /// Creates and validates an explicit Linux host network plan.
    ///
    /// # Errors
    ///
    /// Returns [`AgentError::Network`] when the interface, MTU, address, or route scope is unsafe.
    pub fn new(
        interface_name: String,
        mtu: u16,
        virtual_ip: Ipv4Addr,
        address_pool: Ipv4Net,
    ) -> Result<Self> {
        let plan = Self {
            interface_name,
            mtu,
            virtual_ip,
            address_pool,
        };
        plan.validate()?;
        Ok(plan)
    }

    /// Builds and validates the host network plan from trusted Agent state.
    ///
    /// # Errors
    ///
    /// Returns [`AgentError::Network`] when the interface, MTU, address, or route scope is unsafe.
    pub fn from_state(config: &AgentConfig, state: &NodeState) -> Result<Self> {
        let address_pool = state
            .configuration_payload
            .address_pool
            .parse::<Ipv4Net>()
            .map_err(|_| AgentError::Network)?;
        Self::new(
            config.interface_name.clone(),
            config.mtu,
            state.virtual_ip,
            address_pool,
        )
    }

    fn validate(&self) -> Result<()> {
        if !(3..=15).contains(&self.interface_name.len())
            || !self.interface_name.starts_with("xs")
            || !self
                .interface_name
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            || !(1280..=1500).contains(&self.mtu)
            || !(8..=30).contains(&self.address_pool.prefix_len())
            || !self.address_pool.contains(&self.virtual_ip)
            || self.virtual_ip == self.address_pool.network()
            || self.virtual_ip == self.address_pool.broadcast()
        {
            return Err(AgentError::Network);
        }
        Ok(())
    }

    #[must_use]
    pub fn interface_name(&self) -> &str {
        &self.interface_name
    }

    #[must_use]
    pub const fn mtu(&self) -> u16 {
        self.mtu
    }

    #[must_use]
    pub const fn virtual_ip(&self) -> Ipv4Addr {
        self.virtual_ip
    }

    #[must_use]
    pub const fn address_pool(&self) -> Ipv4Net {
        self.address_pool
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct NetworkManifest {
    schema_version: u8,
    interface_name: String,
    interface_index: u32,
    mtu: u16,
    virtual_ip: Ipv4Addr,
    address_pool: Ipv4Net,
    #[serde(default)]
    subnet_routes: Vec<Ipv4Net>,
    #[cfg(target_os = "linux")]
    #[serde(default)]
    gateway_routes: Vec<GatewayRoute>,
    #[cfg(target_os = "linux")]
    #[serde(default)]
    forwarding: Vec<ForwardingRecord>,
    #[cfg(target_os = "linux")]
    #[serde(default)]
    nat_table_name: Option<String>,
}

#[cfg(target_os = "linux")]
mod platform {
    use std::{
        collections::{HashMap, HashSet},
        net::{IpAddr, Ipv4Addr},
        path::{Path, PathBuf},
        time::Duration,
    };

    use futures_util::TryStreamExt as _;
    use ipnet::Ipv4Net;
    use rtnetlink::{
        Handle, LinkUnspec, RouteMessageBuilder, new_connection,
        packet_route::{
            link::LinkAttribute,
            route::{RouteAddress, RouteAttribute, RouteMessage},
        },
    };
    use tokio::task::JoinHandle;
    use tokio_tun::{Tun, TunBuilder};
    use xs_core::SubnetRoutePolicy;

    use super::{NetworkManifest, NetworkPlan};
    use crate::{
        error::{AgentError, Result},
        gateway::{
            ForwardingRecord, GatewayRoute, delete_nat_table, forwarding_path_exists,
            forwarding_value, replace_nat_table, restore_forwarding, set_forwarding, table_name,
            validate_gateway_routes,
        },
        state::NodeState,
        storage::{read_json, write_json},
    };

    pub struct TunNetwork {
        plan: NetworkPlan,
        interface_index: u32,
        device: Option<Tun>,
        handle: Option<Handle>,
        connection: Option<JoinHandle<()>>,
        route: Option<RouteMessage>,
        subnet_routes: HashMap<Ipv4Net, RouteMessage>,
        gateway_routes: Vec<GatewayRoute>,
        forwarding: Vec<ForwardingRecord>,
        nat_table_name: Option<String>,
        manifest_path: PathBuf,
    }

    impl TunNetwork {
        /// Creates a non-persistent TUN and configures its address and route using Netlink.
        ///
        /// # Errors
        ///
        /// Returns [`AgentError::Network`] when a stale resource exists, TUN allocation fails,
        /// Netlink configuration fails, or the recovery manifest cannot be persisted.
        pub async fn create(plan: NetworkPlan, manifest_path: &Path) -> Result<Self> {
            Self::recover_stale(&plan, manifest_path).await?;
            let (connection, handle, _) = new_connection().map_err(|_| AgentError::Network)?;
            let connection = tokio::spawn(connection);
            if find_link_index(&handle, plan.interface_name())
                .await?
                .is_some()
            {
                connection.abort();
                return Err(AgentError::Network);
            }
            if ensure_route_available(&handle, plan.address_pool())
                .await
                .is_err()
            {
                connection.abort();
                return Err(AgentError::Network);
            }

            let mut devices = TunBuilder::new()
                .name(plan.interface_name())
                .close_on_exec()
                .build()
                .map_err(|_| AgentError::Network)?;
            if devices.len() != 1 {
                connection.abort();
                return Err(AgentError::Network);
            }
            let device = devices.pop().ok_or(AgentError::Network)?;
            if device.name() != plan.interface_name() {
                drop(device);
                connection.abort();
                return Err(AgentError::Network);
            }

            let setup = configure_link(&handle, &plan).await;
            let (interface_index, route) = match setup {
                Ok(configured) => configured,
                Err(error) => {
                    drop(device);
                    connection.abort();
                    return Err(error);
                }
            };
            let manifest = NetworkManifest {
                schema_version: 3,
                interface_name: plan.interface_name.clone(),
                interface_index,
                mtu: plan.mtu,
                virtual_ip: plan.virtual_ip,
                address_pool: plan.address_pool,
                subnet_routes: Vec::new(),
                gateway_routes: Vec::new(),
                forwarding: Vec::new(),
                nat_table_name: None,
            };
            if write_json(manifest_path, &manifest).is_err() {
                drop(device);
                connection.abort();
                return Err(AgentError::Network);
            }

            Ok(Self {
                plan,
                interface_index,
                device: Some(device),
                handle: Some(handle),
                connection: Some(connection),
                route: Some(route),
                subnet_routes: HashMap::new(),
                gateway_routes: Vec::new(),
                forwarding: Vec::new(),
                nat_table_name: None,
                manifest_path: manifest_path.to_path_buf(),
            })
        }

        /// Removes the project route, closes the TUN FD, and clears the recovery manifest.
        ///
        /// # Errors
        ///
        /// Returns [`AgentError::Network`] when the route, interface, or manifest is not removed.
        pub async fn shutdown(mut self) -> Result<()> {
            let handle = self.handle.take().ok_or(AgentError::Network)?;
            let gateway_result = self.cleanup_gateway();
            let mut subnet_routes_removed = true;
            for (_, route) in self.subnet_routes.drain() {
                if handle.route().del(route).execute().await.is_err() {
                    subnet_routes_removed = false;
                }
            }
            let route_result = if let Some(route) = self.route.take() {
                handle.route().del(route).execute().await
            } else {
                Ok(())
            };
            drop(self.device.take());
            let interface_removed =
                wait_for_link_removal(&handle, self.plan.interface_name()).await;
            let manifest_result = remove_manifest(&self.manifest_path);
            if let Some(connection) = self.connection.take() {
                connection.abort();
            }
            if gateway_result.is_err()
                || !subnet_routes_removed
                || route_result.is_err()
                || !interface_removed
                || manifest_result.is_err()
            {
                return Err(AgentError::Network);
            }
            Ok(())
        }

        /// Removes a stale manifest only when its recorded interface no longer exists.
        ///
        /// # Errors
        ///
        /// Returns [`AgentError::Network`] for malformed manifests, mismatched plans, active
        /// interfaces, Netlink failures, or manifest removal failures.
        pub async fn recover_stale(plan: &NetworkPlan, manifest_path: &Path) -> Result<()> {
            if !manifest_path.exists() {
                return Ok(());
            }
            let manifest: NetworkManifest = read_json(manifest_path)?;
            if !(1..=3).contains(&manifest.schema_version)
                || manifest.interface_name != plan.interface_name
                || manifest.mtu != plan.mtu
                || manifest.virtual_ip != plan.virtual_ip
                || manifest.address_pool != plan.address_pool
            {
                return Err(AgentError::Network);
            }
            let (connection, handle, _) = new_connection().map_err(|_| AgentError::Network)?;
            let connection = tokio::spawn(connection);
            let active = find_link_index(&handle, plan.interface_name()).await?;
            connection.abort();
            if active.is_some() {
                return Err(AgentError::Network);
            }
            cleanup_manifest_gateway(&manifest)?;
            remove_manifest(manifest_path)
        }

        #[must_use]
        pub const fn interface_index(&self) -> u32 {
            self.interface_index
        }

        #[must_use]
        pub fn plan(&self) -> &NetworkPlan {
            &self.plan
        }

        /// Reconciles approved client subnet routes through the project TUN.
        ///
        /// # Errors
        ///
        /// Returns a network error without changing routes when a desired prefix conflicts with
        /// a non-project system route.
        pub async fn reconcile_subnet_routes(&mut self, state: &NodeState) -> Result<()> {
            let policy = SubnetRoutePolicy::compile(&state.configuration_payload)
                .map_err(|_| AgentError::ControllerTrust)?;
            let desired_gateway_routes = policy
                .local_gateway_routes(&state.node_id_base64)
                .into_iter()
                .map(GatewayRoute::from_resolved)
                .collect::<Vec<_>>();
            let handle = self.handle.as_ref().ok_or(AgentError::Network)?.clone();
            self.reconcile_gateway_routes(&handle, desired_gateway_routes)
                .await?;
            let desired = policy
                .active_client_prefixes(&state.node_id_base64, chrono::Utc::now())
                .into_iter()
                .collect::<HashSet<_>>();
            let current = self.subnet_routes.keys().copied().collect::<HashSet<_>>();
            if desired == current {
                return Ok(());
            }
            let handle = self.handle.as_ref().ok_or(AgentError::Network)?;
            let additions = desired.difference(&current).copied().collect::<Vec<_>>();
            ensure_subnet_routes_available(handle, &additions, self.interface_index, &current)
                .await?;

            let mut added = Vec::with_capacity(additions.len());
            for prefix in additions {
                let route =
                    subnet_route_message(prefix, self.interface_index, self.plan.virtual_ip());
                if handle.route().add(route.clone()).execute().await.is_err() {
                    for (_, added_route) in added {
                        let _ = handle.route().del(added_route).execute().await;
                    }
                    return Err(AgentError::Network);
                }
                added.push((prefix, route));
            }
            for (prefix, route) in &added {
                self.subnet_routes.insert(*prefix, route.clone());
            }

            let removals = current.difference(&desired).copied().collect::<Vec<_>>();
            for prefix in removals {
                let route = self
                    .subnet_routes
                    .remove(&prefix)
                    .ok_or(AgentError::Network)?;
                if handle.route().del(route.clone()).execute().await.is_err() {
                    self.subnet_routes.insert(prefix, route);
                    return Err(AgentError::Network);
                }
            }
            self.persist_manifest()
        }

        fn persist_manifest(&self) -> Result<()> {
            let mut subnet_routes = self.subnet_routes.keys().copied().collect::<Vec<_>>();
            subnet_routes.sort();
            write_json(
                &self.manifest_path,
                &NetworkManifest {
                    schema_version: 3,
                    interface_name: self.plan.interface_name.clone(),
                    interface_index: self.interface_index,
                    mtu: self.plan.mtu,
                    virtual_ip: self.plan.virtual_ip,
                    address_pool: self.plan.address_pool,
                    subnet_routes,
                    gateway_routes: self.gateway_routes.clone(),
                    forwarding: self.forwarding.clone(),
                    nat_table_name: self.nat_table_name.clone(),
                },
            )
        }

        async fn reconcile_gateway_routes(
            &mut self,
            handle: &Handle,
            desired: Vec<GatewayRoute>,
        ) -> Result<()> {
            validate_gateway_routes(handle, &desired, self.plan.interface_name()).await?;
            if desired == self.gateway_routes {
                if self
                    .forwarding
                    .iter()
                    .any(|record| !matches!(forwarding_value(&record.interface_name), Ok(1)))
                {
                    return Err(AgentError::Network);
                }
                return Ok(());
            }

            let desired_interfaces = if desired.is_empty() {
                HashSet::new()
            } else {
                std::iter::once(self.plan.interface_name().to_owned())
                    .chain(desired.iter().map(|route| route.interface_name.clone()))
                    .collect::<HashSet<_>>()
            };
            for interface_name in &desired_interfaces {
                if !self
                    .forwarding
                    .iter()
                    .any(|record| record.interface_name == *interface_name)
                {
                    self.forwarding.push(ForwardingRecord {
                        interface_name: interface_name.clone(),
                        previous_value: forwarding_value(interface_name)?,
                    });
                }
            }
            self.forwarding
                .sort_by(|left, right| left.interface_name.cmp(&right.interface_name));

            let previous_table = self.nat_table_name.clone();
            let desired_table = desired
                .iter()
                .any(|route| route.mode == xs_core::SubnetRouteMode::Nat)
                .then(|| table_name(self.plan.interface_name()));
            if self.nat_table_name.is_none() {
                self.nat_table_name.clone_from(&desired_table);
            }
            self.gateway_routes.clone_from(&desired);
            self.persist_manifest()?;

            for record in &self.forwarding {
                set_forwarding(&record.interface_name, 0)?;
            }
            if let Some(table) = previous_table.as_ref().or(desired_table.as_ref()) {
                replace_nat_table(
                    table,
                    self.plan.interface_name(),
                    previous_table.is_some(),
                    &desired,
                )?;
            }
            self.nat_table_name = desired_table;
            for interface_name in &desired_interfaces {
                set_forwarding(interface_name, 1)?;
            }
            let removed = self
                .forwarding
                .iter()
                .filter(|record| !desired_interfaces.contains(&record.interface_name))
                .cloned()
                .collect::<Vec<_>>();
            restore_forwarding(&removed)?;
            self.forwarding
                .retain(|record| desired_interfaces.contains(&record.interface_name));
            self.persist_manifest()
        }

        fn cleanup_gateway(&mut self) -> Result<()> {
            let records = self.forwarding.clone();
            let mut cleaned = true;
            for record in &records {
                if set_forwarding(&record.interface_name, 0).is_err() {
                    cleaned = false;
                }
            }
            if let Some(table) = self.nat_table_name.take()
                && delete_nat_table(&table).is_err()
            {
                cleaned = false;
            }
            if restore_forwarding(&records).is_err() {
                cleaned = false;
            }
            self.forwarding.clear();
            self.gateway_routes.clear();
            if !cleaned {
                return Err(AgentError::Network);
            }
            Ok(())
        }

        /// Receives one raw layer-three packet from the TUN device.
        ///
        /// # Errors
        ///
        /// Returns [`AgentError::Network`] when the device is closed or the read fails.
        pub async fn receive(&self, buffer: &mut [u8]) -> Result<usize> {
            self.device
                .as_ref()
                .ok_or(AgentError::Network)?
                .recv(buffer)
                .await
                .map_err(|_| AgentError::Network)
        }

        /// Sends one complete raw layer-three packet to the TUN device.
        ///
        /// # Errors
        ///
        /// Returns [`AgentError::Network`] when the device is closed or the write fails.
        pub async fn send(&self, packet: &[u8]) -> Result<()> {
            self.device
                .as_ref()
                .ok_or(AgentError::Network)?
                .send_all(packet)
                .await
                .map_err(|_| AgentError::Network)
        }
    }

    impl Drop for TunNetwork {
        fn drop(&mut self) {
            drop(self.device.take());
            if let Some(connection) = self.connection.take() {
                connection.abort();
            }
        }
    }

    async fn configure_link(handle: &Handle, plan: &NetworkPlan) -> Result<(u32, RouteMessage)> {
        let index = find_link_index(handle, plan.interface_name())
            .await?
            .ok_or(AgentError::Network)?;
        handle
            .link()
            .set(
                LinkUnspec::new_with_index(index)
                    .mtu(u32::from(plan.mtu()))
                    .up()
                    .build(),
            )
            .execute()
            .await
            .map_err(|_| AgentError::Network)?;
        handle
            .address()
            .add(index, IpAddr::V4(plan.virtual_ip()), 32)
            .execute()
            .await
            .map_err(|_| AgentError::Network)?;
        let route = RouteMessageBuilder::<Ipv4Addr>::new()
            .destination_prefix(
                plan.address_pool().network(),
                plan.address_pool().prefix_len(),
            )
            .output_interface(index)
            .pref_source(plan.virtual_ip())
            .build();
        handle
            .route()
            .add(route.clone())
            .execute()
            .await
            .map_err(|_| AgentError::Network)?;
        Ok((index, route))
    }

    async fn find_link_index(handle: &Handle, name: &str) -> Result<Option<u32>> {
        let mut links = handle.link().get().execute();
        let mut matching_index = None;
        while let Some(link) = links.try_next().await.map_err(|_| AgentError::Network)? {
            let matches_name = link.attributes.iter().any(
                |attribute| matches!(attribute, LinkAttribute::IfName(value) if value == name),
            );
            if matches_name && matching_index.replace(link.header.index).is_some() {
                return Err(AgentError::Network);
            }
        }
        Ok(matching_index)
    }

    async fn ensure_route_available(handle: &Handle, desired: Ipv4Net) -> Result<()> {
        let mut routes = handle
            .route()
            .get(RouteMessageBuilder::<Ipv4Addr>::new().build())
            .execute();
        while let Some(route) = routes.try_next().await.map_err(|_| AgentError::Network)? {
            let prefix = route.header.destination_prefix_length;
            if prefix == 0 {
                continue;
            }
            let destination = route.attributes.iter().find_map(|attribute| {
                if let RouteAttribute::Destination(RouteAddress::Inet(address)) = attribute {
                    Some(*address)
                } else {
                    None
                }
            });
            let Some(destination) = destination else {
                return Err(AgentError::Network);
            };
            let existing = Ipv4Net::new(destination, prefix).map_err(|_| AgentError::Network)?;
            if networks_overlap(existing, desired) {
                return Err(AgentError::Network);
            }
        }
        Ok(())
    }

    async fn ensure_subnet_routes_available(
        handle: &Handle,
        desired: &[Ipv4Net],
        project_interface_index: u32,
        managed: &HashSet<Ipv4Net>,
    ) -> Result<()> {
        if desired.is_empty() {
            return Ok(());
        }
        let mut routes = handle
            .route()
            .get(RouteMessageBuilder::<Ipv4Addr>::new().build())
            .execute();
        while let Some(route) = routes.try_next().await.map_err(|_| AgentError::Network)? {
            let prefix = route.header.destination_prefix_length;
            if prefix == 0 {
                continue;
            }
            let destination = route.attributes.iter().find_map(|attribute| {
                if let RouteAttribute::Destination(RouteAddress::Inet(address)) = attribute {
                    Some(*address)
                } else {
                    None
                }
            });
            let Some(destination) = destination else {
                return Err(AgentError::Network);
            };
            let existing = Ipv4Net::new(destination, prefix).map_err(|_| AgentError::Network)?;
            let project_owned = managed.contains(&existing)
                && route.attributes.iter().any(|attribute| {
                    matches!(attribute, RouteAttribute::Oif(index) if *index == project_interface_index)
                });
            if !project_owned
                && desired
                    .iter()
                    .any(|candidate| networks_overlap(existing, *candidate))
            {
                return Err(AgentError::Network);
            }
        }
        Ok(())
    }

    fn subnet_route_message(
        prefix: Ipv4Net,
        interface_index: u32,
        preferred_source: Ipv4Addr,
    ) -> RouteMessage {
        RouteMessageBuilder::<Ipv4Addr>::new()
            .destination_prefix(prefix.network(), prefix.prefix_len())
            .output_interface(interface_index)
            .pref_source(preferred_source)
            .build()
    }

    fn networks_overlap(left: Ipv4Net, right: Ipv4Net) -> bool {
        left.contains(&right.network()) || right.contains(&left.network())
    }

    async fn wait_for_link_removal(handle: &Handle, name: &str) -> bool {
        for _ in 0..40 {
            if matches!(find_link_index(handle, name).await, Ok(None)) {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        false
    }

    fn remove_manifest(path: &Path) -> Result<()> {
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(AgentError::Network),
        }
    }

    fn cleanup_manifest_gateway(manifest: &NetworkManifest) -> Result<()> {
        let mut cleaned = true;
        let active_records = manifest
            .forwarding
            .iter()
            .filter(|record| forwarding_path_exists(&record.interface_name))
            .cloned()
            .collect::<Vec<_>>();
        for record in &active_records {
            if set_forwarding(&record.interface_name, 0).is_err() {
                cleaned = false;
            }
        }
        if let Some(table) = &manifest.nat_table_name
            && delete_nat_table(table).is_err()
        {
            cleaned = false;
        }
        if restore_forwarding(&active_records).is_err() {
            cleaned = false;
        }
        if !cleaned {
            return Err(AgentError::Network);
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::networks_overlap;

        #[test]
        fn overlap_detection_handles_both_prefix_directions() {
            assert!(networks_overlap(
                "100.88.0.0/16".parse().expect("left"),
                "100.88.10.0/24".parse().expect("right"),
            ));
            assert!(networks_overlap(
                "100.88.10.0/24".parse().expect("left"),
                "100.88.0.0/16".parse().expect("right"),
            ));
            assert!(!networks_overlap(
                "100.88.0.0/16".parse().expect("left"),
                "100.89.0.0/16".parse().expect("right"),
            ));
        }
    }
}

#[cfg(target_os = "linux")]
pub use platform::TunNetwork;

#[cfg(not(target_os = "linux"))]
pub struct TunNetwork;

#[cfg(not(target_os = "linux"))]
impl TunNetwork {
    /// Rejects TUN creation on non-Linux targets until the native platform implementation exists.
    ///
    /// # Errors
    ///
    /// Always returns [`AgentError::UnsupportedPlatform`].
    pub async fn create(_plan: NetworkPlan, _manifest_path: &std::path::Path) -> Result<Self> {
        Err(AgentError::UnsupportedPlatform)
    }
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use super::NetworkPlan;

    fn plan(pool: &str, address: Ipv4Addr) -> NetworkPlan {
        NetworkPlan {
            interface_name: "xstest0".to_owned(),
            mtu: 1280,
            virtual_ip: address,
            address_pool: pool.parse().expect("valid test pool"),
        }
    }

    #[test]
    fn plan_rejects_default_broad_and_tiny_routes() {
        assert!(
            plan("0.0.0.0/0", Ipv4Addr::new(100, 88, 0, 1))
                .validate()
                .is_err()
        );
        assert!(
            plan("0.0.0.0/7", Ipv4Addr::new(100, 88, 0, 1))
                .validate()
                .is_err()
        );
        assert!(
            plan("100.88.0.0/31", Ipv4Addr::new(100, 88, 0, 1))
                .validate()
                .is_err()
        );
    }

    #[test]
    fn plan_requires_a_usable_address_inside_the_pool() {
        assert!(
            plan("100.88.0.0/24", Ipv4Addr::new(100, 88, 0, 16))
                .validate()
                .is_ok()
        );
        assert!(
            plan("100.88.0.0/24", Ipv4Addr::new(100, 89, 0, 16))
                .validate()
                .is_err()
        );
        assert!(
            plan("100.88.0.0/24", Ipv4Addr::new(100, 88, 0, 0))
                .validate()
                .is_err()
        );
        assert!(
            plan("100.88.0.0/24", Ipv4Addr::new(100, 88, 0, 255))
                .validate()
                .is_err()
        );
    }
}
