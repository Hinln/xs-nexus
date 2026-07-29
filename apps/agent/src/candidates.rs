use std::{
    collections::{HashMap, HashSet},
    net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV6},
    sync::Arc,
    time::{Duration, Instant},
};

use chrono::{Duration as ChronoDuration, Utc};
use ed25519_dalek::VerifyingKey;
use futures_util::TryStreamExt as _;
use getrandom::fill;
use rtnetlink::{new_connection, packet_route::address::AddressAttribute};
use tokio::net::UdpSocket;
use xs_core::{CandidateAdvertisement, EndpointCandidate, EndpointCandidateKind};
use xs_protocol::{
    CREDENTIAL_LENGTH, DISCOVERY_RESPONSE_LENGTH, DiscoveryRequest, verify_discovery_response,
};

use crate::{
    error::{AgentError, Result},
    state::{NodeState, decode_fixed},
    storage::Identity,
};

const CANDIDATE_LIFETIME: ChronoDuration = ChronoDuration::minutes(10);
#[cfg(not(feature = "privileged-network-tests"))]
const CANDIDATE_REFRESH_INTERVAL: Duration = Duration::from_secs(4 * 60);
#[cfg(feature = "privileged-network-tests")]
const CANDIDATE_REFRESH_INTERVAL: Duration = Duration::from_secs(2);
const MAX_LOCAL_CANDIDATES: usize = 12;
const MAX_DISCOVERY_ENDPOINTS: usize = 8;
const MAX_ADVERTISED_CANDIDATES: usize = 16;

pub struct CandidateManager {
    network_id: uuid::Uuid,
    node_id_base64: String,
    node_id: [u8; 16],
    credential: [u8; CREDENTIAL_LENGTH],
    configuration_verifying_key: VerifyingKey,
    identity: Arc<Identity>,
    generation: u64,
    local: Vec<(EndpointCandidateKind, SocketAddr)>,
    mapped: HashMap<SocketAddr, chrono::DateTime<Utc>>,
    pending: HashMap<SocketAddr, DiscoveryRequest>,
    next_refresh: Instant,
    dirty: bool,
}

impl CandidateManager {
    /// Creates candidate discovery state from validated persisted node state.
    ///
    /// # Errors
    ///
    /// Returns an Agent error when persisted credentials, identities, or trust keys are invalid.
    pub fn new(state: &NodeState, identity: Arc<Identity>) -> Result<Self> {
        let configuration_verifying_key = VerifyingKey::from_bytes(&decode_fixed::<32>(
            &state.configuration_signing_public_key_base64,
        )?)
        .map_err(|_| AgentError::ControllerTrust)?;
        Ok(Self {
            network_id: state.network_id,
            node_id_base64: state.node_id_base64.clone(),
            node_id: decode_fixed::<16>(&state.node_id_base64)?,
            credential: decode_fixed::<CREDENTIAL_LENGTH>(&state.credential_base64)?,
            configuration_verifying_key,
            identity,
            generation: state.candidate_generation,
            local: Vec::new(),
            mapped: HashMap::new(),
            pending: HashMap::new(),
            next_refresh: Instant::now(),
            dirty: true,
        })
    }

    #[must_use]
    pub fn refresh_due(&self, now: Instant) -> bool {
        now >= self.next_refresh
    }

    pub fn force_refresh(&mut self) {
        self.next_refresh = Instant::now();
    }

    /// Refreshes interface candidates and starts authenticated mapping discovery.
    ///
    /// # Errors
    ///
    /// Returns an Agent error when Netlink enumeration, randomness, time, or socket inspection
    /// fails.
    pub async fn refresh(&mut self, socket: &UdpSocket, state: &NodeState) -> Result<()> {
        let port = socket.local_addr().map_err(|_| AgentError::Network)?.port();
        let local = collect_local_endpoints(port, state.virtual_ip).await?;
        if local != self.local {
            self.local = local;
        }
        self.mapped.retain(|_, expires_at| *expires_at > Utc::now());
        self.pending.clear();

        for endpoint in state
            .configuration_payload
            .discovery_endpoints
            .iter()
            .copied()
            .take(MAX_DISCOVERY_ENDPOINTS)
        {
            let request_id = random_array()?;
            let request = DiscoveryRequest::new(
                *self.network_id.as_bytes(),
                self.node_id,
                request_id,
                unix_time()?,
                self.credential,
                self.identity.signing_key(),
            )
            .map_err(|_| AgentError::DataPlane)?;
            if socket.send_to(request.encoded(), endpoint).await.is_ok() {
                self.pending.insert(normalize_endpoint(endpoint), request);
            }
        }

        self.next_refresh = Instant::now() + CANDIDATE_REFRESH_INTERVAL;
        self.dirty = true;
        Ok(())
    }

    #[must_use]
    pub fn handles_discovery_response(&self, datagram: &[u8]) -> bool {
        datagram.len() == DISCOVERY_RESPONSE_LENGTH && datagram.starts_with(b"XSD1")
    }

    pub fn handle_discovery_response(&mut self, source: SocketAddr, datagram: &[u8]) -> bool {
        let source = normalize_endpoint(source);
        let Some(request) = self.pending.get(&source) else {
            return false;
        };
        let Ok(response) = verify_discovery_response(
            datagram,
            request,
            &self.configuration_verifying_key,
            match unix_time() {
                Ok(now) => now,
                Err(_) => return false,
            },
        ) else {
            return false;
        };
        self.pending.remove(&source);
        self.mapped.insert(
            normalize_endpoint(response.observed_endpoint),
            Utc::now() + CANDIDATE_LIFETIME,
        );
        self.dirty = true;
        true
    }

    pub fn take_advertisement(&mut self) -> Option<CandidateAdvertisement> {
        if !self.dirty {
            return None;
        }
        let now = Utc::now();
        self.mapped.retain(|_, expires_at| *expires_at > now);
        let expires_at = now + CANDIDATE_LIFETIME;
        let candidates =
            prioritize_candidates(&self.local, self.mapped.keys().copied(), expires_at);
        let clock_generation = u64::try_from(now.timestamp_millis()).unwrap_or(1).max(1);
        self.generation = self.generation.saturating_add(1).max(clock_generation);
        self.dirty = false;
        Some(CandidateAdvertisement {
            schema_version: 1,
            network_id: self.network_id,
            node_id_base64: self.node_id_base64.clone(),
            generation: self.generation,
            generated_at: now,
            expires_at,
            candidates,
        })
    }

    #[must_use]
    pub fn candidates(&self) -> Vec<EndpointCandidate> {
        prioritize_candidates(
            &self.local,
            self.mapped.keys().copied(),
            Utc::now() + CANDIDATE_LIFETIME,
        )
    }
}

async fn collect_local_endpoints(
    port: u16,
    virtual_ip: Ipv4Addr,
) -> Result<Vec<(EndpointCandidateKind, SocketAddr)>> {
    let (connection, handle, _) = new_connection().map_err(|_| AgentError::Network)?;
    let connection = tokio::spawn(connection);
    let mut messages = handle.address().get().execute();
    let mut endpoints = HashSet::new();
    while let Some(message) = messages.try_next().await.map_err(|_| AgentError::Network)? {
        let address = message
            .attributes
            .iter()
            .find_map(|attribute| match attribute {
                AddressAttribute::Local(address) => Some(*address),
                _ => None,
            })
            .or_else(|| {
                message
                    .attributes
                    .iter()
                    .find_map(|attribute| match attribute {
                        AddressAttribute::Address(address) => Some(*address),
                        _ => None,
                    })
            });
        let Some(address) = address else {
            continue;
        };
        if let Some(candidate) = local_candidate(address, message.header.index, port, virtual_ip) {
            endpoints.insert(candidate);
        }
    }
    connection.abort();
    let mut endpoints = endpoints.into_iter().collect::<Vec<_>>();
    endpoints.sort_by_key(|(kind, endpoint)| (candidate_rank(*kind), *endpoint));
    endpoints.truncate(MAX_LOCAL_CANDIDATES);
    Ok(endpoints)
}

fn local_candidate(
    address: IpAddr,
    interface_index: u32,
    port: u16,
    virtual_ip: Ipv4Addr,
) -> Option<(EndpointCandidateKind, SocketAddr)> {
    match address {
        IpAddr::V4(address)
            if !address.is_unspecified()
                && !address.is_loopback()
                && !address.is_multicast()
                && address != Ipv4Addr::BROADCAST
                && address != virtual_ip =>
        {
            Some((
                EndpointCandidateKind::Local,
                SocketAddr::new(IpAddr::V4(address), port),
            ))
        }
        IpAddr::V6(address)
            if !address.is_unspecified() && !address.is_loopback() && !address.is_multicast() =>
        {
            let scope_id = if address.is_unicast_link_local() {
                interface_index
            } else {
                0
            };
            let endpoint = SocketAddr::V6(SocketAddrV6::new(address, port, 0, scope_id));
            let kind = if address.is_unique_local() || address.is_unicast_link_local() {
                EndpointCandidateKind::Local
            } else {
                EndpointCandidateKind::PublicIpv6
            };
            Some((kind, endpoint))
        }
        IpAddr::V4(_) | IpAddr::V6(_) => None,
    }
}

fn prioritize_candidates(
    local: &[(EndpointCandidateKind, SocketAddr)],
    mapped: impl IntoIterator<Item = SocketAddr>,
    expires_at: chrono::DateTime<Utc>,
) -> Vec<EndpointCandidate> {
    let mut raw = local.to_vec();
    raw.extend(
        mapped
            .into_iter()
            .map(|endpoint| (EndpointCandidateKind::Mapped, endpoint)),
    );
    raw.sort_by_key(|(kind, endpoint)| (candidate_rank(*kind), *endpoint));
    let mut seen = HashSet::new();
    raw.into_iter()
        .filter(|(_, endpoint)| seen.insert(*endpoint))
        .take(MAX_ADVERTISED_CANDIDATES)
        .enumerate()
        .map(|(index, (kind, endpoint))| EndpointCandidate {
            kind,
            endpoint,
            priority: 10_000_u32.saturating_sub(u32::try_from(index).unwrap_or(u32::MAX)),
            expires_at,
        })
        .collect()
}

const fn candidate_rank(kind: EndpointCandidateKind) -> u8 {
    match kind {
        EndpointCandidateKind::Local => 0,
        EndpointCandidateKind::PublicIpv6 => 1,
        EndpointCandidateKind::Mapped => 2,
        EndpointCandidateKind::Static => 3,
        EndpointCandidateKind::Relay => 4,
    }
}

#[must_use]
pub fn normalize_endpoint(endpoint: SocketAddr) -> SocketAddr {
    match endpoint {
        SocketAddr::V6(endpoint) => endpoint
            .ip()
            .to_ipv4_mapped()
            .map_or(SocketAddr::V6(endpoint), |address| {
                SocketAddr::new(IpAddr::V4(address), endpoint.port())
            }),
        SocketAddr::V4(_) => endpoint,
    }
}

fn random_array<const LENGTH: usize>() -> Result<[u8; LENGTH]> {
    let mut bytes = [0_u8; LENGTH];
    fill(&mut bytes).map_err(|_| AgentError::DataPlane)?;
    Ok(bytes)
}

fn unix_time() -> Result<u64> {
    u64::try_from(Utc::now().timestamp()).map_err(|_| AgentError::DataPlane)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_priority_prefers_lan_then_ipv6_then_mapping() {
        let expires_at = Utc::now() + CANDIDATE_LIFETIME;
        let candidates = prioritize_candidates(
            &[
                (
                    EndpointCandidateKind::PublicIpv6,
                    "[2001:db8::2]:42000".parse().expect("IPv6 endpoint"),
                ),
                (
                    EndpointCandidateKind::Local,
                    "192.168.1.2:42000".parse().expect("LAN endpoint"),
                ),
            ],
            ["198.51.100.2:52000".parse().expect("mapped endpoint")],
            expires_at,
        );
        assert_eq!(candidates[0].kind, EndpointCandidateKind::Local);
        assert_eq!(candidates[1].kind, EndpointCandidateKind::PublicIpv6);
        assert_eq!(candidates[2].kind, EndpointCandidateKind::Mapped);
        assert!(
            candidates
                .windows(2)
                .all(|pair| pair[0].priority > pair[1].priority)
        );
    }

    #[test]
    fn ipv4_mapped_sources_are_canonicalized() {
        assert_eq!(
            normalize_endpoint("[::ffff:192.0.2.7]:42000".parse().expect("mapped IPv6")),
            "192.0.2.7:42000".parse().expect("IPv4 endpoint")
        );
    }
}
