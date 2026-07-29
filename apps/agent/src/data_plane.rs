use std::{
    collections::{HashMap, VecDeque},
    net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6},
    sync::Arc,
    time::{Duration, Instant},
};

use chrono::Utc;
use ed25519_dalek::VerifyingKey;
use getrandom::fill;
use sha2::{Digest, Sha256};
use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::UdpSocket;
use xs_core::{
    CandidateAdvertisement, EndpointCandidate, EndpointCandidateKind, PathSelectionReason,
};
use xs_protocol::{
    CLIENT_FINISH_TYPE, CLIENT_HELLO_TYPE, CREDENTIAL_LENGTH, ClientFinishSent,
    ClientHandshakeParameters, ClientHelloSent, DataFlags, DataReceiver, DataSender,
    EphemeralPrivateKey, EstablishedSession, HandshakeContext, MAX_DATAGRAM_LENGTH,
    MAX_ENCRYPTED_PAYLOAD_LENGTH, PacketType, SERVER_FINISH_TYPE, SERVER_HELLO_TYPE,
    ServerHandshakeParameters, ServerHelloSent, key_update_payload, verify_key_update_payload,
};

use crate::{
    candidates::{CandidateManager, normalize_endpoint},
    error::{AgentError, Result},
    state::{NodeState, decode_fixed},
    storage::Identity,
};

const MAX_CONFIGURED_PEERS: usize = 1024;
const MAX_QUEUED_PACKETS_PER_PEER: usize = 32;
const MAX_QUEUED_BYTES_PER_PEER: usize = 64 * 1024;
const MAX_RECENT_CLIENT_HELLOS: usize = 64;
const CLIENT_HELLO_CACHE_LIFETIME: Duration = Duration::from_secs(300);
const HANDSHAKE_RETRY_INTERVAL: Duration = Duration::from_millis(300);
const HANDSHAKE_MAX_ATTEMPTS: u8 = 6;
const KEY_UPDATE_RETRY_INTERVAL: Duration = Duration::from_millis(300);
const KEY_UPDATE_MAX_ATTEMPTS: u8 = 6;
const PATH_PROBE_RETRY_INTERVAL: Duration = Duration::from_millis(300);
const PATH_PROBE_MAX_ATTEMPTS: u8 = 4;
const PATH_PROBE_COOLDOWN: Duration = Duration::from_secs(30);
#[cfg(not(feature = "privileged-network-tests"))]
const KEY_UPDATE_PACKET_LIMIT: u64 = 1 << 20;
#[cfg(feature = "privileged-network-tests")]
const KEY_UPDATE_PACKET_LIMIT: u64 = 4;
#[cfg(not(feature = "privileged-network-tests"))]
const KEY_UPDATE_INTERVAL: Duration = Duration::from_secs(60 * 60);
#[cfg(feature = "privileged-network-tests")]
const KEY_UPDATE_INTERVAL: Duration = Duration::from_secs(2);
#[cfg(not(feature = "privileged-network-tests"))]
const PREVIOUS_EPOCH_RETENTION: Duration = Duration::from_secs(30);
#[cfg(feature = "privileged-network-tests")]
const PREVIOUS_EPOCH_RETENTION: Duration = Duration::from_secs(5);

struct LocalMaterial {
    network_id: [u8; 16],
    node_id: [u8; 16],
    virtual_ip: Ipv4Addr,
    credential: [u8; CREDENTIAL_LENGTH],
    controller_verifying_key: VerifyingKey,
    identity: Arc<Identity>,
}

struct Peer {
    node_id: [u8; 16],
    virtual_ip: Ipv4Addr,
    candidates: Vec<EndpointCandidate>,
    active_endpoint: Option<SocketAddr>,
    path_reason: Option<PathSelectionReason>,
    state: PeerState,
    queued_packets: VecDeque<Vec<u8>>,
    queued_bytes: usize,
    recent_client_hellos: VecDeque<([u8; 32], Instant)>,
    pending_path_probe: Option<PendingPathProbe>,
    path_probe_retry_after: Instant,
}

enum PeerState {
    Idle,
    ClientHello(Box<ClientHelloPending>),
    ClientFinish(Box<ClientFinishPending>),
    ServerHello(Box<ServerHelloPending>),
    Established(Box<EstablishedPeer>),
}

struct ClientHelloPending {
    machine: ClientHelloSent,
    retry: RetryState,
}

struct ClientFinishPending {
    machine: ClientFinishSent,
    retry: RetryState,
}

struct ServerHelloPending {
    machine: ServerHelloSent,
    request_hash: [u8; 32],
    retry: RetryState,
}

struct EstablishedPeer {
    session_id: [u8; 16],
    sender: DataSender,
    receiver: DataReceiver,
    sent_packets_in_epoch: u64,
    epoch_started_at: Instant,
    outbound_key_update: Option<PendingKeyUpdate>,
    previous_epoch_installed_at: Option<Instant>,
}

struct PendingKeyUpdate {
    next_epoch: u32,
    retry: RetryState,
}

struct PendingPathProbe {
    endpoint: SocketAddr,
    path_id: u32,
    token: [u8; 8],
    retry: RetryState,
}

struct RetryState {
    encoded: Vec<u8>,
    last_sent: Instant,
    attempts: u8,
    interval: Duration,
    max_attempts: u8,
}

impl RetryState {
    fn handshake(encoded: Vec<u8>, now: Instant) -> Self {
        Self::new(
            encoded,
            now,
            HANDSHAKE_RETRY_INTERVAL,
            HANDSHAKE_MAX_ATTEMPTS,
        )
    }

    fn key_update(encoded: Vec<u8>, now: Instant) -> Self {
        Self::new(
            encoded,
            now,
            KEY_UPDATE_RETRY_INTERVAL,
            KEY_UPDATE_MAX_ATTEMPTS,
        )
    }

    fn path_probe(encoded: Vec<u8>, now: Instant) -> Self {
        Self::new(
            encoded,
            now,
            PATH_PROBE_RETRY_INTERVAL,
            PATH_PROBE_MAX_ATTEMPTS,
        )
    }

    fn new(encoded: Vec<u8>, now: Instant, interval: Duration, max_attempts: u8) -> Self {
        Self {
            encoded,
            last_sent: now,
            attempts: 1,
            interval,
            max_attempts,
        }
    }

    fn poll(&mut self, now: Instant) -> RetryAction {
        if now.duration_since(self.last_sent) < self.interval {
            return RetryAction::Wait;
        }
        if self.attempts >= self.max_attempts {
            return RetryAction::Expired;
        }
        self.attempts = self.attempts.saturating_add(1);
        self.last_sent = now;
        RetryAction::Send(self.encoded.clone())
    }
}

enum RetryAction {
    Wait,
    Send(Vec<u8>),
    Expired,
}

struct ProcessResult {
    outbound: Vec<Vec<u8>>,
    plaintext: Option<Vec<u8>>,
    path_authenticated: bool,
}

impl ProcessResult {
    fn empty() -> Self {
        Self {
            outbound: Vec::new(),
            plaintext: None,
            path_authenticated: false,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct DataPlaneStatus {
    pub local_candidates: Vec<EndpointCandidate>,
    pub peers: HashMap<Ipv4Addr, PeerPathStatus>,
}

#[derive(Clone, Debug)]
pub struct PeerPathStatus {
    pub candidates: Vec<EndpointCandidate>,
    pub active_endpoint: Option<SocketAddr>,
    pub active_candidate_kind: Option<EndpointCandidateKind>,
    pub path_reason: Option<PathSelectionReason>,
    pub session_established: bool,
}

pub type SharedDataPlaneStatus = Arc<tokio::sync::RwLock<DataPlaneStatus>>;
type PeerDirectory = (
    SocketAddr,
    HashMap<Ipv4Addr, Peer>,
    HashMap<SocketAddr, Ipv4Addr>,
);

/// Owns the authenticated XSP/1 UDP sessions for one Agent.
pub struct UdpDataPlane {
    socket: UdpSocket,
    material: LocalMaterial,
    peers_by_virtual_ip: HashMap<Ipv4Addr, Peer>,
    peer_by_endpoint: HashMap<SocketAddr, Ipv4Addr>,
    candidate_manager: CandidateManager,
    status: SharedDataPlaneStatus,
    configuration_version: u64,
}

impl UdpDataPlane {
    /// Builds the signed peer directory and binds the local UDP endpoint.
    ///
    /// # Errors
    ///
    /// Returns an Agent error when trusted state, endpoints, credentials, or the UDP bind fail.
    pub async fn bind(state: &NodeState, identity: Arc<Identity>) -> Result<Self> {
        let material = LocalMaterial {
            network_id: *state.network_id.as_bytes(),
            node_id: decode_fixed::<16>(&state.node_id_base64)?,
            virtual_ip: state.virtual_ip,
            credential: decode_fixed::<CREDENTIAL_LENGTH>(&state.credential_base64)?,
            controller_verifying_key: VerifyingKey::from_bytes(&decode_fixed::<32>(
                &state.credential_signing_public_key_base64,
            )?)
            .map_err(|_| AgentError::ControllerTrust)?,
            identity: Arc::clone(&identity),
        };
        let (local_endpoint, peers_by_virtual_ip, peer_by_endpoint) =
            build_peer_directory(state, material.node_id)?;
        let socket = bind_data_socket(local_endpoint)?;
        let candidate_manager = CandidateManager::new(state, identity)?;
        let status = Arc::new(tokio::sync::RwLock::new(DataPlaneStatus::default()));
        let data_plane = Self {
            socket,
            material,
            peers_by_virtual_ip,
            peer_by_endpoint,
            candidate_manager,
            status,
            configuration_version: state.configuration.version,
        };
        data_plane.synchronize_status().await;
        Ok(data_plane)
    }

    #[must_use]
    pub fn status_handle(&self) -> SharedDataPlaneStatus {
        Arc::clone(&self.status)
    }

    #[must_use]
    pub const fn configuration_version(&self) -> u64 {
        self.configuration_version
    }

    /// Applies a newer trusted peer candidate directory while preserving established sessions.
    ///
    /// # Errors
    ///
    /// Returns an Agent error when the trusted configuration contains an unusable peer identity.
    pub async fn apply_configuration(&mut self, state: &NodeState) -> Result<()> {
        let (_, desired, _) = build_peer_directory(state, self.material.node_id)?;
        let mut updated = HashMap::with_capacity(desired.len());
        for (virtual_ip, mut replacement) in desired {
            if let Some(mut existing) = self.peers_by_virtual_ip.remove(&virtual_ip)
                && existing.node_id == replacement.node_id
            {
                existing.candidates = replacement.candidates;
                if !matches!(existing.state, PeerState::Established(_))
                    && existing
                        .active_endpoint
                        .is_none_or(|endpoint| !peer_has_endpoint(&existing, endpoint))
                {
                    existing.active_endpoint = existing
                        .candidates
                        .first()
                        .map(|candidate| candidate.endpoint);
                    existing.path_reason = Some(PathSelectionReason::ConfigurationUpdate);
                }
                replacement = existing;
            }
            updated.insert(virtual_ip, replacement);
        }
        self.peers_by_virtual_ip = updated;
        self.rebuild_endpoint_index()?;
        self.configuration_version = state.configuration.version;
        self.candidate_manager.force_refresh();
        self.synchronize_status().await;
        Ok(())
    }

    /// Returns the actual local UDP socket address.
    ///
    /// # Errors
    ///
    /// Returns an Agent error when the socket address cannot be queried.
    pub fn local_addr(&self) -> Result<SocketAddr> {
        self.socket.local_addr().map_err(|_| AgentError::Network)
    }

    /// Encrypts or queues one raw IPv4 packet read from TUN.
    ///
    /// The returned boolean is true when the packet was dropped.
    ///
    /// # Errors
    ///
    /// Returns an Agent error when local randomness, protocol state, or UDP transmission fails.
    pub async fn forward_tun(&mut self, packet: &[u8]) -> Result<bool> {
        if packet.len() > MAX_ENCRYPTED_PAYLOAD_LENGTH {
            return Ok(true);
        }
        let Some((source, destination)) = ipv4_endpoints(packet) else {
            return Ok(true);
        };
        if source != self.material.virtual_ip {
            return Ok(true);
        }
        let now = Instant::now();
        let Some(peer) = self.peers_by_virtual_ip.get_mut(&destination) else {
            return Ok(true);
        };

        let outbound = if let PeerState::Established(established) = &mut peer.state {
            let Ok(outbound) = established
                .sender
                .seal_ipv4(DataFlags::ACK_ELICITING, 0, packet)
            else {
                return Ok(true);
            };
            established.sent_packets_in_epoch = established.sent_packets_in_epoch.saturating_add(1);
            outbound
        } else {
            if !queue_packet(peer, packet) {
                return Ok(true);
            }
            if matches!(peer.state, PeerState::Idle) {
                begin_client_handshake(&self.material, peer, now)?
            } else {
                return Ok(false);
            }
        };
        let Some(endpoint) = peer.active_endpoint else {
            return Ok(true);
        };
        self.socket
            .send_to(&outbound, endpoint)
            .await
            .map_err(|_| AgentError::Network)?;
        Ok(false)
    }

    /// Receives and processes one UDP datagram, returning an authenticated IPv4 packet for TUN.
    ///
    /// # Errors
    ///
    /// Returns an Agent error only for local socket failures; untrusted protocol input is dropped.
    pub async fn receive(&mut self) -> Result<Option<Vec<u8>>> {
        let mut datagram = [0_u8; MAX_DATAGRAM_LENGTH];
        let (length, source) = self
            .socket
            .recv_from(&mut datagram)
            .await
            .map_err(|_| AgentError::Network)?;
        let source = normalize_endpoint(source);
        if self
            .candidate_manager
            .handles_discovery_response(&datagram[..length])
        {
            let _ = self
                .candidate_manager
                .handle_discovery_response(source, &datagram[..length]);
            return Ok(None);
        }
        let Some(peer_ip) = self.peer_by_endpoint.get(&source).copied() else {
            return Ok(None);
        };
        let Some(peer) = self.peers_by_virtual_ip.get_mut(&peer_ip) else {
            return Ok(None);
        };
        let handshake_packet = matches!(
            datagram.get(5).copied(),
            Some(CLIENT_HELLO_TYPE | SERVER_HELLO_TYPE | CLIENT_FINISH_TYPE | SERVER_FINISH_TYPE)
        );
        let result = process_datagram(
            &self.material,
            peer,
            source,
            &datagram[..length],
            Instant::now(),
        );
        let retain_fallback_reason = handshake_packet
            && peer.active_endpoint == Some(source)
            && peer.path_reason == Some(PathSelectionReason::HandshakeFallback);
        if result.path_authenticated
            && !retain_fallback_reason
            && (handshake_packet || peer.active_endpoint != Some(source))
        {
            promote_path(
                peer,
                source,
                if handshake_packet {
                    PathSelectionReason::AuthenticatedHandshake
                } else {
                    PathSelectionReason::AuthenticatedPeerTraffic
                },
            );
        }
        for outbound in result.outbound {
            self.socket
                .send_to(&outbound, source)
                .await
                .map_err(|_| AgentError::Network)?;
        }
        self.synchronize_status().await;
        Ok(result.plaintext)
    }

    /// Retransmits bounded pending handshakes and expires stale state.
    ///
    /// # Errors
    ///
    /// Returns an Agent error when a UDP retransmission fails.
    pub async fn maintain(&mut self, state: &NodeState) -> Result<Option<CandidateAdvertisement>> {
        let now = Instant::now();
        self.prune_expired_candidates()?;
        let mut retransmissions = Vec::new();
        for peer in self.peers_by_virtual_ip.values_mut() {
            prune_client_hello_cache(peer, now);
            match poll_peer_retry(peer, now) {
                RetryAction::Wait => {}
                RetryAction::Send(encoded) => {
                    if let Some(endpoint) = peer.active_endpoint {
                        retransmissions.push((endpoint, encoded));
                    }
                }
                RetryAction::Expired => {
                    if advance_handshake_candidate(peer) {
                        if let Some(endpoint) = peer.active_endpoint {
                            let encoded = begin_client_handshake(&self.material, peer, now)?;
                            retransmissions.push((endpoint, encoded));
                        }
                    } else {
                        clear_queue(peer);
                    }
                }
            }
            if let PeerState::Established(established) = &mut peer.state
                && let Some(encoded) = maintain_established(established, now)?
                && let Some(endpoint) = peer.active_endpoint
            {
                retransmissions.push((endpoint, encoded));
            }
            if let Some((endpoint, encoded)) = maintain_path_probe(peer, now)? {
                retransmissions.push((endpoint, encoded));
            }
        }
        for (endpoint, encoded) in retransmissions {
            self.socket
                .send_to(&encoded, endpoint)
                .await
                .map_err(|_| AgentError::Network)?;
        }
        if self.candidate_manager.refresh_due(now) {
            self.candidate_manager.refresh(&self.socket, state).await?;
        }
        let advertisement = self.candidate_manager.take_advertisement();
        self.synchronize_status().await;
        Ok(advertisement)
    }

    fn prune_expired_candidates(&mut self) -> Result<()> {
        let now = Utc::now();
        for peer in self.peers_by_virtual_ip.values_mut() {
            peer.candidates
                .retain(|candidate| candidate.expires_at > now);
            if !matches!(peer.state, PeerState::Established(_))
                && peer
                    .active_endpoint
                    .is_some_and(|endpoint| !peer_has_endpoint(peer, endpoint))
            {
                peer.active_endpoint = peer.candidates.first().map(|candidate| candidate.endpoint);
                peer.path_reason = Some(PathSelectionReason::ConfigurationUpdate);
            }
        }
        self.rebuild_endpoint_index()
    }

    fn rebuild_endpoint_index(&mut self) -> Result<()> {
        let mut endpoints = HashMap::new();
        for (virtual_ip, peer) in &self.peers_by_virtual_ip {
            let mut peer_endpoints = std::collections::HashSet::new();
            for endpoint in peer
                .candidates
                .iter()
                .map(|candidate| candidate.endpoint)
                .chain(peer.active_endpoint)
            {
                if !peer_endpoints.insert(endpoint) {
                    continue;
                }
                if endpoints.insert(endpoint, *virtual_ip).is_some() {
                    return Err(AgentError::DataPlane);
                }
            }
        }
        self.peer_by_endpoint = endpoints;
        Ok(())
    }

    async fn synchronize_status(&self) {
        let mut status = self.status.write().await;
        status.local_candidates = self.candidate_manager.candidates();
        status.peers = self
            .peers_by_virtual_ip
            .iter()
            .map(|(virtual_ip, peer)| {
                (
                    *virtual_ip,
                    PeerPathStatus {
                        candidates: peer.candidates.clone(),
                        active_endpoint: peer.active_endpoint,
                        active_candidate_kind: peer.active_endpoint.and_then(|endpoint| {
                            peer.candidates
                                .iter()
                                .find(|candidate| candidate.endpoint == endpoint)
                                .map(|candidate| candidate.kind)
                        }),
                        path_reason: peer.path_reason,
                        session_established: matches!(peer.state, PeerState::Established(_)),
                    },
                )
            })
            .collect();
    }
}

fn build_peer_directory(state: &NodeState, local_node_id: [u8; 16]) -> Result<PeerDirectory> {
    let mut local_endpoint = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0));
    let mut peers = HashMap::new();
    let mut endpoints = HashMap::new();
    for node in &state.configuration_payload.nodes {
        let node_id = decode_fixed::<16>(&node.node_id_base64)?;
        let virtual_ip = node
            .virtual_ip
            .parse::<Ipv4Addr>()
            .map_err(|_| AgentError::ControllerTrust)?;
        let direct_endpoints = node
            .direct_endpoints
            .iter()
            .map(|encoded| {
                encoded
                    .parse::<SocketAddrV4>()
                    .map(SocketAddr::V4)
                    .map_err(|_| AgentError::ControllerTrust)
            })
            .collect::<Result<Vec<_>>>()?;
        if node_id == local_node_id {
            if let Some(endpoint) = direct_endpoints.first() {
                local_endpoint = *endpoint;
            }
            continue;
        }
        if peers.len() >= MAX_CONFIGURED_PEERS || peers.contains_key(&virtual_ip) {
            return Err(AgentError::DataPlane);
        }
        let candidates = configured_candidates(node, direct_endpoints);
        for candidate in &candidates {
            if endpoints.insert(candidate.endpoint, virtual_ip).is_some() {
                return Err(AgentError::DataPlane);
            }
        }
        let active_endpoint = candidates.first().map(|candidate| candidate.endpoint);
        peers.insert(
            virtual_ip,
            Peer {
                node_id,
                virtual_ip,
                candidates,
                active_endpoint,
                path_reason: active_endpoint.map(|_| PathSelectionReason::HighestPriority),
                state: PeerState::Idle,
                queued_packets: VecDeque::new(),
                queued_bytes: 0,
                recent_client_hellos: VecDeque::new(),
                pending_path_probe: None,
                path_probe_retry_after: Instant::now(),
            },
        );
    }
    Ok((local_endpoint, peers, endpoints))
}

fn configured_candidates(
    node: &xs_core::ConfigurationNode,
    direct_endpoints: Vec<SocketAddr>,
) -> Vec<EndpointCandidate> {
    let now = Utc::now();
    let mut seen = std::collections::HashSet::new();
    let mut candidates = node
        .candidates
        .iter()
        .filter(|candidate| {
            candidate.expires_at > now && candidate.kind != EndpointCandidateKind::Relay
        })
        .cloned()
        .map(|mut candidate| {
            candidate.endpoint = normalize_endpoint(candidate.endpoint);
            candidate
        })
        .filter(|candidate| seen.insert(candidate.endpoint))
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        candidates.extend(
            direct_endpoints
                .into_iter()
                .map(normalize_endpoint)
                .filter(|endpoint| seen.insert(*endpoint))
                .enumerate()
                .map(|(index, endpoint)| EndpointCandidate {
                    kind: EndpointCandidateKind::Static,
                    endpoint,
                    priority: 10_000_u32.saturating_sub(u32::try_from(index).unwrap_or(u32::MAX)),
                    expires_at: node.credential_not_after,
                }),
        );
    }
    candidates.sort_by(|left, right| right.priority.cmp(&left.priority));
    candidates
}

fn bind_data_socket(endpoint: SocketAddr) -> Result<UdpSocket> {
    let (domain, bind_endpoint, dual_stack) = match endpoint {
        SocketAddr::V4(endpoint) if endpoint.ip().is_unspecified() => (
            Domain::IPV6,
            SocketAddr::V6(SocketAddrV6::new(
                Ipv6Addr::UNSPECIFIED,
                endpoint.port(),
                0,
                0,
            )),
            true,
        ),
        SocketAddr::V4(_) => (Domain::IPV4, endpoint, false),
        SocketAddr::V6(_) => (Domain::IPV6, endpoint, true),
    };
    let socket =
        Socket::new(domain, Type::DGRAM, Some(Protocol::UDP)).map_err(|_| AgentError::Network)?;
    if dual_stack {
        socket.set_only_v6(false).map_err(|_| AgentError::Network)?;
    }
    socket
        .set_nonblocking(true)
        .map_err(|_| AgentError::Network)?;
    socket
        .bind(&bind_endpoint.into())
        .map_err(|_| AgentError::Network)?;
    let socket: std::net::UdpSocket = socket.into();
    UdpSocket::from_std(socket).map_err(|_| AgentError::Network)
}

fn peer_has_endpoint(peer: &Peer, endpoint: SocketAddr) -> bool {
    peer.candidates
        .iter()
        .any(|candidate| candidate.endpoint == endpoint)
}

fn advance_handshake_candidate(peer: &mut Peer) -> bool {
    let next = peer.active_endpoint.map_or(0, |active| {
        peer.candidates
            .iter()
            .position(|candidate| candidate.endpoint == active)
            .map_or(0, |index| index.saturating_add(1))
    });
    let Some(candidate) = peer.candidates.get(next) else {
        return false;
    };
    peer.active_endpoint = Some(candidate.endpoint);
    peer.path_reason = Some(PathSelectionReason::HandshakeFallback);
    peer.state = PeerState::Idle;
    peer.pending_path_probe = None;
    true
}

fn promote_path(peer: &mut Peer, endpoint: SocketAddr, reason: PathSelectionReason) {
    peer.active_endpoint = Some(endpoint);
    peer.path_reason = Some(reason);
    peer.pending_path_probe = None;
}

fn maintain_path_probe(peer: &mut Peer, now: Instant) -> Result<Option<(SocketAddr, Vec<u8>)>> {
    if let Some(pending) = &mut peer.pending_path_probe {
        return match pending.retry.poll(now) {
            RetryAction::Wait => Ok(None),
            RetryAction::Send(encoded) => Ok(Some((pending.endpoint, encoded))),
            RetryAction::Expired => {
                peer.pending_path_probe = None;
                peer.path_probe_retry_after = now + PATH_PROBE_COOLDOWN;
                Ok(None)
            }
        };
    }
    if now < peer.path_probe_retry_after {
        return Ok(None);
    }
    let target = best_probe_target(peer);
    let Some(endpoint) = target else {
        return Ok(None);
    };
    let PeerState::Established(established) = &mut peer.state else {
        return Ok(None);
    };
    let token = random_array::<8>()?;
    let mut path_id = u32::from_be_bytes(random_array()?);
    if path_id == 0 {
        path_id = 1;
    }
    let encoded = established
        .sender
        .seal_control(
            PacketType::PathChallenge,
            DataFlags::PATH_PROBE,
            path_id,
            &token,
        )
        .map_err(|_| AgentError::DataPlane)?;
    peer.pending_path_probe = Some(PendingPathProbe {
        endpoint,
        path_id,
        token,
        retry: RetryState::path_probe(encoded.clone(), now),
    });
    Ok(Some((endpoint, encoded)))
}

fn best_probe_target(peer: &Peer) -> Option<SocketAddr> {
    let active_priority = peer.active_endpoint.and_then(|active| {
        peer.candidates
            .iter()
            .find(|candidate| candidate.endpoint == active)
            .map(|candidate| candidate.priority)
    });
    peer.candidates
        .iter()
        .find(|candidate| {
            Some(candidate.endpoint) != peer.active_endpoint
                && active_priority.is_none_or(|priority| candidate.priority > priority)
        })
        .map(|candidate| candidate.endpoint)
}

fn queue_packet(peer: &mut Peer, packet: &[u8]) -> bool {
    if peer.queued_packets.len() >= MAX_QUEUED_PACKETS_PER_PEER
        || peer.queued_bytes.saturating_add(packet.len()) > MAX_QUEUED_BYTES_PER_PEER
    {
        return false;
    }
    peer.queued_bytes = peer.queued_bytes.saturating_add(packet.len());
    peer.queued_packets.push_back(packet.to_vec());
    true
}

fn begin_client_handshake(
    material: &LocalMaterial,
    peer: &mut Peer,
    now: Instant,
) -> Result<Vec<u8>> {
    let machine = ClientHelloSent::start(
        ClientHandshakeParameters {
            context: context(material, peer),
            credential: material.credential,
            ephemeral_private_key: EphemeralPrivateKey::from_bytes(random_array()?),
            client_nonce: random_array()?,
            message_id: u32::from_be_bytes(random_array()?),
            client_time: unix_time()?,
        },
        material.identity.signing_key(),
    )
    .map_err(|_| AgentError::DataPlane)?;
    let encoded = machine.encoded().to_vec();
    peer.state = PeerState::ClientHello(Box::new(ClientHelloPending {
        machine,
        retry: RetryState::handshake(encoded.clone(), now),
    }));
    Ok(encoded)
}

fn process_datagram(
    material: &LocalMaterial,
    peer: &mut Peer,
    source: SocketAddr,
    datagram: &[u8],
    now: Instant,
) -> ProcessResult {
    match datagram.get(5).copied() {
        Some(CLIENT_HELLO_TYPE) => handle_client_hello(material, peer, datagram, now),
        Some(SERVER_HELLO_TYPE) => handle_server_hello(material, peer, datagram, now),
        Some(CLIENT_FINISH_TYPE) => handle_client_finish(peer, datagram),
        Some(SERVER_FINISH_TYPE) => handle_server_finish(peer, datagram),
        Some(value) if PacketType::try_from(value).is_ok() => handle_data(peer, source, datagram),
        _ => ProcessResult::empty(),
    }
}

fn handle_client_hello(
    material: &LocalMaterial,
    peer: &mut Peer,
    datagram: &[u8],
    now: Instant,
) -> ProcessResult {
    let request_hash: [u8; 32] = Sha256::digest(datagram).into();
    if let PeerState::ServerHello(pending) = &peer.state
        && pending.request_hash == request_hash
    {
        return ProcessResult {
            outbound: vec![pending.retry.encoded.clone()],
            plaintext: None,
            path_authenticated: true,
        };
    }
    prune_client_hello_cache(peer, now);
    if peer
        .recent_client_hellos
        .iter()
        .any(|(known, _)| *known == request_hash)
    {
        return ProcessResult::empty();
    }
    if matches!(
        peer.state,
        PeerState::ClientHello(_) | PeerState::ClientFinish(_)
    ) && material.node_id < peer.node_id
    {
        return ProcessResult::empty();
    }
    let parameters = ServerHandshakeParameters {
        context: context(material, peer),
        credential: material.credential,
        ephemeral_private_key: match random_array() {
            Ok(value) => EphemeralPrivateKey::from_bytes(value),
            Err(_) => return ProcessResult::empty(),
        },
        server_nonce: match random_array() {
            Ok(value) => value,
            Err(_) => return ProcessResult::empty(),
        },
        session_id: match random_array() {
            Ok(value) => value,
            Err(_) => return ProcessResult::empty(),
        },
        server_time: match unix_time() {
            Ok(value) => value,
            Err(_) => return ProcessResult::empty(),
        },
    };
    let Ok(machine) = ServerHelloSent::accept(
        datagram,
        parameters,
        material.identity.signing_key(),
        &material.controller_verifying_key,
        match unix_time() {
            Ok(value) => value,
            Err(_) => return ProcessResult::empty(),
        },
    ) else {
        return ProcessResult::empty();
    };
    let encoded = machine.encoded().to_vec();
    remember_client_hello(peer, request_hash, now);
    peer.state = PeerState::ServerHello(Box::new(ServerHelloPending {
        machine,
        request_hash,
        retry: RetryState::handshake(encoded.clone(), now),
    }));
    ProcessResult {
        outbound: vec![encoded],
        plaintext: None,
        path_authenticated: true,
    }
}

fn handle_server_hello(
    material: &LocalMaterial,
    peer: &mut Peer,
    datagram: &[u8],
    now: Instant,
) -> ProcessResult {
    if let PeerState::ClientFinish(pending) = &peer.state {
        return ProcessResult {
            outbound: vec![pending.retry.encoded.clone()],
            plaintext: None,
            path_authenticated: true,
        };
    }
    let state = std::mem::replace(&mut peer.state, PeerState::Idle);
    let PeerState::ClientHello(pending) = state else {
        peer.state = state;
        return ProcessResult::empty();
    };
    let Ok(machine) = pending.machine.accept_server_hello(
        datagram,
        &material.controller_verifying_key,
        match unix_time() {
            Ok(value) => value,
            Err(_) => return ProcessResult::empty(),
        },
    ) else {
        clear_queue(peer);
        return ProcessResult::empty();
    };
    let encoded = machine.encoded().to_vec();
    peer.state = PeerState::ClientFinish(Box::new(ClientFinishPending {
        machine,
        retry: RetryState::handshake(encoded.clone(), now),
    }));
    ProcessResult {
        outbound: vec![encoded],
        plaintext: None,
        path_authenticated: true,
    }
}

fn handle_client_finish(peer: &mut Peer, datagram: &[u8]) -> ProcessResult {
    let state = std::mem::replace(&mut peer.state, PeerState::Idle);
    let PeerState::ServerHello(pending) = state else {
        peer.state = state;
        return ProcessResult::empty();
    };
    let Ok((session, server_finish)) = pending.machine.accept_client_finish(datagram) else {
        clear_queue(peer);
        return ProcessResult::empty();
    };
    let mut outbound = vec![server_finish];
    outbound.extend(install_session(peer, session));
    ProcessResult {
        outbound,
        plaintext: None,
        path_authenticated: true,
    }
}

fn handle_server_finish(peer: &mut Peer, datagram: &[u8]) -> ProcessResult {
    let state = std::mem::replace(&mut peer.state, PeerState::Idle);
    let PeerState::ClientFinish(pending) = state else {
        peer.state = state;
        return ProcessResult::empty();
    };
    let Ok(session) = pending.machine.accept_server_finish(datagram) else {
        clear_queue(peer);
        return ProcessResult::empty();
    };
    ProcessResult {
        outbound: install_session(peer, session),
        plaintext: None,
        path_authenticated: true,
    }
}

fn handle_data(peer: &mut Peer, source: SocketAddr, datagram: &[u8]) -> ProcessResult {
    let mut path_probe_succeeded = false;
    let result = {
        let PeerState::Established(established) = &mut peer.state else {
            return ProcessResult::empty();
        };
        let Ok(opened) = established.receiver.open(datagram) else {
            return ProcessResult::empty();
        };
        match opened.packet_type {
            PacketType::Data => ProcessResult {
                outbound: Vec::new(),
                plaintext: Some(opened.plaintext),
                path_authenticated: true,
            },
            PacketType::KeyUpdate => ProcessResult {
                outbound: handle_key_update(established, &opened)
                    .into_iter()
                    .collect(),
                plaintext: None,
                path_authenticated: true,
            },
            PacketType::KeyUpdateAck => {
                handle_key_update_ack(established, &opened);
                ProcessResult {
                    outbound: Vec::new(),
                    plaintext: None,
                    path_authenticated: true,
                }
            }
            PacketType::Keepalive => ProcessResult {
                outbound: Vec::new(),
                plaintext: None,
                path_authenticated: true,
            },
            PacketType::PathChallenge => ProcessResult {
                outbound: established
                    .sender
                    .seal_control(
                        PacketType::PathResponse,
                        DataFlags::PATH_PROBE,
                        opened.path_id,
                        &opened.plaintext,
                    )
                    .into_iter()
                    .collect(),
                plaintext: None,
                path_authenticated: false,
            },
            PacketType::PathResponse => {
                path_probe_succeeded = peer.pending_path_probe.as_ref().is_some_and(|pending| {
                    pending.endpoint == source
                        && pending.path_id == opened.path_id
                        && pending.token.as_slice() == opened.plaintext
                });
                ProcessResult::empty()
            }
            PacketType::Close => ProcessResult::empty(),
        }
    };
    if path_probe_succeeded {
        peer.pending_path_probe = None;
        promote_path(peer, source, PathSelectionReason::AuthenticatedPathProbe);
    }
    result
}

fn install_session(peer: &mut Peer, session: EstablishedSession) -> Vec<Vec<u8>> {
    let session_id = session.session_id();
    let Ok((mut sender, receiver)) = session.into_data_plane() else {
        clear_queue(peer);
        peer.state = PeerState::Idle;
        return Vec::new();
    };
    let mut outbound = Vec::with_capacity(peer.queued_packets.len());
    while let Some(packet) = peer.queued_packets.pop_front() {
        if let Ok(encoded) = sender.seal_ipv4(DataFlags::ACK_ELICITING, 0, &packet) {
            outbound.push(encoded);
        }
    }
    peer.queued_bytes = 0;
    peer.state = PeerState::Established(Box::new(EstablishedPeer {
        session_id,
        sender,
        receiver,
        sent_packets_in_epoch: 0,
        epoch_started_at: Instant::now(),
        outbound_key_update: None,
        previous_epoch_installed_at: None,
    }));
    outbound
}

fn handle_key_update(
    established: &mut EstablishedPeer,
    opened: &xs_protocol::OpenedPacket,
) -> Option<Vec<u8>> {
    let next_epoch =
        verify_key_update_payload(&opened.plaintext, established.session_id, opened.key_epoch)
            .ok()?;
    let current_epoch = established.receiver.current_epoch();
    if current_epoch.checked_add(1) == Some(next_epoch) {
        established.receiver.install_next_epoch(next_epoch).ok()?;
        established.previous_epoch_installed_at = Some(Instant::now());
    } else if current_epoch != next_epoch || opened.key_epoch.checked_add(1) != Some(current_epoch)
    {
        return None;
    }
    established
        .sender
        .seal_control(
            PacketType::KeyUpdateAck,
            DataFlags::CONTROL,
            0,
            &opened.plaintext,
        )
        .ok()
}

fn handle_key_update_ack(established: &mut EstablishedPeer, opened: &xs_protocol::OpenedPacket) {
    let Some(pending) = established.outbound_key_update.as_ref() else {
        return;
    };
    let current_epoch = established.sender.current_epoch();
    let Ok(next_epoch) =
        verify_key_update_payload(&opened.plaintext, established.session_id, current_epoch)
    else {
        return;
    };
    if next_epoch != pending.next_epoch || established.sender.rotate_epoch(next_epoch).is_err() {
        return;
    }
    established.sent_packets_in_epoch = 0;
    established.epoch_started_at = Instant::now();
    established.outbound_key_update = None;
}

fn maintain_established(
    established: &mut EstablishedPeer,
    now: Instant,
) -> Result<Option<Vec<u8>>> {
    if established
        .previous_epoch_installed_at
        .is_some_and(|installed| now.duration_since(installed) >= PREVIOUS_EPOCH_RETENTION)
    {
        established.receiver.retire_previous_epoch();
        established.previous_epoch_installed_at = None;
    }

    if let Some(pending) = &mut established.outbound_key_update {
        return match pending.retry.poll(now) {
            RetryAction::Wait => Ok(None),
            RetryAction::Send(encoded) => Ok(Some(encoded)),
            RetryAction::Expired => {
                established.outbound_key_update = None;
                Ok(None)
            }
        };
    }
    if established.sent_packets_in_epoch < KEY_UPDATE_PACKET_LIMIT
        && now.duration_since(established.epoch_started_at) < KEY_UPDATE_INTERVAL
    {
        return Ok(None);
    }

    let current_epoch = established.sender.current_epoch();
    let next_epoch = current_epoch.checked_add(1).ok_or(AgentError::DataPlane)?;
    let payload = key_update_payload(established.session_id, current_epoch, next_epoch)
        .map_err(|_| AgentError::DataPlane)?;
    let encoded = established
        .sender
        .seal_control(PacketType::KeyUpdate, DataFlags::CONTROL, 0, &payload)
        .map_err(|_| AgentError::DataPlane)?;
    established.outbound_key_update = Some(PendingKeyUpdate {
        next_epoch,
        retry: RetryState::key_update(encoded.clone(), now),
    });
    Ok(Some(encoded))
}

fn poll_peer_retry(peer: &mut Peer, now: Instant) -> RetryAction {
    let state = std::mem::replace(&mut peer.state, PeerState::Idle);
    let (state, action, client_initiated) = match state {
        PeerState::ClientHello(mut pending) => {
            let action = pending.retry.poll(now);
            (PeerState::ClientHello(pending), action, true)
        }
        PeerState::ClientFinish(mut pending) => {
            let action = pending.retry.poll(now);
            (PeerState::ClientFinish(pending), action, true)
        }
        PeerState::ServerHello(mut pending) => {
            let action = pending.retry.poll(now);
            (PeerState::ServerHello(pending), action, false)
        }
        other => {
            peer.state = other;
            return RetryAction::Wait;
        }
    };
    match action {
        RetryAction::Wait => {
            peer.state = state;
            RetryAction::Wait
        }
        RetryAction::Send(encoded) => {
            peer.state = state;
            RetryAction::Send(encoded)
        }
        RetryAction::Expired if client_initiated => RetryAction::Expired,
        RetryAction::Expired => {
            peer.state = PeerState::Idle;
            RetryAction::Wait
        }
    }
}

fn remember_client_hello(peer: &mut Peer, request_hash: [u8; 32], now: Instant) {
    if peer.recent_client_hellos.len() >= MAX_RECENT_CLIENT_HELLOS {
        peer.recent_client_hellos.pop_front();
    }
    peer.recent_client_hellos.push_back((request_hash, now));
}

fn prune_client_hello_cache(peer: &mut Peer, now: Instant) {
    while peer
        .recent_client_hellos
        .front()
        .is_some_and(|(_, seen)| now.duration_since(*seen) >= CLIENT_HELLO_CACHE_LIFETIME)
    {
        peer.recent_client_hellos.pop_front();
    }
}

fn clear_queue(peer: &mut Peer) {
    peer.queued_packets.clear();
    peer.queued_bytes = 0;
    peer.state = PeerState::Idle;
}

fn context(material: &LocalMaterial, peer: &Peer) -> HandshakeContext {
    HandshakeContext {
        network_id: material.network_id,
        local_node_id: material.node_id,
        local_virtual_ip: material.virtual_ip,
        peer_node_id: peer.node_id,
        peer_virtual_ip: peer.virtual_ip,
    }
}

fn ipv4_endpoints(packet: &[u8]) -> Option<(Ipv4Addr, Ipv4Addr)> {
    if packet.len() < 20 || packet[0] >> 4 != 4 {
        return None;
    }
    let header_length = usize::from(packet[0] & 0x0f).checked_mul(4)?;
    if header_length < 20 || packet.len() < header_length {
        return None;
    }
    Some((
        Ipv4Addr::new(packet[12], packet[13], packet[14], packet[15]),
        Ipv4Addr::new(packet[16], packet[17], packet[18], packet[19]),
    ))
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
    fn ipv4_endpoints_require_a_complete_ipv4_header() {
        let mut packet = [0_u8; 20];
        packet[0] = 0x45;
        packet[12..16].copy_from_slice(&[100, 88, 0, 1]);
        packet[16..20].copy_from_slice(&[100, 88, 0, 2]);
        assert_eq!(
            ipv4_endpoints(&packet),
            Some((Ipv4Addr::new(100, 88, 0, 1), Ipv4Addr::new(100, 88, 0, 2)))
        );
        packet[0] = 0x65;
        assert_eq!(ipv4_endpoints(&packet), None);
        assert_eq!(ipv4_endpoints(&packet[..19]), None);
    }

    #[test]
    fn retry_state_is_bounded() {
        let start = Instant::now();
        let mut retry = RetryState::handshake(vec![1, 2, 3], start);
        assert!(matches!(retry.poll(start), RetryAction::Wait));
        for attempt in 2..=HANDSHAKE_MAX_ATTEMPTS {
            let now = start + HANDSHAKE_RETRY_INTERVAL * u32::from(attempt - 1);
            assert!(matches!(retry.poll(now), RetryAction::Send(_)));
        }
        let expired = start + HANDSHAKE_RETRY_INTERVAL * u32::from(HANDSHAKE_MAX_ATTEMPTS);
        assert!(matches!(retry.poll(expired), RetryAction::Expired));
    }
}
