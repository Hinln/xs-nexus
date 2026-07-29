use std::{
    collections::{HashMap, VecDeque},
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    sync::Arc,
    time::{Duration, Instant},
};

use chrono::Utc;
use ed25519_dalek::VerifyingKey;
use getrandom::fill;
use sha2::{Digest, Sha256};
use tokio::net::UdpSocket;
use xs_protocol::{
    CLIENT_FINISH_TYPE, CLIENT_HELLO_TYPE, CREDENTIAL_LENGTH, ClientFinishSent,
    ClientHandshakeParameters, ClientHelloSent, DataFlags, DataReceiver, DataSender,
    EphemeralPrivateKey, EstablishedSession, HandshakeContext, MAX_DATAGRAM_LENGTH,
    MAX_ENCRYPTED_PAYLOAD_LENGTH, PacketType, SERVER_FINISH_TYPE, SERVER_HELLO_TYPE,
    ServerHandshakeParameters, ServerHelloSent, key_update_payload, verify_key_update_payload,
};

use crate::{
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
    endpoint: SocketAddr,
    state: PeerState,
    queued_packets: VecDeque<Vec<u8>>,
    queued_bytes: usize,
    recent_client_hellos: VecDeque<([u8; 32], Instant)>,
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
}

impl ProcessResult {
    fn empty() -> Self {
        Self {
            outbound: Vec::new(),
            plaintext: None,
        }
    }
}

/// Owns the authenticated XSP/1 UDP sessions for one Agent.
pub struct UdpDataPlane {
    socket: UdpSocket,
    material: LocalMaterial,
    peers_by_virtual_ip: HashMap<Ipv4Addr, Peer>,
    peer_by_endpoint: HashMap<SocketAddr, Ipv4Addr>,
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
            identity,
        };
        let mut local_endpoint = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0));
        let mut peers_by_virtual_ip = HashMap::new();
        let mut peer_by_endpoint = HashMap::new();

        for node in &state.configuration_payload.nodes {
            let node_id = decode_fixed::<16>(&node.node_id_base64)?;
            let virtual_ip = node
                .virtual_ip
                .parse::<Ipv4Addr>()
                .map_err(|_| AgentError::ControllerTrust)?;
            let endpoints = node
                .direct_endpoints
                .iter()
                .map(|encoded| {
                    encoded
                        .parse::<SocketAddrV4>()
                        .map(SocketAddr::V4)
                        .map_err(|_| AgentError::ControllerTrust)
                })
                .collect::<Result<Vec<_>>>()?;
            if node_id == material.node_id {
                if let Some(endpoint) = endpoints.first() {
                    local_endpoint = *endpoint;
                }
                continue;
            }
            let Some(endpoint) = endpoints.first().copied() else {
                continue;
            };
            if peers_by_virtual_ip.len() >= MAX_CONFIGURED_PEERS
                || peers_by_virtual_ip.contains_key(&virtual_ip)
            {
                return Err(AgentError::DataPlane);
            }
            for candidate in endpoints {
                if peer_by_endpoint.insert(candidate, virtual_ip).is_some() {
                    return Err(AgentError::DataPlane);
                }
            }
            peers_by_virtual_ip.insert(
                virtual_ip,
                Peer {
                    node_id,
                    virtual_ip,
                    endpoint,
                    state: PeerState::Idle,
                    queued_packets: VecDeque::new(),
                    queued_bytes: 0,
                    recent_client_hellos: VecDeque::new(),
                },
            );
        }

        let socket = UdpSocket::bind(local_endpoint)
            .await
            .map_err(|_| AgentError::Network)?;
        Ok(Self {
            socket,
            material,
            peers_by_virtual_ip,
            peer_by_endpoint,
        })
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
        let endpoint = peer.endpoint;
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
        let Some(peer_ip) = self.peer_by_endpoint.get(&source).copied() else {
            return Ok(None);
        };
        let Some(peer) = self.peers_by_virtual_ip.get_mut(&peer_ip) else {
            return Ok(None);
        };
        let result = process_datagram(&self.material, peer, &datagram[..length], Instant::now());
        for outbound in result.outbound {
            self.socket
                .send_to(&outbound, peer.endpoint)
                .await
                .map_err(|_| AgentError::Network)?;
        }
        Ok(result.plaintext)
    }

    /// Retransmits bounded pending handshakes and expires stale state.
    ///
    /// # Errors
    ///
    /// Returns an Agent error when a UDP retransmission fails.
    pub async fn maintain(&mut self) -> Result<()> {
        let now = Instant::now();
        let mut retransmissions = Vec::new();
        for peer in self.peers_by_virtual_ip.values_mut() {
            prune_client_hello_cache(peer, now);
            if let Some(encoded) = poll_peer_retry(peer, now) {
                retransmissions.push((peer.endpoint, encoded));
            }
            if let PeerState::Established(established) = &mut peer.state
                && let Some(encoded) = maintain_established(established, now)?
            {
                retransmissions.push((peer.endpoint, encoded));
            }
        }
        for (endpoint, encoded) in retransmissions {
            self.socket
                .send_to(&encoded, endpoint)
                .await
                .map_err(|_| AgentError::Network)?;
        }
        Ok(())
    }
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
    datagram: &[u8],
    now: Instant,
) -> ProcessResult {
    match datagram.get(5).copied() {
        Some(CLIENT_HELLO_TYPE) => handle_client_hello(material, peer, datagram, now),
        Some(SERVER_HELLO_TYPE) => handle_server_hello(material, peer, datagram, now),
        Some(CLIENT_FINISH_TYPE) => handle_client_finish(peer, datagram),
        Some(SERVER_FINISH_TYPE) => handle_server_finish(peer, datagram),
        Some(value) if PacketType::try_from(value).is_ok() => handle_data(peer, datagram),
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
    }
}

fn handle_data(peer: &mut Peer, datagram: &[u8]) -> ProcessResult {
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
        },
        PacketType::KeyUpdate => ProcessResult {
            outbound: handle_key_update(established, &opened)
                .into_iter()
                .collect(),
            plaintext: None,
        },
        PacketType::KeyUpdateAck => {
            handle_key_update_ack(established, &opened);
            ProcessResult::empty()
        }
        PacketType::Keepalive
        | PacketType::PathChallenge
        | PacketType::PathResponse
        | PacketType::Close => ProcessResult::empty(),
    }
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

fn poll_peer_retry(peer: &mut Peer, now: Instant) -> Option<Vec<u8>> {
    let state = std::mem::replace(&mut peer.state, PeerState::Idle);
    let (state, action) = match state {
        PeerState::ClientHello(mut pending) => {
            let action = pending.retry.poll(now);
            (PeerState::ClientHello(pending), action)
        }
        PeerState::ClientFinish(mut pending) => {
            let action = pending.retry.poll(now);
            (PeerState::ClientFinish(pending), action)
        }
        PeerState::ServerHello(mut pending) => {
            let action = pending.retry.poll(now);
            (PeerState::ServerHello(pending), action)
        }
        other => {
            peer.state = other;
            return None;
        }
    };
    match action {
        RetryAction::Wait => {
            peer.state = state;
            None
        }
        RetryAction::Send(encoded) => {
            peer.state = state;
            Some(encoded)
        }
        RetryAction::Expired => {
            clear_queue(peer);
            None
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
