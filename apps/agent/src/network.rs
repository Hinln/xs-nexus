use std::net::Ipv4Addr;

use ipnet::Ipv4Net;
use serde::{Deserialize, Serialize};

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
}

#[cfg(target_os = "linux")]
mod platform {
    use std::{
        net::{IpAddr, Ipv4Addr},
        path::{Path, PathBuf},
        time::Duration,
    };

    use futures_util::TryStreamExt as _;
    use rtnetlink::{
        Handle, LinkUnspec, RouteMessageBuilder, new_connection,
        packet_route::{link::LinkAttribute, route::RouteMessage},
    };
    use tokio::task::JoinHandle;
    use tokio_tun::{Tun, TunBuilder};

    use super::{NetworkManifest, NetworkPlan};
    use crate::{
        error::{AgentError, Result},
        storage::{read_json, write_json},
    };

    pub struct TunNetwork {
        plan: NetworkPlan,
        interface_index: u32,
        device: Option<Tun>,
        handle: Option<Handle>,
        connection: Option<JoinHandle<()>>,
        route: Option<RouteMessage>,
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
                schema_version: 1,
                interface_name: plan.interface_name.clone(),
                interface_index,
                mtu: plan.mtu,
                virtual_ip: plan.virtual_ip,
                address_pool: plan.address_pool,
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
            if route_result.is_err() || !interface_removed || manifest_result.is_err() {
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
            if manifest.schema_version != 1
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
