use std::path::Path;

use ipnet::Ipv4Net;
use xs_windows_route_manager::{
    IpHelperBackend, ManifestState, NetworkManifest, RouteKey, exact_address_present,
    execute_recovery, plan_reconcile, plan_recovery, provision_network, read_network_manifest,
    remove_network_manifest, snapshot_routes, write_network_manifest_atomic,
};

use crate::{
    error::{AgentError, Result},
    network::NetworkPlan,
    windows_xsnet::{Win32DeviceTransport, XsnetDeviceSession},
};

pub struct WindowsNetworkPreparation {
    manifest: NetworkManifest,
}

impl WindowsNetworkPreparation {
    /// Recovers stale state using the authoritative LUID bound to this exact xsnet session.
    ///
    /// # Errors
    ///
    /// Returns an error for manifest drift, ownership conflicts, IP Helper failures, or cleanup
    /// failures.
    pub fn recover_for_session(
        plan: &NetworkPlan,
        session: &XsnetDeviceSession<Win32DeviceTransport>,
        manifest_path: &Path,
    ) -> Result<()> {
        Self::recover_stale(plan, session.interface_luid(), manifest_path)
    }

    /// Prepares address and routes for the authoritative LUID bound to this exact xsnet session.
    ///
    /// # Errors
    ///
    /// Returns an error for stale recovery, unsafe/conflicting routes, persistence, DAD, IP
    /// Helper, or rollback failure.
    pub fn prepare_for_session(
        plan: &NetworkPlan,
        session: &XsnetDeviceSession<Win32DeviceTransport>,
        desired_prefixes: &[Ipv4Net],
        manifest_path: &Path,
        temporary_path: &Path,
    ) -> Result<Self> {
        Self::prepare(
            plan,
            session.interface_luid(),
            desired_prefixes,
            manifest_path,
            temporary_path,
        )
    }

    /// Recovers a trusted stale manifest before any new Windows network mutation.
    ///
    /// # Errors
    ///
    /// Fails closed for plan mismatch, ownership drift, IP Helper failure, cleanup failure, or
    /// private manifest removal failure.
    fn recover_stale(plan: &NetworkPlan, interface_luid: u64, manifest_path: &Path) -> Result<()> {
        if !manifest_path.exists() {
            return Ok(());
        }
        let manifest = read_network_manifest(manifest_path).map_err(|_| AgentError::Network)?;
        validate_manifest_plan(&manifest, plan, interface_luid)?;
        let system = snapshot_routes().map_err(|_| AgentError::Network)?;
        let address_present = exact_address_present(
            interface_luid,
            plan.virtual_ip(),
            plan.address_pool().prefix_len(),
        )
        .map_err(|_| AgentError::Network)?;
        let recovery =
            plan_recovery(&manifest, &system, address_present).map_err(|_| AgentError::Network)?;
        let mut backend = IpHelperBackend;
        execute_recovery(&mut backend, &manifest, &recovery).map_err(|_| AgentError::Network)?;
        remove_network_manifest(manifest_path).map_err(|_| AgentError::Network)
    }

    /// Persists Preparing state, proves DAD, applies exact routes, then atomically marks Active.
    ///
    /// # Errors
    ///
    /// Fails closed for stale recovery, unsafe/conflicting routes, persistence, DAD, IP Helper, or
    /// rollback failure. A post-persistence failure deliberately leaves Preparing state for the
    /// next trusted recovery.
    fn prepare(
        plan: &NetworkPlan,
        interface_luid: u64,
        desired_prefixes: &[Ipv4Net],
        manifest_path: &Path,
        temporary_path: &Path,
    ) -> Result<Self> {
        Self::recover_stale(plan, interface_luid, manifest_path)?;
        let system = snapshot_routes().map_err(|_| AgentError::Network)?;
        let route_plan = plan_reconcile(interface_luid, desired_prefixes, &[], &system)
            .map_err(|_| AgentError::Network)?;
        let mut routes = desired_prefixes
            .iter()
            .copied()
            .map(|prefix| RouteKey::project(prefix, interface_luid))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|_| AgentError::Network)?;
        routes.sort_by_key(|route| (u32::from(route.prefix.network()), route.prefix.prefix_len()));
        let preparing = NetworkManifest::new(
            ManifestState::Preparing,
            interface_luid,
            plan.virtual_ip(),
            plan.address_pool().prefix_len(),
            routes,
        )
        .map_err(|_| AgentError::Network)?;
        write_network_manifest_atomic(manifest_path, temporary_path, &preparing)
            .map_err(|_| AgentError::Network)?;
        let mut backend = IpHelperBackend;
        provision_network(
            &mut backend,
            interface_luid,
            plan.virtual_ip(),
            plan.address_pool().prefix_len(),
            100,
            &route_plan,
        )
        .map_err(|_| AgentError::Network)?;
        let active = NetworkManifest::new(
            ManifestState::Active,
            interface_luid,
            plan.virtual_ip(),
            plan.address_pool().prefix_len(),
            preparing.routes.clone(),
        )
        .map_err(|_| AgentError::Network)?;
        write_network_manifest_atomic(manifest_path, temporary_path, &active)
            .map_err(|_| AgentError::Network)?;
        Ok(Self { manifest: active })
    }

    /// Cleans exact active resources and removes the manifest only after complete success.
    ///
    /// # Errors
    ///
    /// Returns ownership, IP Helper, cleanup, or private manifest removal failure.
    pub fn shutdown(self, manifest_path: &Path) -> Result<()> {
        let system = snapshot_routes().map_err(|_| AgentError::Network)?;
        let address_present = exact_address_present(
            self.manifest.interface_luid,
            self.manifest.address,
            self.manifest.prefix_length,
        )
        .map_err(|_| AgentError::Network)?;
        let recovery = plan_recovery(&self.manifest, &system, address_present)
            .map_err(|_| AgentError::Network)?;
        let mut backend = IpHelperBackend;
        execute_recovery(&mut backend, &self.manifest, &recovery)
            .map_err(|_| AgentError::Network)?;
        remove_network_manifest(manifest_path).map_err(|_| AgentError::Network)
    }
}

fn validate_manifest_plan(
    manifest: &NetworkManifest,
    plan: &NetworkPlan,
    interface_luid: u64,
) -> Result<()> {
    if manifest.interface_luid != interface_luid
        || manifest.address != plan.virtual_ip()
        || manifest.prefix_length != plan.address_pool().prefix_len()
    {
        return Err(AgentError::Network);
    }
    Ok(())
}
