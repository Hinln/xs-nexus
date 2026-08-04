use std::{path::Path, time::Duration};

use xs_windows_wintun::{DEFAULT_RING_CAPACITY, WintunSession};

use super::NetworkPlan;
use crate::{
    config::WindowsWintunConfig,
    error::{AgentError, Result},
    state::NodeState,
    windows_network::WindowsNetworkPreparation,
};

/// Windows layer-three network implementation backed by the vendor-signed Wintun adapter.
///
/// Wintun owns only the virtual adapter and packet ring. XS Nexus continues to own encrypted
/// transport, peer selection, ACL policy, address allocation, and route policy.
pub struct TunNetwork {
    session: WintunSession,
    preparation: Option<WindowsNetworkPreparation>,
    manifest_path: std::path::PathBuf,
}

#[allow(clippy::unused_async)] // Mirrors the async Linux lifecycle so runtime orchestration stays platform-neutral.
impl TunNetwork {
    /// Creates one ephemeral Wintun adapter after recovering only exact stale project state.
    ///
    /// # Errors
    ///
    /// Fails closed when the release-pinned Wintun library cannot be verified, a stale manifest
    /// overlaps foreign state, adapter creation fails, or address/DAD preparation fails.
    pub async fn create(
        plan: NetworkPlan,
        manifest_path: &Path,
        temporary_path: &Path,
        windows_wintun: Option<&WindowsWintunConfig>,
    ) -> Result<Self> {
        let wintun = windows_wintun.ok_or(AgentError::Configuration)?;
        let hash = wintun.sha256_bytes()?;
        WindowsNetworkPreparation::recover_after_adapter_removal(&plan, manifest_path)?;
        let session = WintunSession::create(
            &wintun.library_path,
            &hash,
            plan.interface_name(),
            "XS Nexus",
            DEFAULT_RING_CAPACITY,
        )
        .map_err(|_| AgentError::Network)?;
        let preparation = WindowsNetworkPreparation::prepare_for_adapter(
            &plan,
            session.interface_luid(),
            &[],
            manifest_path,
            temporary_path,
        )?;
        Ok(Self {
            session,
            preparation: Some(preparation),
            manifest_path: manifest_path.to_path_buf(),
        })
    }

    #[must_use]
    pub const fn interface_index(&self) -> u32 {
        self.session.interface_index()
    }

    /// Waits for and copies one raw IPv4 packet from the local adapter.
    ///
    /// # Errors
    ///
    /// Stops the Agent on malformed packets or a Wintun API error. An empty ring waits with a
    /// bounded timer so shutdown and control traffic remain responsive.
    pub async fn receive(&self, buffer: &mut [u8]) -> Result<usize> {
        loop {
            match self
                .session
                .try_receive(buffer)
                .map_err(|_| AgentError::Network)?
            {
                Some(length) if valid_ipv4(&buffer[..length]) => return Ok(length),
                Some(_) => return Err(AgentError::Network),
                None => tokio::time::sleep(Duration::from_millis(10)).await,
            }
        }
    }

    /// Copies one validated raw IPv4 packet to the local adapter.
    ///
    /// # Errors
    ///
    /// Rejects malformed packets rather than giving arbitrary local traffic to the driver.
    pub async fn send(&self, packet: &[u8]) -> Result<()> {
        if !valid_ipv4(packet) {
            return Err(AgentError::Network);
        }
        self.session.send(packet).map_err(|_| AgentError::Network)
    }

    /// Windows client-side subnet routing is deliberately not enabled until its transaction and
    /// crash recovery are exercised with the Wintun adapter in a VM.
    ///
    /// # Errors
    ///
    /// Rejects any non-empty approved route set rather than mutating Windows routes without the
    /// required live evidence.
    pub async fn reconcile_subnet_routes(&mut self, state: &NodeState) -> Result<()> {
        if state.configuration_payload.subnet_routes.is_empty() {
            Ok(())
        } else {
            Err(AgentError::UnsupportedPlatform)
        }
    }

    /// Removes exact address and route state before the Wintun adapter handle is released.
    ///
    /// # Errors
    ///
    /// Returns a network error if any recorded resource cannot be removed.
    pub async fn shutdown(mut self) -> Result<()> {
        let preparation = self.preparation.take().ok_or(AgentError::Network)?;
        preparation.shutdown(&self.manifest_path)
    }

    /// Cleans only a stale manifest after its prior ephemeral adapter was removed.
    ///
    /// # Errors
    ///
    /// Fails closed on malformed manifests or ownership drift.
    pub async fn recover_stale(plan: &NetworkPlan, manifest_path: &Path) -> Result<()> {
        WindowsNetworkPreparation::recover_after_adapter_removal(plan, manifest_path)
    }
}

fn valid_ipv4(packet: &[u8]) -> bool {
    if packet.len() < 20 || packet[0] >> 4 != 4 {
        return false;
    }
    let header_length = usize::from(packet[0] & 0x0f) * 4;
    let total_length = usize::from(u16::from_be_bytes([packet[2], packet[3]]));
    header_length >= 20 && header_length <= packet.len() && total_length == packet.len()
}

#[cfg(test)]
mod tests {
    use super::valid_ipv4;

    #[test]
    fn packet_validation_requires_exact_ipv4_total_length() {
        let mut packet = vec![0_u8; 20];
        packet[0] = 0x45;
        packet[2..4].copy_from_slice(&20_u16.to_be_bytes());
        assert!(valid_ipv4(&packet));
        packet[3] = 19;
        assert!(!valid_ipv4(&packet));
    }
}
