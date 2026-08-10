use std::{
    collections::{HashMap, VecDeque},
    io,
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use ed25519_dalek::SigningKey;
use tokio::{
    net::UdpSocket,
    sync::watch,
    time::{MissedTickBehavior, interval},
};
use xs_protocol::{
    RELAY_DATA_TYPE, RELAY_KEEPALIVE_TYPE, RELAY_MAX_FRAME_LENGTH, RELAY_REGISTER_REQUEST_LENGTH,
    RELAY_REGISTER_REQUEST_TYPE, RelayKeepaliveResponse, RelayRegisterResponse, parse_relay_frame,
    parse_relay_keepalive, sign_relay_keepalive_response, sign_relay_register_response,
    verify_relay_register_request,
};

use crate::{config::RelayConfig, metrics::RelayMetrics};

const REPLAY_WINDOW_BITS: usize = 1024;
const REPLAY_WINDOW_WORDS: usize = REPLAY_WINDOW_BITS / 64;
const MAX_SEQUENCE_ADVANCE: u64 = 1 << 20;
const RECEIVE_BUFFER_LENGTH: usize = 2048;
const REQUEST_REPLAY_TTL: Duration = Duration::from_secs(300);
const SOURCE_WINDOW: Duration = Duration::from_secs(60);
const GLOBAL_REGISTRATION_WINDOW: Duration = Duration::from_secs(1);
const TRAFFIC_WINDOW: Duration = Duration::from_secs(1);
const CLEANUP_INTERVAL: Duration = Duration::from_secs(1);
const FLUSH_INTERVAL: Duration = Duration::from_millis(1);
const MAX_FLUSH_PER_TICK: usize = 256;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct NodeKey {
    network_id: [u8; 16],
    node_id: [u8; 16],
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct RequestKey {
    node: NodeKey,
    request_id: [u8; 16],
}

struct Lease {
    node: NodeKey,
    endpoint: SocketAddr,
    expires_at: u64,
    last_activity: Instant,
    replay: ReplayWindow,
    traffic: TrafficBudget,
    queue: VecDeque<QueuedDatagram>,
    queued_bytes: usize,
    queue_scheduled: bool,
}

struct QueuedDatagram {
    bytes: Vec<u8>,
    enqueued_at: Instant,
}

struct CachedRegistration {
    endpoint: SocketAddr,
    expires: Instant,
    response: [u8; 168],
}

struct SourceBudget {
    window_started: Instant,
    requests: u16,
}

struct RegistrationBudget {
    window_started: Instant,
    requests: u32,
}

struct TrafficBudget {
    window_started: Instant,
    packets: u32,
    bytes: u64,
}

#[derive(Clone)]
struct ReplayWindow {
    highest: Option<u64>,
    words: [u64; REPLAY_WINDOW_WORDS],
}

pub struct RelayServer {
    relay_id: [u8; 16],
    controller_credential_key: ed25519_dalek::VerifyingKey,
    identity_key: SigningKey,
    lease_ttl_seconds: u64,
    idle_timeout: Duration,
    max_leases: usize,
    registration_requests_per_minute: u16,
    registration_requests_global_per_second: u32,
    packets_per_lease_per_second: u32,
    bytes_per_lease_per_second: u64,
    queue_packets_per_node: usize,
    queue_bytes_per_node: usize,
    queue_packets_global: usize,
    queue_bytes_global: usize,
    queued_packets: usize,
    queued_bytes: usize,
    nodes: HashMap<NodeKey, [u8; 16]>,
    leases: HashMap<[u8; 16], Lease>,
    active_queues: VecDeque<[u8; 16]>,
    registrations: HashMap<RequestKey, CachedRegistration>,
    registration_order: VecDeque<RequestKey>,
    source_budgets: HashMap<IpAddr, SourceBudget>,
    source_order: VecDeque<IpAddr>,
    registration_budget: RegistrationBudget,
    metrics: RelayMetrics,
}

impl RelayServer {
    #[must_use]
    pub fn new(config: RelayConfig, metrics: RelayMetrics) -> Self {
        Self {
            relay_id: config.relay_id,
            controller_credential_key: config.controller_credential_key,
            identity_key: config.identity_key,
            lease_ttl_seconds: config.lease_ttl_seconds,
            idle_timeout: Duration::from_secs(config.idle_timeout_seconds),
            max_leases: config.max_leases,
            registration_requests_per_minute: config.registration_requests_per_minute,
            registration_requests_global_per_second: config.registration_requests_global_per_second,
            packets_per_lease_per_second: config.packets_per_lease_per_second,
            bytes_per_lease_per_second: config.bytes_per_lease_per_second,
            queue_packets_per_node: config.queue_packets_per_node,
            queue_bytes_per_node: config.queue_bytes_per_node,
            queue_packets_global: config.queue_packets_global,
            queue_bytes_global: config.queue_bytes_global,
            queued_packets: 0,
            queued_bytes: 0,
            nodes: HashMap::new(),
            leases: HashMap::new(),
            active_queues: VecDeque::new(),
            registrations: HashMap::new(),
            registration_order: VecDeque::new(),
            source_budgets: HashMap::new(),
            source_order: VecDeque::new(),
            registration_budget: RegistrationBudget::new(Instant::now()),
            metrics,
        }
    }

    /// Serves authenticated XSR/1 UDP traffic until shutdown.
    ///
    /// Invalid and unauthenticated datagrams are silently dropped.
    ///
    /// # Errors
    ///
    /// Returns an I/O error only when the bound UDP socket receive operation fails.
    pub async fn serve(
        mut self,
        socket: Arc<UdpSocket>,
        mut shutdown: watch::Receiver<bool>,
    ) -> io::Result<()> {
        let mut datagram = [0_u8; RECEIVE_BUFFER_LENGTH];
        let mut cleanup = interval(CLEANUP_INTERVAL);
        cleanup.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut flush = interval(FLUSH_INTERVAL);
        flush.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        return Ok(());
                    }
                }
                _ = cleanup.tick() => {
                    self.cleanup(Instant::now(), unix_time());
                }
                _ = flush.tick() => {
                    self.flush_queues(&socket);
                }
                received = socket.recv_from(&mut datagram) => {
                    let (length, source) = received?;
                    self.metrics.received(length);
                    self.handle_datagram(&socket, &datagram[..length], source);
                    self.flush_queues(&socket);
                }
            }
        }
    }

    fn handle_datagram(&mut self, socket: &UdpSocket, datagram: &[u8], source: SocketAddr) {
        if datagram.len() < 6
            || datagram.len() > RELAY_MAX_FRAME_LENGTH
            || &datagram[0..4] != b"XSR1"
        {
            self.metrics.invalid_drop();
            return;
        }
        match datagram[5] {
            RELAY_REGISTER_REQUEST_TYPE => self.handle_registration(socket, datagram, source),
            RELAY_DATA_TYPE => self.handle_data(datagram, source),
            RELAY_KEEPALIVE_TYPE => self.handle_keepalive(socket, datagram, source),
            _ => self.metrics.invalid_drop(),
        }
    }

    fn handle_registration(&mut self, socket: &UdpSocket, datagram: &[u8], source: SocketAddr) {
        let instant = Instant::now();
        if datagram.len() != RELAY_REGISTER_REQUEST_LENGTH
            || !self.allow_registration(source.ip(), instant)
        {
            self.metrics.registration_rejected();
            return;
        }
        let now = unix_time();
        let Ok(verified) =
            verify_relay_register_request(datagram, &self.controller_credential_key, now)
        else {
            self.metrics.registration_rejected();
            return;
        };
        if verified.request.relay_id != self.relay_id {
            self.metrics.registration_rejected();
            return;
        }
        let node = NodeKey {
            network_id: verified.request.network_id,
            node_id: verified.request.node_id,
        };
        let request_key = RequestKey {
            node,
            request_id: verified.request.request_id,
        };
        if let Some(cached) = self.registrations.get(&request_key) {
            if cached.endpoint == source && cached.expires > instant {
                let _ = socket.try_send_to(&cached.response, source);
                self.metrics.registration_retry();
            } else {
                self.metrics.registration_rejected();
            }
            return;
        }
        if self.leases.len() >= self.max_leases && !self.nodes.contains_key(&node) {
            self.metrics.registration_rejected();
            return;
        }
        let Some(lease_id) = self.new_lease_id() else {
            self.metrics.registration_rejected();
            return;
        };
        let expires_at = now
            .saturating_add(self.lease_ttl_seconds)
            .min(verified.credential.not_after);
        if expires_at <= now {
            self.metrics.registration_rejected();
            return;
        }
        if let Some(previous) = self.nodes.insert(node, lease_id) {
            self.remove_lease(previous);
        }
        self.leases.insert(
            lease_id,
            Lease {
                node,
                endpoint: source,
                expires_at,
                last_activity: instant,
                replay: ReplayWindow::new(),
                traffic: TrafficBudget::new(instant),
                queue: VecDeque::new(),
                queued_bytes: 0,
                queue_scheduled: false,
            },
        );
        let response = sign_relay_register_response(
            RelayRegisterResponse {
                network_id: node.network_id,
                node_id: node.node_id,
                relay_id: self.relay_id,
                request_id: request_key.request_id,
                lease_id,
                expires_at,
            },
            &self.identity_key,
        );
        self.cache_registration(request_key, source, response, instant);
        self.metrics.set_active_leases(self.leases.len());
        self.metrics.registration_accepted();
        let _ = socket.try_send_to(&response, source);
    }

    fn handle_data(&mut self, datagram: &[u8], source: SocketAddr) {
        let Ok(frame) = parse_relay_frame(datagram) else {
            self.metrics.invalid_drop();
            return;
        };
        if frame.relay_id != self.relay_id {
            self.metrics.authentication_drop();
            return;
        }
        let now = Instant::now();
        let unix_now = unix_time();
        let Some(source_lease) = self.leases.get(&frame.lease_id) else {
            self.metrics.authentication_drop();
            return;
        };
        if !lease_matches(
            source_lease,
            frame.network_id,
            frame.source_node_id,
            source,
            unix_now,
        ) {
            self.metrics.authentication_drop();
            return;
        }
        if source_lease.replay.precheck(frame.sequence).is_err() {
            self.metrics.replay_drop();
            return;
        }
        if !source_lease.traffic.can_accept(
            now,
            datagram.len(),
            self.packets_per_lease_per_second,
            self.bytes_per_lease_per_second,
        ) {
            self.metrics.rate_limit_drop();
            return;
        }
        let destination = NodeKey {
            network_id: frame.network_id,
            node_id: frame.destination_node_id,
        };
        let Some(destination_lease_id) = self.nodes.get(&destination).copied() else {
            self.metrics.destination_drop();
            return;
        };
        let Some(destination_lease) = self.leases.get(&destination_lease_id) else {
            self.metrics.destination_drop();
            return;
        };
        if destination_lease.expires_at <= unix_now
            || !self.queue_has_capacity(destination_lease, datagram.len())
        {
            self.metrics.queue_drop();
            return;
        }
        let Some(source_lease) = self.leases.get_mut(&frame.lease_id) else {
            self.metrics.authentication_drop();
            return;
        };
        if source_lease.replay.commit(frame.sequence).is_err() {
            self.metrics.replay_drop();
            return;
        }
        source_lease.traffic.commit(now, datagram.len());
        source_lease.last_activity = now;
        let destination_lease = self
            .leases
            .get_mut(&destination_lease_id)
            .expect("destination lease was prechecked");
        destination_lease.queued_bytes = destination_lease
            .queued_bytes
            .saturating_add(datagram.len());
        destination_lease.queue.push_back(QueuedDatagram {
            bytes: datagram.to_vec(),
            enqueued_at: now,
        });
        if !destination_lease.queue_scheduled {
            destination_lease.queue_scheduled = true;
            self.active_queues.push_back(destination_lease_id);
        }
        self.queued_packets = self.queued_packets.saturating_add(1);
        self.queued_bytes = self.queued_bytes.saturating_add(datagram.len());
        self.metrics
            .set_queue_depth(self.queued_packets, self.queued_bytes);
    }

    fn handle_keepalive(&mut self, socket: &UdpSocket, datagram: &[u8], source: SocketAddr) {
        let Ok(frame) = parse_relay_keepalive(datagram) else {
            self.metrics.invalid_drop();
            return;
        };
        if frame.relay_id != self.relay_id {
            self.metrics.authentication_drop();
            return;
        }
        let now = Instant::now();
        let unix_now = unix_time();
        let Some(lease) = self.leases.get_mut(&frame.lease_id) else {
            self.metrics.authentication_drop();
            return;
        };
        if !lease_matches(
            lease,
            frame.network_id,
            frame.source_node_id,
            source,
            unix_now,
        ) {
            self.metrics.authentication_drop();
            return;
        }
        if lease.replay.precheck(frame.sequence).is_err() {
            self.metrics.replay_drop();
            return;
        }
        if !lease.traffic.can_accept(
            now,
            datagram.len(),
            self.packets_per_lease_per_second,
            self.bytes_per_lease_per_second,
        ) {
            self.metrics.rate_limit_drop();
            return;
        }
        if lease.replay.commit(frame.sequence).is_err() {
            self.metrics.replay_drop();
            return;
        }
        lease.traffic.commit(now, datagram.len());
        lease.last_activity = now;
        let response = sign_relay_keepalive_response(
            RelayKeepaliveResponse {
                network_id: frame.network_id,
                relay_id: self.relay_id,
                node_id: frame.source_node_id,
                lease_id: frame.lease_id,
                server_time: unix_now,
            },
            &self.identity_key,
        );
        if socket.try_send_to(&response, source).is_err() {
            self.metrics.send_drop();
        } else {
            self.metrics.keepalive_accepted();
        }
    }

    fn flush_queues(&mut self, socket: &UdpSocket) {
        for _ in 0..MAX_FLUSH_PER_TICK {
            let Some(lease_id) = self.active_queues.pop_front() else {
                return;
            };
            let Some(lease) = self.leases.get(&lease_id) else {
                continue;
            };
            let Some(datagram) = lease.queue.front() else {
                self.leases
                    .get_mut(&lease_id)
                    .expect("active lease exists")
                    .queue_scheduled = false;
                continue;
            };
            match socket.try_send_to(&datagram.bytes, lease.endpoint) {
                Ok(length) => {
                    let datagram = self
                        .dequeue_datagram(lease_id)
                        .expect("queue front was prechecked");
                    if length == datagram.bytes.len() {
                        self.metrics
                            .forwarded(length, datagram.enqueued_at.elapsed());
                    } else {
                        self.metrics.send_drop();
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    self.active_queues.push_front(lease_id);
                    return;
                }
                Err(_) => {
                    self.dequeue_datagram(lease_id);
                    self.metrics.send_drop();
                }
            }
            let Some(lease) = self.leases.get_mut(&lease_id) else {
                continue;
            };
            if lease.queue.is_empty() {
                lease.queue_scheduled = false;
            } else {
                self.active_queues.push_back(lease_id);
            }
        }
    }

    fn cleanup(&mut self, now: Instant, unix_now: u64) {
        let expired = self
            .leases
            .iter()
            .filter_map(|(lease_id, lease)| {
                (lease.expires_at <= unix_now
                    || now.saturating_duration_since(lease.last_activity) >= self.idle_timeout)
                    .then_some((*lease_id, lease.node))
            })
            .collect::<Vec<_>>();
        for (lease_id, node) in expired {
            self.remove_lease(lease_id);
            if self.nodes.get(&node) == Some(&lease_id) {
                self.nodes.remove(&node);
            }
        }
        while let Some(request) = self.registration_order.front().copied() {
            if self
                .registrations
                .get(&request)
                .is_some_and(|cached| cached.expires > now)
            {
                break;
            }
            self.registration_order.pop_front();
            self.registrations.remove(&request);
        }
        self.metrics.set_active_leases(self.leases.len());
    }

    fn new_lease_id(&self) -> Option<[u8; 16]> {
        for _ in 0..8 {
            let mut lease_id = [0_u8; 16];
            if getrandom::fill(&mut lease_id).is_ok()
                && lease_id != [0_u8; 16]
                && !self.leases.contains_key(&lease_id)
            {
                return Some(lease_id);
            }
        }
        None
    }

    fn cache_registration(
        &mut self,
        request: RequestKey,
        endpoint: SocketAddr,
        response: [u8; 168],
        now: Instant,
    ) {
        let maximum = self.max_leases.saturating_mul(4).max(16);
        while self.registrations.len() >= maximum {
            let Some(oldest) = self.registration_order.pop_front() else {
                break;
            };
            self.registrations.remove(&oldest);
        }
        self.registration_order.push_back(request);
        self.registrations.insert(
            request,
            CachedRegistration {
                endpoint,
                expires: now + REQUEST_REPLAY_TTL,
                response,
            },
        );
    }

    fn queue_has_capacity(&self, lease: &Lease, datagram_length: usize) -> bool {
        lease.queue.len() < self.queue_packets_per_node
            && lease.queued_bytes.saturating_add(datagram_length) <= self.queue_bytes_per_node
            && self.queued_packets < self.queue_packets_global
            && self.queued_bytes.saturating_add(datagram_length) <= self.queue_bytes_global
    }

    fn dequeue_datagram(&mut self, lease_id: [u8; 16]) -> Option<QueuedDatagram> {
        let lease = self.leases.get_mut(&lease_id)?;
        let datagram = lease.queue.pop_front()?;
        lease.queued_bytes = lease.queued_bytes.saturating_sub(datagram.bytes.len());
        self.queued_packets = self.queued_packets.saturating_sub(1);
        self.queued_bytes = self.queued_bytes.saturating_sub(datagram.bytes.len());
        self.metrics
            .set_queue_depth(self.queued_packets, self.queued_bytes);
        Some(datagram)
    }

    fn remove_lease(&mut self, lease_id: [u8; 16]) {
        let Some(lease) = self.leases.remove(&lease_id) else {
            return;
        };
        self.queued_packets = self.queued_packets.saturating_sub(lease.queue.len());
        self.queued_bytes = self.queued_bytes.saturating_sub(lease.queued_bytes);
        self.active_queues
            .retain(|scheduled| *scheduled != lease_id);
        self.metrics.set_active_leases(self.leases.len());
        self.metrics
            .set_queue_depth(self.queued_packets, self.queued_bytes);
    }

    fn allow_registration(&mut self, source: IpAddr, now: Instant) -> bool {
        self.registration_budget
            .allow(now, self.registration_requests_global_per_second)
            && self.allow_registration_source(source, now)
    }

    fn allow_registration_source(&mut self, source: IpAddr, now: Instant) -> bool {
        if let Some(budget) = self.source_budgets.get_mut(&source) {
            if now.saturating_duration_since(budget.window_started) >= SOURCE_WINDOW {
                budget.window_started = now;
                budget.requests = 1;
                return true;
            }
            if budget.requests >= self.registration_requests_per_minute {
                return false;
            }
            budget.requests = budget.requests.saturating_add(1);
            return true;
        }
        let maximum = self.max_leases.clamp(64, 4096);
        while self.source_budgets.len() >= maximum {
            let Some(oldest) = self.source_order.pop_front() else {
                break;
            };
            self.source_budgets.remove(&oldest);
        }
        self.source_order.push_back(source);
        self.source_budgets.insert(
            source,
            SourceBudget {
                window_started: now,
                requests: 1,
            },
        );
        true
    }
}

impl RegistrationBudget {
    fn new(now: Instant) -> Self {
        Self {
            window_started: now,
            requests: 0,
        }
    }

    fn allow(&mut self, now: Instant, maximum: u32) -> bool {
        if now.saturating_duration_since(self.window_started) >= GLOBAL_REGISTRATION_WINDOW {
            self.window_started = now;
            self.requests = 0;
        }
        if self.requests >= maximum {
            return false;
        }
        self.requests = self.requests.saturating_add(1);
        true
    }
}

fn lease_matches(
    lease: &Lease,
    network_id: [u8; 16],
    node_id: [u8; 16],
    endpoint: SocketAddr,
    now: u64,
) -> bool {
    lease.node.network_id == network_id
        && lease.node.node_id == node_id
        && lease.endpoint == endpoint
        && lease.expires_at > now
}

impl TrafficBudget {
    const fn new(now: Instant) -> Self {
        Self {
            window_started: now,
            packets: 0,
            bytes: 0,
        }
    }

    fn can_accept(&self, now: Instant, bytes: usize, packet_limit: u32, byte_limit: u64) -> bool {
        if now.saturating_duration_since(self.window_started) >= TRAFFIC_WINDOW {
            return u64::try_from(bytes).is_ok_and(|bytes| bytes <= byte_limit);
        }
        self.packets < packet_limit
            && u64::try_from(bytes)
                .is_ok_and(|bytes| self.bytes.saturating_add(bytes) <= byte_limit)
    }

    fn commit(&mut self, now: Instant, bytes: usize) {
        if now.saturating_duration_since(self.window_started) >= TRAFFIC_WINDOW {
            self.window_started = now;
            self.packets = 0;
            self.bytes = 0;
        }
        self.packets = self.packets.saturating_add(1);
        self.bytes = self
            .bytes
            .saturating_add(u64::try_from(bytes).unwrap_or(u64::MAX));
    }
}

impl ReplayWindow {
    const fn new() -> Self {
        Self {
            highest: None,
            words: [0_u64; REPLAY_WINDOW_WORDS],
        }
    }

    fn precheck(&self, sequence: u64) -> Result<(), ()> {
        let Some(highest) = self.highest else {
            return (sequence <= MAX_SEQUENCE_ADVANCE).then_some(()).ok_or(());
        };
        if sequence > highest {
            return (sequence - highest <= MAX_SEQUENCE_ADVANCE)
                .then_some(())
                .ok_or(());
        }
        let offset = usize::try_from(highest - sequence).map_err(|_| ())?;
        if offset >= REPLAY_WINDOW_BITS || self.bit_is_set(offset) {
            Err(())
        } else {
            Ok(())
        }
    }

    fn commit(&mut self, sequence: u64) -> Result<(), ()> {
        self.precheck(sequence)?;
        match self.highest {
            None => {
                self.highest = Some(sequence);
                self.words[0] = 1;
            }
            Some(highest) if sequence > highest => {
                let distance = usize::try_from(sequence - highest).map_err(|_| ())?;
                self.shift(distance);
                self.highest = Some(sequence);
                self.words[0] |= 1;
            }
            Some(highest) => {
                let offset = usize::try_from(highest - sequence).map_err(|_| ())?;
                self.words[offset / 64] |= 1_u64 << (offset % 64);
            }
        }
        Ok(())
    }

    fn bit_is_set(&self, offset: usize) -> bool {
        self.words[offset / 64] & (1_u64 << (offset % 64)) != 0
    }

    fn shift(&mut self, distance: usize) {
        if distance >= REPLAY_WINDOW_BITS {
            self.words.fill(0);
            return;
        }
        let old = self.words;
        self.words.fill(0);
        let word_shift = distance / 64;
        let bit_shift = distance % 64;
        for (source, word) in old.into_iter().enumerate() {
            let target = source + word_shift;
            if target >= REPLAY_WINDOW_WORDS {
                break;
            }
            self.words[target] |= word << bit_shift;
            if bit_shift != 0 && target + 1 < REPLAY_WINDOW_WORDS {
                self.words[target + 1] |= word >> (64 - bit_shift);
            }
        }
    }
}

fn unix_time() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        net::{Ipv4Addr, SocketAddrV4},
    };

    use ed25519_dalek::VerifyingKey;
    use tokio::time::{sleep, timeout};
    use xs_protocol::{
        CREDENTIAL_LENGTH, CredentialClaims, DATA_HEADER_LENGTH, DATA_TAG_LENGTH, DataFlags,
        DataHeader, PacketType, RELAY_KEEPALIVE_RESPONSE_LENGTH, RELAY_REGISTER_REQUEST_LENGTH,
        RELAY_REGISTER_RESPONSE_LENGTH, RelayFrame, RelayRegisterRequest, encode_relay_frame,
        encode_relay_keepalive, node_id, role_set_digest, sign_credential,
        sign_relay_register_request, verify_relay_keepalive_response,
        verify_relay_register_response,
    };

    use super::*;

    struct TestContext {
        relay_endpoint: SocketAddr,
        network_id: [u8; 16],
        relay_id: [u8; 16],
        controller_key: SigningKey,
        relay_key: SigningKey,
    }

    struct RegisteredNode {
        socket: UdpSocket,
        node_id: [u8; 16],
        lease_id: [u8; 16],
        identity_key: SigningKey,
        identity_seed: u8,
    }

    struct TestRelay {
        context: TestContext,
        metrics: RelayMetrics,
        shutdown: watch::Sender<bool>,
        task: tokio::task::JoinHandle<io::Result<()>>,
    }

    #[derive(Clone, Copy)]
    enum GlobalQueueConstraint {
        Packets,
        Bytes,
    }

    struct GlobalQueueScenario {
        server: RelayServer,
        metrics: RelayMetrics,
        first_frame: Vec<u8>,
        second_frame: Vec<u8>,
        first_source_endpoint: SocketAddr,
        second_source_endpoint: SocketAddr,
        second_source_lease: [u8; 16],
        first_destination_lease: [u8; 16],
        now: Instant,
    }

    #[test]
    fn replay_window_enforces_duplicate_old_and_jump_bounds() {
        let mut window = ReplayWindow::new();
        window.commit(0).expect("initial sequence");
        window.commit(2).expect("future sequence");
        window.commit(1).expect("reordered sequence");
        assert!(window.commit(1).is_err());
        assert!(window.commit(MAX_SEQUENCE_ADVANCE + 3).is_err());

        let mut boundary = ReplayWindow::new();
        boundary.commit(0).expect("initial sequence");
        boundary.commit(1023).expect("window edge");
        assert!(boundary.commit(0).is_err());
    }

    #[test]
    fn global_registration_budget_blocks_source_spray_before_source_state_growth() {
        let controller_key = SigningKey::from_bytes(&[71_u8; 32]);
        let relay_key = SigningKey::from_bytes(&[72_u8; 32]);
        let mut config = test_config([73_u8; 16], controller_key.verifying_key(), relay_key);
        config.registration_requests_global_per_second = 2;
        let mut server = RelayServer::new(config, RelayMetrics::default());
        let now = Instant::now();
        server.registration_budget = RegistrationBudget::new(now);
        let first = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1));
        let second = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 2));
        let blocked = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 3));

        assert!(server.allow_registration(first, now));
        assert!(server.allow_registration(second, now));
        assert!(!server.allow_registration(blocked, now));
        assert_eq!(server.source_budgets.len(), 2);
        assert!(!server.source_budgets.contains_key(&blocked));

        assert!(server.allow_registration(blocked, now + GLOBAL_REGISTRATION_WINDOW));
        assert_eq!(server.source_budgets.len(), 3);
    }

    #[test]
    fn global_packet_queue_limit_is_shared_and_releases_on_cleanup() {
        assert_global_queue_limit(GlobalQueueConstraint::Packets);
    }

    #[test]
    fn global_byte_queue_limit_is_shared_and_releases_on_cleanup() {
        assert_global_queue_limit(GlobalQueueConstraint::Bytes);
    }

    #[tokio::test]
    async fn udp_relay_authenticates_forwards_rejects_replay_and_answers_keepalive() {
        let relay = start_test_relay(100).await;
        let first = register(&relay.context, 31, [51_u8; 16]).await;
        let second = register(&relay.context, 32, [52_u8; 16]).await;
        assert_ne!(first.lease_id, second.lease_id);
        let inner = assert_forwarding_and_replay(&relay.context, &first, &second).await;
        assert_keepalive(&relay.context, &first).await;
        assert_endpoint_spoof_is_rejected(&relay.context, &first, &second, &inner).await;

        sleep(Duration::from_millis(20)).await;
        let snapshot = relay.metrics.snapshot();
        assert_eq!(snapshot.active_leases, 2);
        assert!(snapshot.packets_received >= 7);
        assert!(snapshot.bytes_received > 0);
        assert_eq!(snapshot.packets_forwarded, 1);
        assert!(snapshot.bytes_forwarded > 0);
        assert_eq!(snapshot.forwarding_latency_samples, 1);
        assert!(snapshot.forwarding_latency_microseconds_average.is_some());
        assert_eq!(snapshot.keepalives_accepted, 1);
        assert!(snapshot.replay_drops >= 1);
        assert!(snapshot.authentication_drops >= 1);
        assert!(snapshot.packets_dropped >= 2);
        assert_eq!(snapshot.registration_retries, 2);
        relay.shutdown().await;
    }

    #[tokio::test]
    async fn udp_relay_enforces_and_recovers_per_lease_packet_rate() {
        let relay = start_test_relay(1).await;
        let first = register(&relay.context, 33, [53_u8; 16]).await;
        let second = register(&relay.context, 34, [54_u8; 16]).await;
        let inner = framed_xsp_keepalive(relay.context.network_id, first.node_id, second.node_id);
        let first_frame = relay_frame(&relay.context, &first, &second, 1, &inner);
        let second_frame = relay_frame(&relay.context, &first, &second, 2, &inner);
        let mut received = [0_u8; RELAY_MAX_FRAME_LENGTH];

        first
            .socket
            .send_to(&first_frame, relay.context.relay_endpoint)
            .await
            .expect("send first frame");
        timeout(
            Duration::from_secs(1),
            second.socket.recv_from(&mut received),
        )
        .await
        .expect("first forward timeout")
        .expect("first frame forwarded");
        first
            .socket
            .send_to(&second_frame, relay.context.relay_endpoint)
            .await
            .expect("send rate-limited frame");
        assert!(
            timeout(
                Duration::from_millis(100),
                second.socket.recv_from(&mut received)
            )
            .await
            .is_err()
        );

        sleep(TRAFFIC_WINDOW + Duration::from_millis(50)).await;
        first
            .socket
            .send_to(&second_frame, relay.context.relay_endpoint)
            .await
            .expect("retry after rate window");
        timeout(
            Duration::from_secs(1),
            second.socket.recv_from(&mut received),
        )
        .await
        .expect("rate recovery timeout")
        .expect("frame forwarded after reset");
        assert!(relay.metrics.snapshot().rate_limit_drops >= 1);
        relay.shutdown().await;
    }

    #[tokio::test]
    async fn udp_relay_throughput_baseline() {
        const RENEWAL_INTERVAL_PACKETS: u32 = 400_000;

        let packets = throughput_packet_count();
        let relay = start_test_relay(1_000_000).await;
        let mut first = register(&relay.context, 35, [55_u8; 16]).await;
        let mut second = register(&relay.context, 36, [56_u8; 16]).await;
        let inner = framed_xsp_keepalive(relay.context.network_id, first.node_id, second.node_id);
        let frame_bytes =
            u32::try_from(relay_frame(&relay.context, &first, &second, 1, &inner).len())
                .expect("frame length fits u32");
        let started = Instant::now();
        let mut received = [0_u8; RELAY_MAX_FRAME_LENGTH];
        let mut completed_packets = 0_u32;
        let mut lease_sequence = 1_u64;
        let mut renewal = 0_u32;
        while completed_packets < packets {
            if completed_packets > 0 && completed_packets.is_multiple_of(RENEWAL_INTERVAL_PACKETS) {
                renewal = renewal.saturating_add(1);
                renew_registration(
                    &relay.context,
                    &mut first,
                    throughput_renewal_request_id(57, renewal),
                )
                .await;
                renew_registration(
                    &relay.context,
                    &mut second,
                    throughput_renewal_request_id(58, renewal),
                )
                .await;
                lease_sequence = 1;
            }
            let packets_until_renewal = RENEWAL_INTERVAL_PACKETS
                .saturating_sub(completed_packets % RENEWAL_INTERVAL_PACKETS);
            let window_packets = packets
                .saturating_sub(completed_packets)
                .min(packets_until_renewal)
                .min(64);
            let frames = (0..window_packets)
                .map(|offset| {
                    relay_frame(
                        &relay.context,
                        &first,
                        &second,
                        lease_sequence.saturating_add(u64::from(offset)),
                        &inner,
                    )
                })
                .collect::<Vec<_>>();
            for frame in &frames {
                first
                    .socket
                    .send_to(frame, relay.context.relay_endpoint)
                    .await
                    .expect("send throughput frame");
            }
            for _ in &frames {
                timeout(
                    Duration::from_secs(10),
                    second.socket.recv_from(&mut received),
                )
                .await
                .expect("throughput receive timeout")
                .expect("receive throughput frame");
            }
            completed_packets = completed_packets.saturating_add(window_packets);
            lease_sequence = lease_sequence.saturating_add(u64::from(window_packets));
        }
        let elapsed = started.elapsed();
        let snapshot = relay.metrics.snapshot();
        assert_eq!(snapshot.packets_forwarded, u64::from(packets));
        assert_eq!(snapshot.packets_dropped, 0);
        assert_eq!(snapshot.queued_packets, 0);
        assert_eq!(snapshot.queued_bytes, 0);
        let total_mib = f64::from(packets) * f64::from(frame_bytes) / 1_048_576.0;
        let report = serde_json::json!({
            "packets": packets,
            "frame_bytes": frame_bytes,
            "elapsed_ms": elapsed.as_secs_f64() * 1000.0,
            "packets_per_second": f64::from(packets) / elapsed.as_secs_f64(),
            "mib_per_second": total_mib / elapsed.as_secs_f64(),
            "forwarding_latency_microseconds_average": snapshot.forwarding_latency_microseconds_average,
            "forwarding_latency_microseconds_max": snapshot.forwarding_latency_microseconds_max,
        });
        if let Some(path) = std::env::var_os("XS_RELAY_THROUGHPUT_REPORT") {
            fs::write(
                path,
                serde_json::to_vec_pretty(&report).expect("serialize Relay throughput report"),
            )
            .expect("write Relay throughput report");
        }
        println!(
            "{}",
            serde_json::to_string(&report).expect("Relay throughput JSON")
        );
        relay.shutdown().await;
    }

    fn throughput_packet_count() -> u32 {
        const DEFAULT_PACKETS: u32 = 10_000;
        const MAXIMUM_PACKETS: u32 = 5_000_000;
        let packets = match std::env::var("XS_RELAY_THROUGHPUT_PACKETS") {
            Ok(value) => value.parse::<u32>().expect("valid throughput packet count"),
            Err(std::env::VarError::NotPresent) => DEFAULT_PACKETS,
            Err(std::env::VarError::NotUnicode(_)) => panic!("throughput packet count is UTF-8"),
        };
        assert!((DEFAULT_PACKETS..=MAXIMUM_PACKETS).contains(&packets));
        packets
    }

    fn throughput_renewal_request_id(marker: u8, renewal: u32) -> [u8; 16] {
        let mut request_id = [marker; 16];
        request_id[..4].copy_from_slice(&renewal.to_be_bytes());
        request_id
    }

    fn test_config(
        relay_id: [u8; 16],
        controller_credential_key: VerifyingKey,
        identity_key: SigningKey,
    ) -> RelayConfig {
        RelayConfig {
            listen: SocketAddr::from(([127, 0, 0, 1], 1)),
            health_listen: SocketAddr::from(([127, 0, 0, 1], 2)),
            relay_id,
            controller_credential_key,
            identity_key,
            controller_metrics_url: "http://127.0.0.1/v1/relay-metrics"
                .parse()
                .expect("Controller metrics URL"),
            metrics_report_interval_seconds: 10,
            lease_ttl_seconds: 120,
            idle_timeout_seconds: 60,
            max_leases: 8,
            registration_requests_per_minute: 30,
            registration_requests_global_per_second: 512,
            packets_per_lease_per_second: 100,
            bytes_per_lease_per_second: 1_000_000,
            queue_packets_per_node: 8,
            queue_bytes_per_node: 16_000,
            queue_packets_global: 64,
            queue_bytes_global: 128_000,
        }
    }

    fn assert_global_queue_limit(constraint: GlobalQueueConstraint) {
        let mut scenario = global_queue_scenario(constraint);
        scenario
            .server
            .handle_data(&scenario.first_frame, scenario.first_source_endpoint);
        scenario
            .server
            .handle_data(&scenario.second_frame, scenario.second_source_endpoint);
        assert_eq!(scenario.server.queued_packets, 1);
        assert_eq!(scenario.server.queued_bytes, scenario.first_frame.len());
        assert_eq!(scenario.metrics.snapshot().queue_drops, 1);
        assert_eq!(
            lease_replay_and_packets(&scenario.server, scenario.second_source_lease),
            (None, 0)
        );

        scenario
            .server
            .leases
            .get_mut(&scenario.first_destination_lease)
            .expect("first destination lease")
            .expires_at = 0;
        scenario.server.cleanup(scenario.now, unix_time());
        assert_eq!(scenario.server.queued_packets, 0);
        assert_eq!(scenario.server.queued_bytes, 0);
        let snapshot = scenario.metrics.snapshot();
        assert_eq!(snapshot.queued_packets, 0);
        assert_eq!(snapshot.queued_bytes, 0);

        scenario
            .server
            .handle_data(&scenario.second_frame, scenario.second_source_endpoint);
        assert_eq!(scenario.server.queued_packets, 1);
        assert_eq!(scenario.server.queued_bytes, scenario.second_frame.len());
        assert_eq!(
            lease_replay_and_packets(&scenario.server, scenario.second_source_lease),
            (Some(1), 1)
        );
    }

    fn global_queue_scenario(constraint: GlobalQueueConstraint) -> GlobalQueueScenario {
        let controller_key = SigningKey::from_bytes(&[81_u8; 32]);
        let relay_key = SigningKey::from_bytes(&[82_u8; 32]);
        let relay_id = [83_u8; 16];
        let network_id = [84_u8; 16];
        let nodes = [85_u8, 86, 87, 88].map(|value| NodeKey {
            network_id,
            node_id: [value; 16],
        });
        let leases = [91_u8, 92, 93, 94].map(|value| [value; 16]);
        let endpoints = [46_001_u16, 46_002, 46_003, 46_004]
            .map(|port| SocketAddr::from(([127, 0, 0, 1], port)));
        let first = test_relay_frame(network_id, relay_id, nodes[0], nodes[2], leases[0]);
        let second = test_relay_frame(network_id, relay_id, nodes[1], nodes[3], leases[1]);
        assert_eq!(first.len(), second.len());

        let mut config = test_config(relay_id, controller_key.verifying_key(), relay_key);
        match constraint {
            GlobalQueueConstraint::Packets => {
                config.queue_packets_per_node = 1;
                config.queue_packets_global = 1;
            }
            GlobalQueueConstraint::Bytes => {
                config.queue_bytes_per_node = first.len();
                config.queue_bytes_global = first.len();
            }
        }
        let metrics = RelayMetrics::default();
        let mut server = RelayServer::new(config, metrics.clone());
        let now = Instant::now();
        for ((node, lease_id), endpoint) in nodes.into_iter().zip(leases).zip(endpoints) {
            insert_test_lease(&mut server, node, lease_id, endpoint, now);
        }
        GlobalQueueScenario {
            server,
            metrics,
            first_frame: first,
            second_frame: second,
            first_source_endpoint: endpoints[0],
            second_source_endpoint: endpoints[1],
            second_source_lease: leases[1],
            first_destination_lease: leases[2],
            now,
        }
    }

    fn lease_replay_and_packets(server: &RelayServer, lease_id: [u8; 16]) -> (Option<u64>, u32) {
        let lease = server.leases.get(&lease_id).expect("source lease");
        (lease.replay.highest, lease.traffic.packets)
    }

    fn insert_test_lease(
        server: &mut RelayServer,
        node: NodeKey,
        lease_id: [u8; 16],
        endpoint: SocketAddr,
        now: Instant,
    ) {
        server.nodes.insert(node, lease_id);
        server.leases.insert(
            lease_id,
            Lease {
                node,
                endpoint,
                expires_at: unix_time() + 600,
                last_activity: now,
                replay: ReplayWindow::new(),
                traffic: TrafficBudget::new(now),
                queue: VecDeque::new(),
                queued_bytes: 0,
                queue_scheduled: false,
            },
        );
        server.metrics.set_active_leases(server.leases.len());
    }

    fn test_relay_frame(
        network_id: [u8; 16],
        relay_id: [u8; 16],
        source: NodeKey,
        destination: NodeKey,
        source_lease: [u8; 16],
    ) -> Vec<u8> {
        let inner = framed_xsp_keepalive(network_id, source.node_id, destination.node_id);
        encode_relay_frame(RelayFrame {
            network_id,
            relay_id,
            source_node_id: source.node_id,
            destination_node_id: destination.node_id,
            lease_id: source_lease,
            sequence: 1,
            payload: &inner,
        })
        .expect("Relay frame")
    }

    async fn start_test_relay(packets_per_second: u32) -> TestRelay {
        let controller_key = SigningKey::from_bytes(&[21_u8; 32]);
        let relay_key = SigningKey::from_bytes(&[22_u8; 32]);
        let relay_id = [23_u8; 16];
        let metrics = RelayMetrics::default();
        let mut config = test_config(relay_id, controller_key.verifying_key(), relay_key.clone());
        config.packets_per_lease_per_second = packets_per_second;
        if packets_per_second > 1_000 {
            config.bytes_per_lease_per_second = 1_000_000_000;
            config.queue_packets_per_node = 16_384;
            config.queue_bytes_per_node = 32_000_000;
            config.queue_packets_global = 32_768;
            config.queue_bytes_global = 64_000_000;
        }
        let server = RelayServer::new(config, metrics.clone());
        let socket = Arc::new(
            UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))
                .await
                .expect("bind Relay"),
        );
        let relay_endpoint = socket.local_addr().expect("Relay endpoint");
        let (shutdown, shutdown_rx) = watch::channel(false);
        let task = tokio::spawn(server.serve(socket, shutdown_rx));
        TestRelay {
            context: TestContext {
                relay_endpoint,
                network_id: [41_u8; 16],
                relay_id,
                controller_key,
                relay_key,
            },
            metrics,
            shutdown,
            task,
        }
    }

    impl TestRelay {
        async fn shutdown(self) {
            self.shutdown.send(true).expect("signal shutdown");
            self.task
                .await
                .expect("Relay task")
                .expect("Relay shutdown cleanly");
        }
    }

    async fn register(
        context: &TestContext,
        identity_seed: u8,
        request_id: [u8; 16],
    ) -> RegisteredNode {
        let socket = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("bind node");
        let identity_key = SigningKey::from_bytes(&[identity_seed; 32]);
        let (request, response, lease_id) =
            registration_exchange(context, &socket, &identity_key, identity_seed, request_id).await;
        socket
            .send_to(&request, context.relay_endpoint)
            .await
            .expect("retry registration");
        let mut retry = [0_u8; RELAY_REGISTER_RESPONSE_LENGTH];
        let (retry_length, retry_source) =
            timeout(Duration::from_secs(1), socket.recv_from(&mut retry))
                .await
                .expect("registration retry timeout")
                .expect("receive registration retry");
        assert_eq!(retry_length, retry.len());
        assert_eq!(retry_source, context.relay_endpoint);
        assert_eq!(retry, response);
        RegisteredNode {
            socket,
            node_id: node_id(&identity_key.verifying_key().to_bytes()),
            lease_id,
            identity_key,
            identity_seed,
        }
    }

    async fn renew_registration(
        context: &TestContext,
        node: &mut RegisteredNode,
        request_id: [u8; 16],
    ) {
        let (_, _, lease_id) = registration_exchange(
            context,
            &node.socket,
            &node.identity_key,
            node.identity_seed,
            request_id,
        )
        .await;
        node.lease_id = lease_id;
    }

    async fn registration_exchange(
        context: &TestContext,
        socket: &UdpSocket,
        identity_key: &SigningKey,
        identity_seed: u8,
        request_id: [u8; 16],
    ) -> (
        [u8; RELAY_REGISTER_REQUEST_LENGTH],
        [u8; RELAY_REGISTER_RESPONSE_LENGTH],
        [u8; 16],
    ) {
        let now = unix_time();
        let node = node_id(&identity_key.verifying_key().to_bytes());
        let credential = sign_credential(
            CredentialClaims {
                network_id: context.network_id,
                identity_public_key: identity_key.verifying_key().to_bytes(),
                virtual_ipv4: Ipv4Addr::new(100, 88, 0, identity_seed),
                serial: u64::from(identity_seed),
                not_before: now.saturating_sub(1),
                not_after: now + 600,
                role_bitmap: 1,
                role_set_digest: role_set_digest(1, &["linux".to_owned()]).expect("role digest"),
            },
            &context.controller_key,
        );
        assert_eq!(credential.len(), CREDENTIAL_LENGTH);
        let request = sign_relay_register_request(
            RelayRegisterRequest {
                network_id: context.network_id,
                node_id: node,
                relay_id: context.relay_id,
                request_id,
                client_time: now,
                credential,
            },
            identity_key,
        );
        socket
            .send_to(&request, context.relay_endpoint)
            .await
            .expect("send registration");
        let mut response = [0_u8; RELAY_REGISTER_RESPONSE_LENGTH];
        let (length, source) = timeout(Duration::from_secs(1), socket.recv_from(&mut response))
            .await
            .expect("registration timeout")
            .expect("receive registration");
        assert_eq!(length, response.len());
        assert_eq!(source, context.relay_endpoint);
        let lease_id = verify_relay_register_response(
            &response,
            &context.relay_key.verifying_key(),
            context.network_id,
            node,
            context.relay_id,
            request_id,
            now,
        )
        .expect("valid registration response")
        .lease_id;
        (request, response, lease_id)
    }

    async fn assert_forwarding_and_replay(
        context: &TestContext,
        first: &RegisteredNode,
        second: &RegisteredNode,
    ) -> Vec<u8> {
        let inner = framed_xsp_keepalive(context.network_id, first.node_id, second.node_id);
        let frame = relay_frame(context, first, second, 1, &inner);
        first
            .socket
            .send_to(&frame, context.relay_endpoint)
            .await
            .expect("send Relay data");
        let mut received = [0_u8; RELAY_MAX_FRAME_LENGTH];
        let (length, source) = timeout(
            Duration::from_secs(1),
            second.socket.recv_from(&mut received),
        )
        .await
        .expect("forward timeout")
        .expect("receive forwarded data");
        assert_eq!(source, context.relay_endpoint);
        assert_eq!(&received[..length], frame);

        first
            .socket
            .send_to(&frame, context.relay_endpoint)
            .await
            .expect("send replay");
        assert!(
            timeout(
                Duration::from_millis(100),
                second.socket.recv_from(&mut received)
            )
            .await
            .is_err()
        );
        inner
    }

    fn relay_frame(
        context: &TestContext,
        source: &RegisteredNode,
        destination: &RegisteredNode,
        sequence: u64,
        inner: &[u8],
    ) -> Vec<u8> {
        encode_relay_frame(RelayFrame {
            network_id: context.network_id,
            relay_id: context.relay_id,
            source_node_id: source.node_id,
            destination_node_id: destination.node_id,
            lease_id: source.lease_id,
            sequence,
            payload: inner,
        })
        .expect("Relay frame")
    }

    async fn assert_keepalive(context: &TestContext, node: &RegisteredNode) {
        let keepalive = encode_relay_keepalive(
            context.network_id,
            context.relay_id,
            node.node_id,
            node.lease_id,
            2,
        );
        node.socket
            .send_to(&keepalive, context.relay_endpoint)
            .await
            .expect("send keepalive");
        let mut response = [0_u8; RELAY_KEEPALIVE_RESPONSE_LENGTH];
        let (length, source) =
            timeout(Duration::from_secs(1), node.socket.recv_from(&mut response))
                .await
                .expect("keepalive timeout")
                .expect("receive keepalive");
        assert_eq!(length, response.len());
        assert_eq!(source, context.relay_endpoint);
        verify_relay_keepalive_response(
            &response,
            &context.relay_key.verifying_key(),
            context.network_id,
            context.relay_id,
            node.node_id,
            node.lease_id,
            unix_time(),
        )
        .expect("valid keepalive response");
    }

    async fn assert_endpoint_spoof_is_rejected(
        context: &TestContext,
        first: &RegisteredNode,
        second: &RegisteredNode,
        inner: &[u8],
    ) {
        let attacker = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("bind attacker");
        let spoofed = encode_relay_frame(RelayFrame {
            network_id: context.network_id,
            relay_id: context.relay_id,
            source_node_id: first.node_id,
            destination_node_id: second.node_id,
            lease_id: first.lease_id,
            sequence: 3,
            payload: inner,
        })
        .expect("spoofed frame");
        attacker
            .send_to(&spoofed, context.relay_endpoint)
            .await
            .expect("send spoofed frame");
        let mut received = [0_u8; RELAY_MAX_FRAME_LENGTH];
        assert!(
            timeout(
                Duration::from_millis(100),
                second.socket.recv_from(&mut received)
            )
            .await
            .is_err()
        );
    }

    fn framed_xsp_keepalive(
        network_id: [u8; 16],
        source_node_id: [u8; 16],
        destination_node_id: [u8; 16],
    ) -> Vec<u8> {
        let header = DataHeader {
            packet_type: PacketType::Keepalive,
            flags: DataFlags::NONE,
            payload_length: 0,
            network_id,
            source_node_id,
            destination_node_id,
            session_id: [61_u8; 16],
            key_epoch: 0,
            sequence: 0,
            path_id: 0,
        };
        let mut packet = Vec::with_capacity(DATA_HEADER_LENGTH + DATA_TAG_LENGTH);
        packet.extend_from_slice(&header.encode());
        packet.extend_from_slice(&[0_u8; DATA_TAG_LENGTH]);
        packet
    }
}
