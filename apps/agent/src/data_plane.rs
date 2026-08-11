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
    AclPolicy, AclProtocol, CandidateAdvertisement, EndpointCandidate, EndpointCandidateKind,
    PathSelectionReason, SubnetRoutePolicy,
};
use xs_protocol::{
    CLIENT_FINISH_TYPE, CLIENT_HELLO_TYPE, CREDENTIAL_LENGTH, ClientFinishSent,
    ClientHandshakeParameters, ClientHelloSent, DataFlags, DataHeader, DataReceiver, DataSender,
    EphemeralPrivateKey, EstablishedSession, HandshakeContext, MAX_DATAGRAM_LENGTH,
    MAX_ENCRYPTED_PAYLOAD_LENGTH, PacketType, SERVER_FINISH_TYPE, SERVER_HELLO_TYPE,
    ServerHandshakeParameters, ServerHelloSent, key_update_payload, verify_key_update_payload,
};

use crate::{
    candidates::{CandidateManager, normalize_endpoint},
    error::{AgentError, Result},
    relay::{RelayInbound, RelayManager},
    state::{NodeState, decode_fixed},
    storage::Identity,
};

const MAX_CONFIGURED_PEERS: usize = 1024;
const MAX_QUEUED_PACKETS_PER_PEER: usize = 32;
const MAX_QUEUED_BYTES_PER_PEER: usize = 64 * 1024;
const MAX_RECENT_CLIENT_HELLOS: usize = 64;
const CLIENT_HELLO_CACHE_LIFETIME: Duration = Duration::from_mins(5);
const HANDSHAKE_RETRY_INTERVAL: Duration = Duration::from_millis(300);
const HANDSHAKE_MAX_ATTEMPTS: u8 = 6;
const SERVER_FINISH_CACHE_LIFETIME: Duration = Duration::from_secs(5);
const MAX_CONCURRENT_PROACTIVE_HANDSHAKES: usize = 32;
const MAX_PROACTIVE_HANDSHAKES_PER_TICK: usize = 8;
const MAX_HANDSHAKE_CANDIDATES_PER_CYCLE: usize = 8;
const MAX_RELAY_CANDIDATES_PER_PEER: usize = 2;
#[cfg(not(feature = "privileged-network-tests"))]
const HANDSHAKE_BACKOFF_BASE: Duration = Duration::from_secs(1);
#[cfg(feature = "privileged-network-tests")]
const HANDSHAKE_BACKOFF_BASE: Duration = Duration::from_millis(200);
#[cfg(not(feature = "privileged-network-tests"))]
const HANDSHAKE_BACKOFF_MAX: Duration = Duration::from_mins(1);
#[cfg(feature = "privileged-network-tests")]
const HANDSHAKE_BACKOFF_MAX: Duration = Duration::from_secs(2);
const KEY_UPDATE_RETRY_INTERVAL: Duration = Duration::from_millis(300);
const KEY_UPDATE_MAX_ATTEMPTS: u8 = 6;
const PATH_PROBE_RETRY_INTERVAL: Duration = Duration::from_millis(300);
const PATH_PROBE_MAX_ATTEMPTS: u8 = 4;
const MANUAL_PATH_PROBE_COOLDOWN: Duration = Duration::from_secs(1);
#[cfg(not(feature = "privileged-network-tests"))]
const PATH_PROBE_COOLDOWN: Duration = Duration::from_secs(30);
#[cfg(feature = "privileged-network-tests")]
const PATH_PROBE_COOLDOWN: Duration = Duration::from_secs(3);
#[cfg(not(feature = "privileged-network-tests"))]
const LATENCY_PROBE_INTERVAL: Duration = Duration::from_secs(30);
#[cfg(feature = "privileged-network-tests")]
const LATENCY_PROBE_INTERVAL: Duration = Duration::from_secs(3);
#[cfg(not(feature = "privileged-network-tests"))]
const KEY_UPDATE_PACKET_LIMIT: u64 = 1 << 20;
#[cfg(feature = "privileged-network-tests")]
const KEY_UPDATE_PACKET_LIMIT: u64 = 4;
#[cfg(not(feature = "privileged-network-tests"))]
const KEY_UPDATE_INTERVAL: Duration = Duration::from_hours(1);
#[cfg(feature = "privileged-network-tests")]
const KEY_UPDATE_INTERVAL: Duration = Duration::from_secs(2);
#[cfg(not(feature = "privileged-network-tests"))]
const PREVIOUS_EPOCH_RETENTION: Duration = Duration::from_secs(30);
#[cfg(feature = "privileged-network-tests")]
const PREVIOUS_EPOCH_RETENTION: Duration = Duration::from_secs(5);
#[cfg(not(feature = "privileged-network-tests"))]
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(15);
#[cfg(feature = "privileged-network-tests")]
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(1);

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
    node_id_base64: String,
    virtual_ip: Ipv4Addr,
    candidates: Vec<EndpointCandidate>,
    active_endpoint: Option<SocketAddr>,
    path_reason: Option<PathSelectionReason>,
    state: PeerState,
    queued_packets: VecDeque<QueuedPacket>,
    queued_bytes: usize,
    recent_client_hellos: VecDeque<([u8; 32], Instant)>,
    cached_server_finish: Option<CachedServerFinish>,
    pending_path_probe: Option<PendingPathProbe>,
    path_probe_retry_after: Instant,
    next_latency_probe_at: Instant,
    next_proactive_handshake_at: Instant,
    handshake_failures: u8,
    handshake_candidate_attempts: usize,
    tx_packets_total: u64,
    tx_bytes_total: u64,
    rx_packets_total: u64,
    rx_bytes_total: u64,
    handshake_attempts_total: u64,
    handshake_successes_total: u64,
    replay_drops_total: u64,
    latency_samples_total: u64,
    latency_microseconds_total: u64,
    last_latency_microseconds: Option<u64>,
}

#[derive(Clone, Copy, Debug, Default)]
struct TelemetryCounters {
    tx_bytes: u64,
    rx_bytes: u64,
    handshake_attempts: u64,
    handshake_successes: u64,
    acl_drops: u64,
    replay_drops: u64,
    latency_samples: u64,
    latency_microseconds: u64,
}

impl TelemetryCounters {
    fn add_peer(&mut self, peer: &Peer) {
        self.tx_bytes = self.tx_bytes.saturating_add(peer.tx_bytes_total);
        self.rx_bytes = self.rx_bytes.saturating_add(peer.rx_bytes_total);
        self.handshake_attempts = self
            .handshake_attempts
            .saturating_add(peer.handshake_attempts_total);
        self.handshake_successes = self
            .handshake_successes
            .saturating_add(peer.handshake_successes_total);
        self.replay_drops = self.replay_drops.saturating_add(peer.replay_drops_total);
        self.latency_samples = self
            .latency_samples
            .saturating_add(peer.latency_samples_total);
        self.latency_microseconds = self
            .latency_microseconds
            .saturating_add(peer.latency_microseconds_total);
    }
}

struct QueuedPacket {
    encoded: Vec<u8>,
    routed: bool,
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
    last_keepalive_at: Instant,
}

struct PendingKeyUpdate {
    next_epoch: u32,
    payload: [u8; 36],
    retry: RetrySchedule,
}

struct PendingPathProbe {
    endpoint: SocketAddr,
    path_id: u32,
    token: [u8; 8],
    promotes_path: bool,
    retry: RetrySchedule,
}

struct CachedServerFinish {
    client_finish_hash: [u8; 32],
    encoded: Vec<u8>,
    expires_at: Instant,
}

struct RetryState {
    encoded: Vec<u8>,
    schedule: RetrySchedule,
}

struct RetrySchedule {
    last_sent: Instant,
    attempts: u8,
    interval: Duration,
    max_attempts: u8,
}

impl RetryState {
    fn handshake(encoded: Vec<u8>, now: Instant) -> Self {
        Self {
            encoded,
            schedule: RetrySchedule::new(now, HANDSHAKE_RETRY_INTERVAL, HANDSHAKE_MAX_ATTEMPTS),
        }
    }

    fn poll(&mut self, now: Instant) -> RetryAction {
        match self.schedule.poll(now) {
            RetryDecision::Wait => RetryAction::Wait,
            RetryDecision::Retry => RetryAction::Send(self.encoded.clone()),
            RetryDecision::Expired => RetryAction::Expired,
        }
    }
}

impl RetrySchedule {
    fn key_update(now: Instant) -> Self {
        Self::new(now, KEY_UPDATE_RETRY_INTERVAL, KEY_UPDATE_MAX_ATTEMPTS)
    }

    fn path_probe(now: Instant) -> Self {
        Self::new(now, PATH_PROBE_RETRY_INTERVAL, PATH_PROBE_MAX_ATTEMPTS)
    }

    fn new(now: Instant, interval: Duration, max_attempts: u8) -> Self {
        Self {
            last_sent: now,
            attempts: 1,
            interval,
            max_attempts,
        }
    }

    fn poll(&mut self, now: Instant) -> RetryDecision {
        if now.duration_since(self.last_sent) < self.interval {
            return RetryDecision::Wait;
        }
        if self.attempts >= self.max_attempts {
            return RetryDecision::Expired;
        }
        self.attempts = self.attempts.saturating_add(1);
        self.last_sent = now;
        RetryDecision::Retry
    }
}

enum RetryAction {
    Wait,
    Send(Vec<u8>),
    Expired,
}

enum RetryDecision {
    Wait,
    Retry,
    Expired,
}

enum EstablishedMaintenance {
    Wait,
    Send(Vec<u8>),
    Rehandshake,
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
    pub tx_bytes_total: u64,
    pub rx_bytes_total: u64,
    pub handshake_attempts_total: u64,
    pub handshake_successes_total: u64,
    pub acl_drops_total: u64,
    pub replay_drops_total: u64,
    pub latency_samples_total: u64,
    pub latency_microseconds_total: u64,
}

#[derive(Clone, Debug)]
pub struct PeerPathStatus {
    pub candidates: Vec<EndpointCandidate>,
    pub active_endpoint: Option<SocketAddr>,
    pub active_candidate_kind: Option<EndpointCandidateKind>,
    pub path_reason: Option<PathSelectionReason>,
    pub session_established: bool,
    pub tx_packets_total: u64,
    pub tx_bytes_total: u64,
    pub rx_packets_total: u64,
    pub rx_bytes_total: u64,
    pub handshake_attempts_total: u64,
    pub handshake_successes_total: u64,
    pub latency_samples_total: u64,
    pub latency_microseconds_total: u64,
    pub last_latency_microseconds: Option<u64>,
}

pub type SharedDataPlaneStatus = Arc<tokio::sync::RwLock<DataPlaneStatus>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManualProbeError {
    PeerNotFound,
    SessionUnavailable,
    Busy,
    SendFailed,
}

type PeerDirectory = (
    SocketAddr,
    HashMap<Ipv4Addr, Peer>,
    HashMap<SocketAddr, Ipv4Addr>,
    HashMap<[u8; 16], Ipv4Addr>,
);

/// Owns the authenticated XSP/1 UDP sessions for one Agent.
pub struct UdpDataPlane {
    socket: UdpSocket,
    material: LocalMaterial,
    peers_by_virtual_ip: HashMap<Ipv4Addr, Peer>,
    peer_by_endpoint: HashMap<SocketAddr, Ipv4Addr>,
    peer_by_node_id: HashMap<[u8; 16], Ipv4Addr>,
    candidate_manager: CandidateManager,
    relay_manager: RelayManager,
    acl_policy: AclPolicy,
    subnet_routes: SubnetRoutePolicy,
    local_node_id_base64: String,
    status: SharedDataPlaneStatus,
    retired_telemetry: TelemetryCounters,
    configuration_version: u64,
    policy_version: u64,
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
        let relay_manager = RelayManager::new(
            state,
            material.network_id,
            material.node_id,
            material.credential,
            Arc::clone(&identity),
        )?;
        let relay_candidates = relay_manager.candidates();
        let (local_endpoint, peers_by_virtual_ip, peer_by_endpoint, peer_by_node_id) =
            build_peer_directory(state, material.node_id, &relay_candidates)?;
        let socket = bind_data_socket(local_endpoint)?;
        let candidate_manager = CandidateManager::new(state, identity)?;
        let acl_policy = AclPolicy::compile(&state.configuration_payload)
            .map_err(|_| AgentError::ControllerTrust)?;
        let subnet_routes = SubnetRoutePolicy::compile(&state.configuration_payload)
            .map_err(|_| AgentError::ControllerTrust)?;
        let status = Arc::new(tokio::sync::RwLock::new(DataPlaneStatus::default()));
        let data_plane = Self {
            socket,
            material,
            peers_by_virtual_ip,
            peer_by_endpoint,
            peer_by_node_id,
            candidate_manager,
            relay_manager,
            acl_policy,
            subnet_routes,
            local_node_id_base64: state.node_id_base64.clone(),
            status,
            retired_telemetry: TelemetryCounters::default(),
            configuration_version: state.configuration.version,
            policy_version: state.configuration_payload.policy_version,
        };
        data_plane.synchronize_status().await;
        Ok(data_plane)
    }

    #[must_use]
    pub fn status_handle(&self) -> SharedDataPlaneStatus {
        Arc::clone(&self.status)
    }

    /// Starts one authenticated path probe for an established peer.
    ///
    /// The response is processed by the normal receive loop and published through the shared
    /// data-plane status. Calls are rate limited per peer and never create a plaintext probe.
    ///
    /// # Errors
    ///
    /// Returns a classified error when the peer is unknown, has no established session, is in its
    /// probe cooldown, or the encrypted probe cannot be sent.
    pub async fn start_manual_path_probe(
        &mut self,
        virtual_ip: Ipv4Addr,
    ) -> std::result::Result<(), ManualProbeError> {
        let now = Instant::now();
        let (endpoint, destination_node_id, encoded) = {
            let peer = self
                .peers_by_virtual_ip
                .get_mut(&virtual_ip)
                .ok_or(ManualProbeError::PeerNotFound)?;
            let endpoint = peer
                .active_endpoint
                .ok_or(ManualProbeError::SessionUnavailable)?;
            if !matches!(peer.state, PeerState::Established(_)) {
                return Err(ManualProbeError::SessionUnavailable);
            }
            if peer.pending_path_probe.is_some() || now < peer.path_probe_retry_after {
                return Err(ManualProbeError::Busy);
            }
            let encoded =
                create_path_probe(peer, endpoint, now).map_err(|_| ManualProbeError::SendFailed)?;
            peer.path_probe_retry_after = now + MANUAL_PATH_PROBE_COOLDOWN;
            (endpoint, peer.node_id, encoded)
        };
        let sent = self
            .send_xsp(endpoint, destination_node_id, &encoded)
            .await
            .map_err(|_| ManualProbeError::SendFailed)?;
        if !sent {
            if let Some(peer) = self.peers_by_virtual_ip.get_mut(&virtual_ip) {
                peer.pending_path_probe = None;
            }
            return Err(ManualProbeError::SendFailed);
        }
        self.synchronize_status().await;
        Ok(())
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
        let acl_policy = AclPolicy::compile(&state.configuration_payload)
            .map_err(|_| AgentError::ControllerTrust)?;
        let subnet_routes = SubnetRoutePolicy::compile(&state.configuration_payload)
            .map_err(|_| AgentError::ControllerTrust)?;
        let policy_changed = state.configuration_payload.policy_version != self.policy_version;
        let failed_relays = self.relay_manager.apply_configuration(state)?;
        let relay_candidates = self.relay_manager.candidates();
        let (_, desired, _, _) =
            build_peer_directory(state, self.material.node_id, &relay_candidates)?;
        let mut updated = HashMap::with_capacity(desired.len());
        let now = Instant::now();
        for (virtual_ip, mut replacement) in desired {
            if let Some(mut existing) = self.peers_by_virtual_ip.remove(&virtual_ip) {
                if existing.node_id == replacement.node_id {
                    if policy_changed {
                        clear_queue(&mut existing);
                    }
                    let candidates_changed =
                        candidate_routes_changed(&existing.candidates, &replacement.candidates);
                    existing.candidates = replacement.candidates;
                    if candidates_changed {
                        existing.pending_path_probe = None;
                        existing.path_probe_retry_after = now;
                        if !matches!(existing.state, PeerState::Established(_)) {
                            existing.state = PeerState::Idle;
                            existing.handshake_candidate_attempts = 0;
                            existing.next_proactive_handshake_at = now;
                            existing.active_endpoint = existing
                                .candidates
                                .first()
                                .map(|candidate| candidate.endpoint);
                            existing.path_reason = Some(PathSelectionReason::ConfigurationUpdate);
                        }
                    }
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
                } else {
                    self.retired_telemetry.add_peer(&existing);
                }
            }
            updated.insert(virtual_ip, replacement);
        }
        for retired in self.peers_by_virtual_ip.values() {
            self.retired_telemetry.add_peer(retired);
        }
        self.peers_by_virtual_ip = updated;
        for endpoint in failed_relays {
            fail_relay_path(&mut self.peers_by_virtual_ip, endpoint);
        }
        self.rebuild_endpoint_index()?;
        self.acl_policy = acl_policy;
        self.subnet_routes = subnet_routes;
        self.configuration_version = state.configuration.version;
        self.policy_version = state.configuration_payload.policy_version;
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

    async fn send_xsp(
        &mut self,
        endpoint: SocketAddr,
        destination_node_id: [u8; 16],
        encoded: &[u8],
    ) -> Result<bool> {
        if self.relay_manager.is_relay(endpoint) {
            let Some(envelope) =
                self.relay_manager
                    .wrap(endpoint, destination_node_id, encoded, unix_time()?)?
            else {
                return Ok(false);
            };
            return Ok(udp_send_succeeded(
                "relay",
                endpoint,
                self.socket.send_to(&envelope, endpoint).await,
            ));
        }
        Ok(udp_send_succeeded(
            "direct",
            endpoint,
            self.socket.send_to(encoded, endpoint).await,
        ))
    }

    /// Encrypts or queues one raw IPv4 packet read from TUN.
    ///
    /// The returned boolean is true when the packet was dropped.
    ///
    /// # Errors
    ///
    /// Returns an Agent error when local randomness or protocol state fails.
    pub async fn forward_tun(&mut self, packet: &[u8]) -> Result<bool> {
        if packet.len() > MAX_ENCRYPTED_PAYLOAD_LENGTH {
            return Ok(true);
        }
        let Some(flow) = ipv4_flow(packet) else {
            return Ok(true);
        };
        if !self
            .acl_policy
            .evaluate(
                flow.source,
                flow.destination,
                flow.protocol,
                flow.destination_port,
            )
            .allowed
        {
            self.retired_telemetry.acl_drops = self.retired_telemetry.acl_drops.saturating_add(1);
            self.synchronize_status().await;
            return Ok(true);
        }
        let (destination_peer_ip, routed_packet) =
            if self.peers_by_virtual_ip.contains_key(&flow.destination) {
                if flow.source != self.material.virtual_ip
                    && self
                        .subnet_routes
                        .local_gateway_route(&self.local_node_id_base64, flow.source)
                        .is_none()
                {
                    return Ok(true);
                }
                (flow.destination, flow.source != self.material.virtual_ip)
            } else {
                if flow.source != self.material.virtual_ip {
                    return Ok(true);
                }
                let Some(route) = self
                    .subnet_routes
                    .resolve_destination(flow.destination, Utc::now())
                else {
                    return Ok(true);
                };
                (route.gateway_virtual_ip, true)
            };
        let now = Instant::now();
        let Some(peer) = self.peers_by_virtual_ip.get_mut(&destination_peer_ip) else {
            return Ok(true);
        };
        if !matches!(peer.state, PeerState::Established(_)) && peer.active_endpoint.is_none() {
            return Ok(true);
        }

        let established_path = matches!(peer.state, PeerState::Established(_));
        let outbound = if let PeerState::Established(established) = &mut peer.state {
            let outbound = if routed_packet {
                established
                    .sender
                    .seal_routed_ipv4(DataFlags::ACK_ELICITING, 0, packet)
            } else {
                established
                    .sender
                    .seal_ipv4(DataFlags::ACK_ELICITING, 0, packet)
            };
            let Ok(outbound) = outbound else {
                return Ok(true);
            };
            established.sent_packets_in_epoch = established.sent_packets_in_epoch.saturating_add(1);
            established.last_keepalive_at = now;
            peer.tx_packets_total = peer.tx_packets_total.saturating_add(1);
            peer.tx_bytes_total = peer
                .tx_bytes_total
                .saturating_add(u64::try_from(packet.len()).unwrap_or(u64::MAX));
            outbound
        } else {
            if !queue_packet(peer, packet, routed_packet) {
                return Ok(true);
            }
            peer.tx_packets_total = peer.tx_packets_total.saturating_add(1);
            peer.tx_bytes_total = peer
                .tx_bytes_total
                .saturating_add(u64::try_from(packet.len()).unwrap_or(u64::MAX));
            if matches!(peer.state, PeerState::Idle) {
                begin_client_handshake(&self.material, peer, now)?
            } else {
                return Ok(false);
            }
        };
        let Some(endpoint) = peer.active_endpoint else {
            return Ok(true);
        };
        let destination_node_id = peer.node_id;
        let sent = self
            .send_xsp(endpoint, destination_node_id, &outbound)
            .await?;
        Ok(established_path && !sent)
    }

    /// Receives and processes one UDP datagram, returning an authenticated IPv4 packet for TUN.
    ///
    /// # Errors
    ///
    /// Returns an Agent error only for local socket failures; untrusted protocol input is dropped.
    #[allow(clippy::too_many_lines)]
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
        let now = Instant::now();
        let unix_now = unix_time()?;
        let Some((peer_ip, source_is_known, through_relay, relay_payload)) =
            self.resolve_inbound_peer(source, &datagram[..length], now, unix_now)
        else {
            return Ok(None);
        };
        let packet = relay_payload
            .as_deref()
            .unwrap_or_else(|| &datagram[..length]);
        let handshake_packet = matches!(
            packet.get(5).copied(),
            Some(CLIENT_HELLO_TYPE | SERVER_HELLO_TYPE | CLIENT_FINISH_TYPE | SERVER_FINISH_TYPE)
        );
        let Some(peer_node_id_base64) = self
            .peers_by_virtual_ip
            .get(&peer_ip)
            .map(|peer| peer.node_id_base64.clone())
        else {
            return Ok(None);
        };
        let allow_routed_data = self
            .subnet_routes
            .permits_routed_session(&self.local_node_id_base64, &peer_node_id_base64);
        let peer = self
            .peers_by_virtual_ip
            .get_mut(&peer_ip)
            .ok_or(AgentError::DataPlane)?;
        let result = process_datagram(&self.material, peer, source, packet, now, allow_routed_data);
        let awaiting_path_response = awaiting_path_response(peer, source, handshake_packet);
        let retain_fallback_reason = !through_relay
            && handshake_packet
            && peer.active_endpoint == Some(source)
            && peer.path_reason == Some(PathSelectionReason::HandshakeFallback);
        if result.path_authenticated
            && !retain_fallback_reason
            && !awaiting_path_response
            && (handshake_packet || peer.active_endpoint != Some(source))
        {
            let reason = authenticated_path_reason(peer, source, handshake_packet, through_relay);
            promote_path(peer, source, reason);
        }
        let endpoint_index_changed = !through_relay
            && !source_is_known
            && result.path_authenticated
            && peer.active_endpoint == Some(source);
        let destination_node_id = peer.node_id;
        let mut acl_dropped = false;
        let plaintext = result.plaintext.filter(|plaintext| {
            ipv4_flow(plaintext).is_some_and(|flow| {
                let identity_bound = (flow.source == peer_ip
                    && (flow.destination == self.material.virtual_ip
                        || self
                            .subnet_routes
                            .local_gateway_route(&self.local_node_id_base64, flow.destination)
                            .is_some()))
                    || (flow.destination == self.material.virtual_ip
                        && self
                            .subnet_routes
                            .source_owned_by_gateway(flow.source, &peer_node_id_base64));
                let decision = self.acl_policy.evaluate(
                    flow.source,
                    flow.destination,
                    flow.protocol,
                    flow.destination_port,
                );
                if identity_bound && !decision.allowed {
                    acl_dropped = true;
                }
                identity_bound && decision.allowed
            })
        });
        if let Some(plaintext) = &plaintext {
            peer.rx_packets_total = peer.rx_packets_total.saturating_add(1);
            peer.rx_bytes_total = peer
                .rx_bytes_total
                .saturating_add(u64::try_from(plaintext.len()).unwrap_or(u64::MAX));
        }
        if through_relay && result.path_authenticated {
            self.relay_manager.authenticate_activity(source, now);
        }
        for outbound in result.outbound {
            let _ = self
                .send_xsp(source, destination_node_id, &outbound)
                .await?;
        }
        if endpoint_index_changed {
            self.rebuild_endpoint_index()?;
        }
        if acl_dropped {
            self.retired_telemetry.acl_drops = self.retired_telemetry.acl_drops.saturating_add(1);
        }
        self.synchronize_status().await;
        Ok(plaintext)
    }

    fn resolve_inbound_peer(
        &mut self,
        source: SocketAddr,
        datagram: &[u8],
        now: Instant,
        unix_now: u64,
    ) -> Option<(Ipv4Addr, bool, bool, Option<Vec<u8>>)> {
        match self
            .relay_manager
            .handle_datagram(source, datagram, now, unix_now)
        {
            RelayInbound::Consumed => None,
            RelayInbound::Data {
                source_node_id,
                payload,
            } => self
                .peer_by_node_id
                .get(&source_node_id)
                .copied()
                .map(|peer_ip| (peer_ip, false, true, Some(payload))),
            RelayInbound::NotRelay => {
                let source_is_known = self.peer_by_endpoint.contains_key(&source);
                let peer_ip = if source_is_known {
                    self.peer_by_endpoint.get(&source).copied()
                } else {
                    self.rebinding_peer(datagram)
                }?;
                Some((peer_ip, source_is_known, false, None))
            }
        }
    }

    fn rebinding_peer(&self, datagram: &[u8]) -> Option<Ipv4Addr> {
        let header = DataHeader::parse(datagram).ok()?;
        if header.network_id != self.material.network_id
            || header.destination_node_id != self.material.node_id
        {
            return None;
        }
        let virtual_ip = self.peer_by_node_id.get(&header.source_node_id).copied()?;
        let peer = self.peers_by_virtual_ip.get(&virtual_ip)?;
        let PeerState::Established(established) = &peer.state else {
            return None;
        };
        (established.session_id == header.session_id).then_some(virtual_ip)
    }

    /// Retransmits bounded pending handshakes and expires stale state.
    ///
    /// # Errors
    ///
    /// Returns an Agent error when a UDP retransmission fails.
    pub async fn maintain(&mut self, state: &NodeState) -> Result<Option<CandidateAdvertisement>> {
        let now = Instant::now();
        self.prune_expired_candidates()?;
        let relay_maintenance = self.relay_manager.maintain(now, unix_time()?)?;
        for endpoint in relay_maintenance.failed_endpoints {
            fail_relay_path(&mut self.peers_by_virtual_ip, endpoint);
        }
        for (endpoint, encoded) in relay_maintenance.outbound {
            let _ = udp_send_succeeded(
                "relay_maintenance",
                endpoint,
                self.socket.send_to(&encoded, endpoint).await,
            );
        }
        let mut retransmissions = Vec::new();
        let mut active_client_handshakes = self
            .peers_by_virtual_ip
            .values()
            .filter(|peer| {
                matches!(
                    peer.state,
                    PeerState::ClientHello(_) | PeerState::ClientFinish(_)
                )
            })
            .count();
        let mut proactive_started = 0_usize;
        for peer in self.peers_by_virtual_ip.values_mut() {
            prune_client_hello_cache(peer, now);
            prune_server_finish_cache(peer, now);
            match poll_peer_retry(peer, now) {
                RetryAction::Wait => {}
                RetryAction::Send(encoded) => {
                    if let Some(endpoint) = peer.active_endpoint {
                        retransmissions.push((endpoint, peer.node_id, encoded));
                    }
                }
                RetryAction::Expired => {
                    if advance_handshake_candidate(peer) {
                        if let Some(endpoint) = peer.active_endpoint {
                            let encoded = begin_client_handshake(&self.material, peer, now)?;
                            retransmissions.push((endpoint, peer.node_id, encoded));
                        }
                    } else {
                        schedule_handshake_retry(peer, now);
                        active_client_handshakes = active_client_handshakes.saturating_sub(1);
                    }
                }
            }
            if let Some(encoded) = maintain_peer_session(peer, now)
                && let Some(endpoint) = peer.active_endpoint
            {
                retransmissions.push((endpoint, peer.node_id, encoded));
            }
            if let Some((endpoint, encoded)) = maintain_path_probe(peer, now)? {
                retransmissions.push((endpoint, peer.node_id, encoded));
            }
            if matches!(peer.state, PeerState::Idle)
                && peer.handshake_candidate_attempts >= MAX_HANDSHAKE_CANDIDATES_PER_CYCLE
            {
                schedule_handshake_retry(peer, now);
            }
            if matches!(peer.state, PeerState::Idle)
                && peer.active_endpoint.is_some()
                && now >= peer.next_proactive_handshake_at
                && active_client_handshakes < MAX_CONCURRENT_PROACTIVE_HANDSHAKES
                && proactive_started < MAX_PROACTIVE_HANDSHAKES_PER_TICK
            {
                let endpoint = peer.active_endpoint.ok_or(AgentError::DataPlane)?;
                let encoded = begin_client_handshake(&self.material, peer, now)?;
                retransmissions.push((endpoint, peer.node_id, encoded));
                active_client_handshakes = active_client_handshakes.saturating_add(1);
                proactive_started = proactive_started.saturating_add(1);
            }
        }
        for (endpoint, destination_node_id, encoded) in retransmissions {
            let _ = self
                .send_xsp(endpoint, destination_node_id, &encoded)
                .await?;
        }
        if self.candidate_manager.refresh_due(now)
            && let Err(error) = self.candidate_manager.refresh(&self.socket, state).await
        {
            eprintln!(
                "xs-agent data_plane=candidate_refresh error={}",
                error.code()
            );
            return Err(error);
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
        let mut node_ids = HashMap::new();
        for (virtual_ip, peer) in &self.peers_by_virtual_ip {
            if node_ids.insert(peer.node_id, *virtual_ip).is_some() {
                return Err(AgentError::DataPlane);
            }
            let mut peer_endpoints = std::collections::HashSet::new();
            for endpoint in peer
                .candidates
                .iter()
                .filter(|candidate| candidate.kind != EndpointCandidateKind::Relay)
                .map(|candidate| candidate.endpoint)
                .chain(
                    peer.active_endpoint
                        .filter(|endpoint| !self.relay_manager.is_relay(*endpoint)),
                )
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
        self.peer_by_node_id = node_ids;
        Ok(())
    }

    async fn synchronize_status(&self) {
        let mut status = self.status.write().await;
        status.local_candidates = self.candidate_manager.candidates();
        let mut totals = self.retired_telemetry;
        for peer in self.peers_by_virtual_ip.values() {
            totals.add_peer(peer);
        }
        status.tx_bytes_total = totals.tx_bytes;
        status.rx_bytes_total = totals.rx_bytes;
        status.handshake_attempts_total = totals.handshake_attempts;
        status.handshake_successes_total = totals.handshake_successes;
        status.acl_drops_total = totals.acl_drops;
        status.replay_drops_total = totals.replay_drops;
        status.latency_samples_total = totals.latency_samples;
        status.latency_microseconds_total = totals.latency_microseconds;
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
                        tx_packets_total: peer.tx_packets_total,
                        tx_bytes_total: peer.tx_bytes_total,
                        rx_packets_total: peer.rx_packets_total,
                        rx_bytes_total: peer.rx_bytes_total,
                        handshake_attempts_total: peer.handshake_attempts_total,
                        handshake_successes_total: peer.handshake_successes_total,
                        latency_samples_total: peer.latency_samples_total,
                        latency_microseconds_total: peer.latency_microseconds_total,
                        last_latency_microseconds: peer.last_latency_microseconds,
                    },
                )
            })
            .collect();
    }
}

fn build_peer_directory(
    state: &NodeState,
    local_node_id: [u8; 16],
    relay_candidates: &[EndpointCandidate],
) -> Result<PeerDirectory> {
    let mut local_endpoint = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0));
    let mut peers = HashMap::new();
    let mut endpoints = HashMap::new();
    let mut node_ids = HashMap::new();
    let now = Instant::now();
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
            local_endpoint = direct_endpoints
                .first()
                .copied()
                .or_else(|| {
                    node.candidates
                        .iter()
                        .find(|candidate| {
                            candidate.expires_at > Utc::now()
                                && matches!(
                                    candidate.kind,
                                    EndpointCandidateKind::Local | EndpointCandidateKind::Static
                                )
                        })
                        .map(|candidate| normalize_endpoint(candidate.endpoint))
                })
                .unwrap_or(local_endpoint);
            continue;
        }
        if peers.len() >= MAX_CONFIGURED_PEERS || peers.contains_key(&virtual_ip) {
            return Err(AgentError::DataPlane);
        }
        if node_ids.insert(node_id, virtual_ip).is_some() {
            return Err(AgentError::DataPlane);
        }
        let candidates = configured_candidates(node, direct_endpoints, relay_candidates);
        for candidate in &candidates {
            if candidate.kind != EndpointCandidateKind::Relay
                && endpoints.insert(candidate.endpoint, virtual_ip).is_some()
            {
                return Err(AgentError::DataPlane);
            }
        }
        let active_endpoint = candidates.first().map(|candidate| candidate.endpoint);
        peers.insert(
            virtual_ip,
            Peer {
                node_id,
                node_id_base64: node.node_id_base64.clone(),
                virtual_ip,
                candidates,
                active_endpoint,
                path_reason: active_endpoint.map(|_| PathSelectionReason::HighestPriority),
                state: PeerState::Idle,
                queued_packets: VecDeque::new(),
                queued_bytes: 0,
                recent_client_hellos: VecDeque::new(),
                cached_server_finish: None,
                pending_path_probe: None,
                path_probe_retry_after: now,
                next_latency_probe_at: now,
                next_proactive_handshake_at: now,
                handshake_failures: 0,
                handshake_candidate_attempts: 0,
                tx_packets_total: 0,
                tx_bytes_total: 0,
                rx_packets_total: 0,
                rx_bytes_total: 0,
                handshake_attempts_total: 0,
                handshake_successes_total: 0,
                replay_drops_total: 0,
                latency_samples_total: 0,
                latency_microseconds_total: 0,
                last_latency_microseconds: None,
            },
        );
    }
    Ok((local_endpoint, peers, endpoints, node_ids))
}

fn configured_candidates(
    node: &xs_core::ConfigurationNode,
    direct_endpoints: Vec<SocketAddr>,
    relay_candidates: &[EndpointCandidate],
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
    candidates.sort_by_key(|candidate| std::cmp::Reverse(candidate.priority));
    let relay_limit = relay_candidates.len().min(MAX_RELAY_CANDIDATES_PER_PEER);
    let direct_limit = MAX_HANDSHAKE_CANDIDATES_PER_CYCLE.saturating_sub(relay_limit);
    candidates.truncate(direct_limit);
    for (index, candidate) in candidates.iter_mut().enumerate() {
        candidate.priority = 20_000_u32.saturating_sub(u32::try_from(index).unwrap_or(u32::MAX));
    }
    candidates.extend(relay_candidates.iter().take(relay_limit).cloned());
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

fn candidate_routes_changed(
    current: &[EndpointCandidate],
    replacement: &[EndpointCandidate],
) -> bool {
    current.len() != replacement.len()
        || current
            .iter()
            .zip(replacement)
            .any(|(current, replacement)| {
                current.kind != replacement.kind
                    || current.endpoint != replacement.endpoint
                    || current.priority != replacement.priority
            })
}

fn advance_handshake_candidate(peer: &mut Peer) -> bool {
    if peer.handshake_candidate_attempts >= MAX_HANDSHAKE_CANDIDATES_PER_CYCLE {
        return false;
    }
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

fn schedule_handshake_retry(peer: &mut Peer, now: Instant) {
    clear_queue(peer);
    peer.handshake_candidate_attempts = 0;
    peer.handshake_failures = peer.handshake_failures.saturating_add(1);
    let exponent = u32::from(peer.handshake_failures.saturating_sub(1).min(6));
    let multiplier = 1_u32 << exponent;
    let delay = (HANDSHAKE_BACKOFF_BASE * multiplier).min(HANDSHAKE_BACKOFF_MAX);
    peer.next_proactive_handshake_at = now + delay;
    peer.active_endpoint = peer.candidates.first().map(|candidate| candidate.endpoint);
    peer.path_reason = peer
        .active_endpoint
        .map(|_| PathSelectionReason::HighestPriority);
}

fn promote_path(peer: &mut Peer, endpoint: SocketAddr, reason: PathSelectionReason) {
    peer.active_endpoint = Some(endpoint);
    peer.path_reason = Some(reason);
    peer.pending_path_probe = None;
}

fn authenticated_handshake_reason(peer: &Peer, source: SocketAddr) -> PathSelectionReason {
    if peer.path_reason == Some(PathSelectionReason::HandshakeFallback) {
        return PathSelectionReason::HandshakeFallback;
    }
    let source_priority = peer
        .candidates
        .iter()
        .find(|candidate| candidate.endpoint == source)
        .map(|candidate| candidate.priority);
    let active_priority = peer.active_endpoint.and_then(|active| {
        peer.candidates
            .iter()
            .find(|candidate| candidate.endpoint == active)
            .map(|candidate| candidate.priority)
    });
    if peer.handshake_candidate_attempts > 0
        && source_priority
            .zip(active_priority)
            .is_some_and(|(source, active)| source < active)
    {
        PathSelectionReason::HandshakeFallback
    } else {
        PathSelectionReason::AuthenticatedHandshake
    }
}

fn authenticated_path_reason(
    peer: &Peer,
    source: SocketAddr,
    handshake_packet: bool,
    through_relay: bool,
) -> PathSelectionReason {
    if through_relay {
        let active_is_relay = peer.active_endpoint.is_some_and(|active| {
            peer.candidates.iter().any(|candidate| {
                candidate.endpoint == active && candidate.kind == EndpointCandidateKind::Relay
            })
        });
        return classify_authenticated_relay_path(
            peer.path_reason,
            peer.active_endpoint,
            source,
            active_is_relay,
        );
    }
    if handshake_packet {
        authenticated_handshake_reason(peer, source)
    } else {
        PathSelectionReason::AuthenticatedPeerTraffic
    }
}

fn classify_authenticated_relay_path(
    current_reason: Option<PathSelectionReason>,
    active_endpoint: Option<SocketAddr>,
    source: SocketAddr,
    active_is_relay: bool,
) -> PathSelectionReason {
    if current_reason == Some(PathSelectionReason::RelayFailover)
        || (active_is_relay && active_endpoint.is_some_and(|active| active != source))
    {
        PathSelectionReason::RelayFailover
    } else {
        PathSelectionReason::RelayFallback
    }
}

fn awaiting_path_response(peer: &Peer, source: SocketAddr, handshake_packet: bool) -> bool {
    !handshake_packet
        && peer
            .pending_path_probe
            .as_ref()
            .is_some_and(|pending| pending.endpoint == source)
}

fn fail_relay_path(peers: &mut HashMap<Ipv4Addr, Peer>, failed_endpoint: SocketAddr) {
    let now = Instant::now();
    for peer in peers.values_mut() {
        if peer.active_endpoint != Some(failed_endpoint) {
            continue;
        }
        let next_index = peer
            .candidates
            .iter()
            .position(|candidate| candidate.endpoint == failed_endpoint)
            .map_or(0, |index| index.saturating_add(1));
        peer.active_endpoint = peer
            .candidates
            .iter()
            .skip(next_index)
            .find(|candidate| candidate.endpoint != failed_endpoint)
            .or_else(|| {
                peer.candidates
                    .iter()
                    .find(|candidate| candidate.endpoint != failed_endpoint)
            })
            .map(|candidate| candidate.endpoint);
        peer.path_reason = peer
            .active_endpoint
            .map(|_| PathSelectionReason::RelayFailover);
        peer.pending_path_probe = None;
        peer.path_probe_retry_after = now;
        if !matches!(peer.state, PeerState::Established(_)) {
            peer.state = PeerState::Idle;
            peer.handshake_candidate_attempts = 0;
            peer.next_proactive_handshake_at = now;
        }
    }
}

fn maintain_path_probe(peer: &mut Peer, now: Instant) -> Result<Option<(SocketAddr, Vec<u8>)>> {
    if !matches!(peer.state, PeerState::Established(_)) {
        peer.pending_path_probe = None;
        return Ok(None);
    }
    if peer.pending_path_probe.is_some() {
        let decision = peer
            .pending_path_probe
            .as_mut()
            .map(|pending| pending.retry.poll(now))
            .ok_or(AgentError::DataPlane)?;
        return match decision {
            RetryDecision::Wait => Ok(None),
            RetryDecision::Retry => {
                let pending = peer
                    .pending_path_probe
                    .as_ref()
                    .ok_or(AgentError::DataPlane)?;
                let endpoint = pending.endpoint;
                let path_id = pending.path_id;
                let token = pending.token;
                let PeerState::Established(established) = &mut peer.state else {
                    return Err(AgentError::DataPlane);
                };
                let encoded = established
                    .sender
                    .seal_control(
                        PacketType::PathChallenge,
                        DataFlags::PATH_PROBE,
                        path_id,
                        &token,
                    )
                    .map_err(|_| AgentError::DataPlane)?;
                Ok(Some((endpoint, encoded)))
            }
            RetryDecision::Expired => {
                peer.pending_path_probe = None;
                peer.path_probe_retry_after = now + PATH_PROBE_COOLDOWN;
                peer.next_latency_probe_at = now + LATENCY_PROBE_INTERVAL;
                Ok(None)
            }
        };
    }
    if now < peer.path_probe_retry_after {
        return Ok(None);
    }
    let target = best_probe_target(peer).or_else(|| {
        (matches!(peer.state, PeerState::Established(_)) && now >= peer.next_latency_probe_at)
            .then_some(peer.active_endpoint)
            .flatten()
    });
    let Some(endpoint) = target else {
        return Ok(None);
    };
    create_path_probe(peer, endpoint, now).map(|encoded| Some((endpoint, encoded)))
}

fn create_path_probe(peer: &mut Peer, endpoint: SocketAddr, now: Instant) -> Result<Vec<u8>> {
    let PeerState::Established(established) = &mut peer.state else {
        return Err(AgentError::DataPlane);
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
        promotes_path: peer.active_endpoint != Some(endpoint),
        retry: RetrySchedule::path_probe(now),
    });
    peer.next_latency_probe_at = now + LATENCY_PROBE_INTERVAL;
    Ok(encoded)
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

fn queue_packet(peer: &mut Peer, packet: &[u8], routed: bool) -> bool {
    if peer.queued_packets.len() >= MAX_QUEUED_PACKETS_PER_PEER
        || peer.queued_bytes.saturating_add(packet.len()) > MAX_QUEUED_BYTES_PER_PEER
    {
        return false;
    }
    peer.queued_bytes = peer.queued_bytes.saturating_add(packet.len());
    peer.queued_packets.push_back(QueuedPacket {
        encoded: packet.to_vec(),
        routed,
    });
    true
}

fn begin_client_handshake(
    material: &LocalMaterial,
    peer: &mut Peer,
    now: Instant,
) -> Result<Vec<u8>> {
    if peer.active_endpoint.is_none()
        || peer.handshake_candidate_attempts >= MAX_HANDSHAKE_CANDIDATES_PER_CYCLE
    {
        return Err(AgentError::DataPlane);
    }
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
    peer.handshake_attempts_total = peer.handshake_attempts_total.saturating_add(1);
    let encoded = machine.encoded().to_vec();
    peer.state = PeerState::ClientHello(Box::new(ClientHelloPending {
        machine,
        retry: RetryState::handshake(encoded.clone(), now),
    }));
    peer.handshake_candidate_attempts = peer.handshake_candidate_attempts.saturating_add(1);
    Ok(encoded)
}

fn process_datagram(
    material: &LocalMaterial,
    peer: &mut Peer,
    source: SocketAddr,
    datagram: &[u8],
    now: Instant,
    allow_routed_data: bool,
) -> ProcessResult {
    match datagram.get(5).copied() {
        Some(CLIENT_HELLO_TYPE) => handle_client_hello(material, peer, datagram, now),
        Some(SERVER_HELLO_TYPE) => handle_server_hello(material, peer, datagram, now),
        Some(CLIENT_FINISH_TYPE) => handle_client_finish(peer, datagram, now),
        Some(SERVER_FINISH_TYPE) => handle_server_finish(peer, datagram),
        Some(value) if PacketType::try_from(value).is_ok() => {
            handle_data(peer, source, datagram, now, allow_routed_data)
        }
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
    peer.handshake_attempts_total = peer.handshake_attempts_total.saturating_add(1);
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

fn handle_client_finish(peer: &mut Peer, datagram: &[u8], now: Instant) -> ProcessResult {
    prune_server_finish_cache(peer, now);
    let client_finish_hash: [u8; 32] = Sha256::digest(datagram).into();
    if let Some(cached) = &peer.cached_server_finish
        && cached.client_finish_hash == client_finish_hash
    {
        return ProcessResult {
            outbound: vec![cached.encoded.clone()],
            plaintext: None,
            path_authenticated: false,
        };
    }
    let state = std::mem::replace(&mut peer.state, PeerState::Idle);
    let PeerState::ServerHello(pending) = state else {
        peer.state = state;
        return ProcessResult::empty();
    };
    let Ok((session, server_finish)) = pending.machine.accept_client_finish(datagram) else {
        clear_queue(peer);
        return ProcessResult::empty();
    };
    let mut outbound = vec![server_finish.clone()];
    outbound.extend(install_session(peer, session));
    if matches!(peer.state, PeerState::Established(_)) {
        peer.cached_server_finish = Some(CachedServerFinish {
            client_finish_hash,
            encoded: server_finish,
            expires_at: now + SERVER_FINISH_CACHE_LIFETIME,
        });
    }
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

fn handle_data(
    peer: &mut Peer,
    source: SocketAddr,
    datagram: &[u8],
    now: Instant,
    allow_routed_data: bool,
) -> ProcessResult {
    let mut path_probe_result = None;
    let (opened, replay_drop_delta) = {
        let PeerState::Established(established) = &mut peer.state else {
            return ProcessResult::empty();
        };
        let replay_drops_before = established.receiver.replay_drops_total();
        let opened = match established.receiver.open(datagram) {
            Ok(opened) => Ok(opened),
            Err(_)
                if allow_routed_data
                    && established.receiver.replay_drops_total() == replay_drops_before
                    && datagram.get(5).copied() == Some(PacketType::Data as u8) =>
            {
                established.receiver.open_routed(datagram)
            }
            Err(error) => Err(error),
        };
        (
            opened,
            established
                .receiver
                .replay_drops_total()
                .saturating_sub(replay_drops_before),
        )
    };
    peer.replay_drops_total = peer.replay_drops_total.saturating_add(replay_drop_delta);
    let Ok(opened) = opened else {
        return ProcessResult::empty();
    };
    let result = {
        let PeerState::Established(established) = &mut peer.state else {
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
                path_probe_result = peer.pending_path_probe.as_ref().and_then(|pending| {
                    (pending.endpoint == source
                        && pending.path_id == opened.path_id
                        && pending.token.as_slice() == opened.plaintext)
                        .then(|| {
                            (
                                now.saturating_duration_since(pending.retry.last_sent),
                                pending.promotes_path,
                            )
                        })
                });
                ProcessResult::empty()
            }
            PacketType::Close => ProcessResult::empty(),
        }
    };
    if let Some((latency, promotes_path)) = path_probe_result {
        peer.pending_path_probe = None;
        let latency_microseconds = u64::try_from(latency.as_micros()).unwrap_or(u64::MAX);
        peer.last_latency_microseconds = Some(latency_microseconds);
        peer.latency_samples_total = peer.latency_samples_total.saturating_add(1);
        peer.latency_microseconds_total = peer
            .latency_microseconds_total
            .saturating_add(latency_microseconds);
        peer.next_latency_probe_at = now + LATENCY_PROBE_INTERVAL;
        if promotes_path {
            promote_path(peer, source, PathSelectionReason::AuthenticatedPathProbe);
        }
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
        let encoded = if packet.routed {
            sender.seal_routed_ipv4(DataFlags::ACK_ELICITING, 0, &packet.encoded)
        } else {
            sender.seal_ipv4(DataFlags::ACK_ELICITING, 0, &packet.encoded)
        };
        if let Ok(encoded) = encoded {
            outbound.push(encoded);
        }
    }
    peer.queued_bytes = 0;
    peer.handshake_failures = 0;
    peer.handshake_candidate_attempts = 0;
    peer.handshake_successes_total = peer.handshake_successes_total.saturating_add(1);
    let now = Instant::now();
    peer.next_proactive_handshake_at = now;
    peer.state = PeerState::Established(Box::new(EstablishedPeer {
        session_id,
        sender,
        receiver,
        sent_packets_in_epoch: 0,
        epoch_started_at: now,
        outbound_key_update: None,
        previous_epoch_installed_at: None,
        last_keepalive_at: now,
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

fn maintain_peer_session(peer: &mut Peer, now: Instant) -> Option<Vec<u8>> {
    let maintenance = match &mut peer.state {
        PeerState::Established(established) => maintain_established(established, now),
        _ => EstablishedMaintenance::Wait,
    };
    match maintenance {
        EstablishedMaintenance::Wait => None,
        EstablishedMaintenance::Send(encoded) => Some(encoded),
        EstablishedMaintenance::Rehandshake => {
            peer.state = PeerState::Idle;
            peer.pending_path_probe = None;
            peer.handshake_candidate_attempts = 0;
            peer.next_proactive_handshake_at = now;
            None
        }
    }
}

fn maintain_established(established: &mut EstablishedPeer, now: Instant) -> EstablishedMaintenance {
    if established
        .previous_epoch_installed_at
        .is_some_and(|installed| now.duration_since(installed) >= PREVIOUS_EPOCH_RETENTION)
    {
        established.receiver.retire_previous_epoch();
        established.previous_epoch_installed_at = None;
    }

    if let Some(pending) = &mut established.outbound_key_update {
        return match pending.retry.poll(now) {
            RetryDecision::Wait => EstablishedMaintenance::Wait,
            RetryDecision::Retry => match established.sender.seal_control(
                PacketType::KeyUpdate,
                DataFlags::CONTROL,
                0,
                &pending.payload,
            ) {
                Ok(encoded) => EstablishedMaintenance::Send(encoded),
                Err(_) => EstablishedMaintenance::Rehandshake,
            },
            RetryDecision::Expired => {
                established.outbound_key_update = None;
                EstablishedMaintenance::Rehandshake
            }
        };
    }
    if established.sent_packets_in_epoch >= KEY_UPDATE_PACKET_LIMIT
        || now.duration_since(established.epoch_started_at) >= KEY_UPDATE_INTERVAL
    {
        let current_epoch = established.sender.current_epoch();
        let Some(next_epoch) = current_epoch.checked_add(1) else {
            return EstablishedMaintenance::Rehandshake;
        };
        let Ok(payload) = key_update_payload(established.session_id, current_epoch, next_epoch)
        else {
            return EstablishedMaintenance::Rehandshake;
        };
        let Ok(encoded) =
            established
                .sender
                .seal_control(PacketType::KeyUpdate, DataFlags::CONTROL, 0, &payload)
        else {
            return EstablishedMaintenance::Rehandshake;
        };
        established.outbound_key_update = Some(PendingKeyUpdate {
            next_epoch,
            payload,
            retry: RetrySchedule::key_update(now),
        });
        return EstablishedMaintenance::Send(encoded);
    }

    if now.duration_since(established.last_keepalive_at) >= KEEPALIVE_INTERVAL {
        let Ok(encoded) =
            established
                .sender
                .seal_control(PacketType::Keepalive, DataFlags::NONE, 0, &[])
        else {
            return EstablishedMaintenance::Rehandshake;
        };
        established.last_keepalive_at = now;
        return EstablishedMaintenance::Send(encoded);
    }
    EstablishedMaintenance::Wait
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

fn prune_server_finish_cache(peer: &mut Peer, now: Instant) {
    if peer
        .cached_server_finish
        .as_ref()
        .is_some_and(|cached| now >= cached.expires_at)
    {
        peer.cached_server_finish = None;
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Ipv4Flow {
    source: Ipv4Addr,
    destination: Ipv4Addr,
    protocol: AclProtocol,
    destination_port: Option<u16>,
}

fn ipv4_flow(packet: &[u8]) -> Option<Ipv4Flow> {
    if packet.len() < 20 || packet[0] >> 4 != 4 {
        return None;
    }
    let header_length = usize::from(packet[0] & 0x0f).checked_mul(4)?;
    if header_length < 20 || packet.len() < header_length {
        return None;
    }
    let total_length = usize::from(u16::from_be_bytes([packet[2], packet[3]]));
    let fragment = u16::from_be_bytes([packet[6], packet[7]]);
    if total_length < header_length || total_length > packet.len() || fragment & 0x3fff != 0 {
        return None;
    }
    let protocol = match packet[9] {
        1 => AclProtocol::Icmp,
        6 => AclProtocol::Tcp,
        17 => AclProtocol::Udp,
        _ => return None,
    };
    let destination_port = if matches!(protocol, AclProtocol::Tcp | AclProtocol::Udp) {
        let transport = packet.get(header_length..total_length)?;
        if transport.len() < 4 {
            return None;
        }
        Some(u16::from_be_bytes([transport[2], transport[3]]))
    } else {
        None
    };
    Some(Ipv4Flow {
        source: Ipv4Addr::new(packet[12], packet[13], packet[14], packet[15]),
        destination: Ipv4Addr::new(packet[16], packet[17], packet[18], packet[19]),
        protocol,
        destination_port,
    })
}

fn random_array<const LENGTH: usize>() -> Result<[u8; LENGTH]> {
    let mut bytes = [0_u8; LENGTH];
    fill(&mut bytes).map_err(|_| AgentError::DataPlane)?;
    Ok(bytes)
}

fn unix_time() -> Result<u64> {
    u64::try_from(Utc::now().timestamp()).map_err(|_| AgentError::DataPlane)
}

fn udp_send_succeeded(
    operation: &str,
    endpoint: SocketAddr,
    result: std::io::Result<usize>,
) -> bool {
    match result {
        Ok(_) => true,
        Err(error) => {
            eprintln!("xs-agent udp_send={operation} endpoint={endpoint} error={error}");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use xs_protocol::{CredentialClaims, role_set_digest, sign_credential};

    const TEST_UNIX_TIME: u64 = 1_700_000_100;

    struct ProtocolFixture {
        controller: SigningKey,
        client_identity: SigningKey,
        server_identity: SigningKey,
        client_credential: [u8; CREDENTIAL_LENGTH],
        server_credential: [u8; CREDENTIAL_LENGTH],
        client_context: HandshakeContext,
        server_context: HandshakeContext,
    }

    fn protocol_fixture() -> ProtocolFixture {
        let controller = SigningKey::from_bytes(&[11_u8; 32]);
        let client_identity = SigningKey::from_bytes(&[21_u8; 32]);
        let server_identity = SigningKey::from_bytes(&[31_u8; 32]);
        let network_id = [41_u8; 16];
        let client_ip = Ipv4Addr::new(100, 88, 0, 21);
        let server_ip = Ipv4Addr::new(100, 88, 0, 31);
        let tags = vec!["linux".to_owned()];
        let client_credential = sign_credential(
            CredentialClaims {
                network_id,
                identity_public_key: client_identity.verifying_key().to_bytes(),
                virtual_ipv4: client_ip,
                serial: 21,
                not_before: TEST_UNIX_TIME - 60,
                not_after: TEST_UNIX_TIME + 3_600,
                role_bitmap: 1,
                role_set_digest: role_set_digest(1, &tags).expect("valid role set"),
            },
            &controller,
        );
        let server_credential = sign_credential(
            CredentialClaims {
                network_id,
                identity_public_key: server_identity.verifying_key().to_bytes(),
                virtual_ipv4: server_ip,
                serial: 31,
                not_before: TEST_UNIX_TIME - 60,
                not_after: TEST_UNIX_TIME + 3_600,
                role_bitmap: 1,
                role_set_digest: role_set_digest(1, &tags).expect("valid role set"),
            },
            &controller,
        );
        let client_node_id = xs_protocol::node_id(client_identity.verifying_key().as_bytes());
        let server_node_id = xs_protocol::node_id(server_identity.verifying_key().as_bytes());
        ProtocolFixture {
            controller,
            client_identity,
            server_identity,
            client_credential,
            server_credential,
            client_context: HandshakeContext {
                network_id,
                local_node_id: client_node_id,
                local_virtual_ip: client_ip,
                peer_node_id: server_node_id,
                peer_virtual_ip: server_ip,
            },
            server_context: HandshakeContext {
                network_id,
                local_node_id: server_node_id,
                local_virtual_ip: server_ip,
                peer_node_id: client_node_id,
                peer_virtual_ip: client_ip,
            },
        }
    }

    fn client_parameters(fixture: &ProtocolFixture) -> ClientHandshakeParameters {
        ClientHandshakeParameters {
            context: fixture.client_context,
            credential: fixture.client_credential,
            ephemeral_private_key: EphemeralPrivateKey::from_bytes([51_u8; 32]),
            client_nonce: [61_u8; 32],
            message_id: 0x1020_3040,
            client_time: TEST_UNIX_TIME,
        }
    }

    fn server_parameters(fixture: &ProtocolFixture) -> ServerHandshakeParameters {
        ServerHandshakeParameters {
            context: fixture.server_context,
            credential: fixture.server_credential,
            ephemeral_private_key: EphemeralPrivateKey::from_bytes([71_u8; 32]),
            server_nonce: [81_u8; 32],
            session_id: [91_u8; 16],
            server_time: TEST_UNIX_TIME,
        }
    }

    fn empty_test_peer(
        node_id: [u8; 16],
        virtual_ip: Ipv4Addr,
        active_endpoint: SocketAddr,
        now: Instant,
    ) -> Peer {
        Peer {
            node_id,
            node_id_base64: "test-peer".to_owned(),
            virtual_ip,
            candidates: Vec::new(),
            active_endpoint: Some(active_endpoint),
            path_reason: Some(PathSelectionReason::HighestPriority),
            state: PeerState::Idle,
            queued_packets: VecDeque::new(),
            queued_bytes: 0,
            recent_client_hellos: VecDeque::new(),
            cached_server_finish: None,
            pending_path_probe: None,
            path_probe_retry_after: now,
            next_latency_probe_at: now,
            next_proactive_handshake_at: now,
            handshake_failures: 0,
            handshake_candidate_attempts: 0,
            tx_packets_total: 0,
            tx_bytes_total: 0,
            rx_packets_total: 0,
            rx_bytes_total: 0,
            handshake_attempts_total: 0,
            handshake_successes_total: 0,
            replay_drops_total: 0,
            latency_samples_total: 0,
            latency_microseconds_total: 0,
            last_latency_microseconds: None,
        }
    }

    fn established_test_peers(now: Instant) -> (Peer, Peer, SocketAddr, SocketAddr) {
        let fixture = protocol_fixture();
        let client = ClientHelloSent::start(client_parameters(&fixture), &fixture.client_identity)
            .expect("client hello");
        let server = ServerHelloSent::accept(
            client.encoded(),
            server_parameters(&fixture),
            &fixture.server_identity,
            &fixture.controller.verifying_key(),
            TEST_UNIX_TIME,
        )
        .expect("server hello");
        let client = client
            .accept_server_hello(
                server.encoded(),
                &fixture.controller.verifying_key(),
                TEST_UNIX_TIME,
            )
            .expect("client finish");
        let (server_session, server_finish) = server
            .accept_client_finish(client.encoded())
            .expect("server finish");
        let client_session = client
            .accept_server_finish(&server_finish)
            .expect("client session");
        let client_endpoint = "192.0.2.10:42001".parse().expect("client endpoint");
        let server_endpoint = "192.0.2.20:42001".parse().expect("server endpoint");
        let mut client_peer = empty_test_peer(
            fixture.client_context.peer_node_id,
            fixture.client_context.peer_virtual_ip,
            server_endpoint,
            now,
        );
        let mut server_peer = empty_test_peer(
            fixture.server_context.peer_node_id,
            fixture.server_context.peer_virtual_ip,
            client_endpoint,
            now,
        );
        assert!(install_session(&mut client_peer, client_session).is_empty());
        assert!(install_session(&mut server_peer, server_session).is_empty());
        (client_peer, server_peer, client_endpoint, server_endpoint)
    }

    #[test]
    fn ipv4_flow_requires_a_complete_unfragmented_packet() {
        let mut packet = [0_u8; 28];
        packet[0] = 0x45;
        packet[2..4].copy_from_slice(&28_u16.to_be_bytes());
        packet[9] = 17;
        packet[12..16].copy_from_slice(&[100, 88, 0, 1]);
        packet[16..20].copy_from_slice(&[100, 88, 0, 2]);
        packet[22..24].copy_from_slice(&53_u16.to_be_bytes());
        assert_eq!(
            ipv4_flow(&packet),
            Some(Ipv4Flow {
                source: Ipv4Addr::new(100, 88, 0, 1),
                destination: Ipv4Addr::new(100, 88, 0, 2),
                protocol: AclProtocol::Udp,
                destination_port: Some(53),
            })
        );
        packet[0] = 0x65;
        assert_eq!(ipv4_flow(&packet), None);
        packet[0] = 0x45;
        packet[6] = 0x20;
        assert_eq!(ipv4_flow(&packet), None);
        assert_eq!(ipv4_flow(&packet[..19]), None);
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

    #[test]
    fn udp_send_errors_are_non_fatal_unsent_results() {
        let endpoint = "127.0.0.1:9".parse().expect("valid endpoint");
        assert!(udp_send_succeeded("direct", endpoint, Ok(128)));
        assert!(!udp_send_succeeded(
            "direct",
            endpoint,
            Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied)),
        ));
    }

    #[test]
    fn candidate_expiry_refresh_does_not_reset_route_progress() {
        let endpoint = "192.0.2.1:443".parse().expect("valid endpoint");
        let current = vec![EndpointCandidate {
            kind: EndpointCandidateKind::Mapped,
            endpoint,
            priority: 20_000,
            expires_at: Utc::now() + chrono::Duration::minutes(5),
        }];
        let mut replacement = current.clone();
        replacement[0].expires_at += chrono::Duration::minutes(5);
        assert!(!candidate_routes_changed(&current, &replacement));

        replacement[0].priority -= 1;
        assert!(candidate_routes_changed(&current, &replacement));
    }

    #[test]
    fn handshake_fallback_does_not_start_a_path_probe_before_session_establishment() {
        let now = Instant::now();
        let preferred = "192.0.2.10:42001".parse().expect("preferred endpoint");
        let fallback = "192.0.2.20:42001".parse().expect("fallback endpoint");
        let expires_at = Utc::now() + chrono::Duration::minutes(5);
        let mut peer = Peer {
            node_id: [1; 16],
            node_id_base64: "test-peer".to_owned(),
            virtual_ip: Ipv4Addr::new(100, 127, 253, 2),
            candidates: vec![
                EndpointCandidate {
                    kind: EndpointCandidateKind::Static,
                    endpoint: preferred,
                    priority: 200,
                    expires_at,
                },
                EndpointCandidate {
                    kind: EndpointCandidateKind::Static,
                    endpoint: fallback,
                    priority: 199,
                    expires_at,
                },
            ],
            active_endpoint: Some(fallback),
            path_reason: Some(PathSelectionReason::HandshakeFallback),
            state: PeerState::Idle,
            queued_packets: VecDeque::new(),
            queued_bytes: 0,
            recent_client_hellos: VecDeque::new(),
            cached_server_finish: None,
            pending_path_probe: None,
            path_probe_retry_after: now,
            next_latency_probe_at: now,
            next_proactive_handshake_at: now,
            handshake_failures: 0,
            handshake_candidate_attempts: 1,
            tx_packets_total: 0,
            tx_bytes_total: 0,
            rx_packets_total: 0,
            rx_bytes_total: 0,
            handshake_attempts_total: 1,
            handshake_successes_total: 0,
            replay_drops_total: 0,
            latency_samples_total: 0,
            latency_microseconds_total: 0,
            last_latency_microseconds: None,
        };

        assert!(
            maintain_path_probe(&mut peer, now)
                .expect("pre-session path probe must remain idle")
                .is_none()
        );
        assert!(peer.pending_path_probe.is_none());
    }

    #[test]
    fn authenticated_alternate_relay_is_classified_as_failover() {
        let first_relay = "192.0.2.10:443".parse().expect("first relay");
        let second_relay = "192.0.2.20:443".parse().expect("second relay");

        assert_eq!(
            classify_authenticated_relay_path(
                Some(PathSelectionReason::RelayFallback),
                Some(first_relay),
                second_relay,
                true,
            ),
            PathSelectionReason::RelayFailover
        );
        assert_eq!(
            classify_authenticated_relay_path(
                Some(PathSelectionReason::AuthenticatedPeerTraffic),
                Some(first_relay),
                second_relay,
                false,
            ),
            PathSelectionReason::RelayFallback
        );
    }

    #[test]
    fn key_update_retry_uses_fresh_sequence_and_recovers_a_lost_ack() {
        let now = Instant::now();
        let (mut client_peer, mut server_peer, client_endpoint, server_endpoint) =
            established_test_peers(now);
        let PeerState::Established(client) = &mut client_peer.state else {
            panic!("client session must be established");
        };
        client.sent_packets_in_epoch = KEY_UPDATE_PACKET_LIMIT;
        let EstablishedMaintenance::Send(first) = maintain_established(client, now) else {
            panic!("initial key update must be sent");
        };
        let first_response = handle_data(&mut server_peer, client_endpoint, &first, now, false);
        assert_eq!(first_response.outbound.len(), 1);

        let retry_time = now + KEY_UPDATE_RETRY_INTERVAL;
        let PeerState::Established(client) = &mut client_peer.state else {
            panic!("client session must remain established");
        };
        let EstablishedMaintenance::Send(retry) = maintain_established(client, retry_time) else {
            panic!("key update retry must be sent");
        };
        assert_ne!(first, retry, "AEAD retries require a fresh sequence");
        let retry_response =
            handle_data(&mut server_peer, client_endpoint, &retry, retry_time, false);
        assert_eq!(retry_response.outbound.len(), 1);
        let _ = handle_data(
            &mut client_peer,
            server_endpoint,
            &retry_response.outbound[0],
            retry_time,
            false,
        );
        let PeerState::Established(client) = &client_peer.state else {
            panic!("client session must remain established");
        };
        assert_eq!(client.sender.current_epoch(), 1);
        assert!(client.outbound_key_update.is_none());
    }

    #[test]
    fn path_probe_retry_uses_fresh_sequence_and_recovers_a_lost_response() {
        let now = Instant::now();
        let (mut client_peer, mut server_peer, client_endpoint, server_endpoint) =
            established_test_peers(now);
        let first = create_path_probe(&mut client_peer, server_endpoint, now)
            .expect("initial path challenge");
        let first_response = handle_data(&mut server_peer, client_endpoint, &first, now, false);
        assert_eq!(first_response.outbound.len(), 1);

        let retry_time = now + PATH_PROBE_RETRY_INTERVAL;
        let (retry_endpoint, retry) = maintain_path_probe(&mut client_peer, retry_time)
            .expect("path probe retry maintenance")
            .expect("path challenge retry");
        assert_eq!(retry_endpoint, server_endpoint);
        assert_ne!(first, retry, "AEAD retries require a fresh sequence");
        let retry_response =
            handle_data(&mut server_peer, client_endpoint, &retry, retry_time, false);
        assert_eq!(retry_response.outbound.len(), 1);
        let _ = handle_data(
            &mut client_peer,
            server_endpoint,
            &retry_response.outbound[0],
            retry_time,
            false,
        );
        assert!(client_peer.pending_path_probe.is_none());
    }

    #[test]
    fn authenticated_replay_updates_peer_telemetry_without_exposing_error_details() {
        let now = Instant::now();
        let (mut client_peer, mut server_peer, client_endpoint, _) = established_test_peers(now);
        let mut packet = vec![0_u8; 20];
        packet[0] = 0x45;
        packet[2..4].copy_from_slice(&20_u16.to_be_bytes());
        packet[12..16].copy_from_slice(&[100, 88, 0, 21]);
        packet[16..20].copy_from_slice(&[100, 88, 0, 31]);
        let PeerState::Established(client) = &mut client_peer.state else {
            panic!("client session must be established");
        };
        let encoded = client
            .sender
            .seal_ipv4(DataFlags::ACK_ELICITING, 0, &packet)
            .expect("data packet");

        let first = handle_data(&mut server_peer, client_endpoint, &encoded, now, false);
        assert_eq!(first.plaintext, Some(packet));
        assert_eq!(server_peer.replay_drops_total, 0);
        let replay = handle_data(&mut server_peer, client_endpoint, &encoded, now, false);
        assert!(replay.plaintext.is_none());
        assert_eq!(server_peer.replay_drops_total, 1);

        let mut tampered = encoded;
        *tampered.last_mut().expect("tag byte") ^= 1;
        let invalid = handle_data(&mut server_peer, client_endpoint, &tampered, now, false);
        assert!(invalid.plaintext.is_none());
        assert_eq!(server_peer.replay_drops_total, 1);
    }

    #[test]
    fn duplicate_client_finish_retransmits_cached_server_finish() {
        let now = Instant::now();
        let fixture = protocol_fixture();
        let client = ClientHelloSent::start(client_parameters(&fixture), &fixture.client_identity)
            .expect("client hello");
        let request_hash = Sha256::digest(client.encoded()).into();
        let server = ServerHelloSent::accept(
            client.encoded(),
            server_parameters(&fixture),
            &fixture.server_identity,
            &fixture.controller.verifying_key(),
            TEST_UNIX_TIME,
        )
        .expect("server hello");
        let server_hello = server.encoded().to_vec();
        let client = client
            .accept_server_hello(
                &server_hello,
                &fixture.controller.verifying_key(),
                TEST_UNIX_TIME,
            )
            .expect("client finish");
        let endpoint = "192.0.2.20:42001".parse().expect("server endpoint");
        let mut server_peer = empty_test_peer(
            fixture.server_context.peer_node_id,
            fixture.server_context.peer_virtual_ip,
            endpoint,
            now,
        );
        server_peer.state = PeerState::ServerHello(Box::new(ServerHelloPending {
            machine: server,
            request_hash,
            retry: RetryState::handshake(server_hello, now),
        }));

        let first = handle_client_finish(&mut server_peer, client.encoded(), now);
        assert_eq!(first.outbound.len(), 1);
        let retry = handle_client_finish(
            &mut server_peer,
            client.encoded(),
            now + HANDSHAKE_RETRY_INTERVAL,
        );
        assert_eq!(retry.outbound, first.outbound);
        assert!(client.accept_server_finish(&retry.outbound[0]).is_ok());
        assert!(matches!(server_peer.state, PeerState::Established(_)));
    }

    #[test]
    fn exhausted_key_update_retries_require_a_full_rehandshake() {
        let now = Instant::now();
        let (mut client_peer, _, _, _) = established_test_peers(now);
        let PeerState::Established(client) = &mut client_peer.state else {
            panic!("client session must be established");
        };
        client.sent_packets_in_epoch = KEY_UPDATE_PACKET_LIMIT;
        assert!(matches!(
            maintain_established(client, now),
            EstablishedMaintenance::Send(_)
        ));
        for attempt in 2..=KEY_UPDATE_MAX_ATTEMPTS {
            let retry_time = now + KEY_UPDATE_RETRY_INTERVAL * u32::from(attempt.saturating_sub(1));
            assert!(matches!(
                maintain_established(client, retry_time),
                EstablishedMaintenance::Send(_)
            ));
        }
        let expired = now + KEY_UPDATE_RETRY_INTERVAL * u32::from(KEY_UPDATE_MAX_ATTEMPTS);
        assert!(matches!(
            maintain_established(client, expired),
            EstablishedMaintenance::Rehandshake
        ));
        assert!(client.outbound_key_update.is_none());
    }
}
