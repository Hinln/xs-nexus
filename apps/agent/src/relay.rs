use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};

use chrono::Utc;
use ed25519_dalek::VerifyingKey;
use xs_core::{ConfigurationRelay, EndpointCandidate, EndpointCandidateKind};
use xs_protocol::{
    CREDENTIAL_LENGTH, RELAY_DATA_TYPE, RELAY_KEEPALIVE_RESPONSE_TYPE,
    RELAY_REGISTER_RESPONSE_TYPE, RelayFrame, RelayRegisterRequest, encode_relay_frame,
    encode_relay_keepalive, parse_relay_frame, sign_relay_register_request,
    verify_relay_keepalive_response, verify_relay_register_response,
};

use crate::{
    candidates::normalize_endpoint,
    error::{AgentError, Result},
    state::{NodeState, decode_fixed},
    storage::Identity,
};

const MAX_RELAYS: usize = 16;
const RELAY_CANDIDATE_PRIORITY_MAX: u32 = 9_999;
const REGISTRATION_MAX_ATTEMPTS: u8 = 6;
#[cfg(not(feature = "privileged-network-tests"))]
const REGISTRATION_RETRY_INTERVAL: Duration = Duration::from_millis(500);
#[cfg(feature = "privileged-network-tests")]
const REGISTRATION_RETRY_INTERVAL: Duration = Duration::from_millis(100);
#[cfg(not(feature = "privileged-network-tests"))]
const REGISTRATION_BACKOFF_BASE: Duration = Duration::from_secs(2);
#[cfg(feature = "privileged-network-tests")]
const REGISTRATION_BACKOFF_BASE: Duration = Duration::from_millis(200);
#[cfg(not(feature = "privileged-network-tests"))]
const REGISTRATION_BACKOFF_MAX: Duration = Duration::from_secs(60);
#[cfg(feature = "privileged-network-tests")]
const REGISTRATION_BACKOFF_MAX: Duration = Duration::from_secs(2);
#[cfg(not(feature = "privileged-network-tests"))]
const LEASE_RENEWAL_MARGIN_SECONDS: u64 = 30;
#[cfg(feature = "privileged-network-tests")]
const LEASE_RENEWAL_MARGIN_SECONDS: u64 = 2;
#[cfg(not(feature = "privileged-network-tests"))]
const RELAY_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(15);
#[cfg(feature = "privileged-network-tests")]
const RELAY_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(1);
#[cfg(not(feature = "privileged-network-tests"))]
const RELAY_FAILURE_TIMEOUT: Duration = Duration::from_secs(45);
#[cfg(feature = "privileged-network-tests")]
const RELAY_FAILURE_TIMEOUT: Duration = Duration::from_secs(4);

struct RelayClient {
    relay_id: [u8; 16],
    endpoint: SocketAddr,
    verifying_key: VerifyingKey,
    priority: u32,
    config_expires_at: chrono::DateTime<Utc>,
    lease: Option<RelayLease>,
    pending: Option<PendingRegistration>,
    next_registration_at: Instant,
    failures: u8,
}

struct RelayLease {
    lease_id: [u8; 16],
    expires_at: u64,
    next_sequence: u64,
    last_keepalive_at: Instant,
    last_authenticated_activity: Instant,
}

struct PendingRegistration {
    request_id: [u8; 16],
    encoded: Vec<u8>,
    last_sent: Instant,
    attempts: u8,
}

pub(crate) struct RelayMaintenance {
    pub outbound: Vec<(SocketAddr, Vec<u8>)>,
    pub failed_endpoints: Vec<SocketAddr>,
}

pub(crate) enum RelayInbound {
    NotRelay,
    Consumed,
    Data {
        source_node_id: [u8; 16],
        payload: Vec<u8>,
    },
}

pub(crate) struct RelayManager {
    network_id: [u8; 16],
    node_id: [u8; 16],
    credential: [u8; CREDENTIAL_LENGTH],
    identity: Arc<Identity>,
    clients: HashMap<SocketAddr, RelayClient>,
}

impl RelayManager {
    pub fn new(
        state: &NodeState,
        network_id: [u8; 16],
        node_id: [u8; 16],
        credential: [u8; CREDENTIAL_LENGTH],
        identity: Arc<Identity>,
    ) -> Result<Self> {
        let clients = relay_clients(&state.configuration_payload.relays)?;
        Ok(Self {
            network_id,
            node_id,
            credential,
            identity,
            clients,
        })
    }

    pub fn apply_configuration(&mut self, state: &NodeState) -> Result<Vec<SocketAddr>> {
        let desired = relay_clients(&state.configuration_payload.relays)?;
        let mut previous = std::mem::take(&mut self.clients);
        let mut updated = HashMap::with_capacity(desired.len());
        let mut failed = Vec::new();
        for (endpoint, mut client) in desired {
            if let Some(existing) = previous.remove(&endpoint) {
                if existing.relay_id == client.relay_id
                    && existing.verifying_key.to_bytes() == client.verifying_key.to_bytes()
                {
                    client.lease = existing.lease;
                    client.pending = existing.pending;
                    client.next_registration_at = existing.next_registration_at;
                    client.failures = existing.failures;
                } else if existing.lease.is_some() {
                    failed.push(endpoint);
                }
            }
            updated.insert(endpoint, client);
        }
        failed.extend(
            previous
                .into_values()
                .filter(|client| client.lease.is_some())
                .map(|client| client.endpoint),
        );
        self.clients = updated;
        Ok(failed)
    }

    #[must_use]
    pub fn candidates(&self) -> Vec<EndpointCandidate> {
        let mut clients = self.clients.values().collect::<Vec<_>>();
        clients.sort_by(|left, right| {
            right
                .priority
                .cmp(&left.priority)
                .then_with(|| left.relay_id.cmp(&right.relay_id))
        });
        clients
            .into_iter()
            .take(MAX_RELAYS)
            .enumerate()
            .map(|(index, client)| EndpointCandidate {
                kind: EndpointCandidateKind::Relay,
                endpoint: client.endpoint,
                priority: RELAY_CANDIDATE_PRIORITY_MAX
                    .saturating_sub(u32::try_from(index).unwrap_or(u32::MAX)),
                expires_at: client.config_expires_at,
            })
            .collect()
    }

    #[must_use]
    pub fn is_relay(&self, endpoint: SocketAddr) -> bool {
        self.clients.contains_key(&normalize_endpoint(endpoint))
    }

    pub fn wrap(
        &mut self,
        endpoint: SocketAddr,
        destination_node_id: [u8; 16],
        payload: &[u8],
        now: u64,
    ) -> Result<Option<Vec<u8>>> {
        let Some(client) = self.clients.get_mut(&normalize_endpoint(endpoint)) else {
            return Ok(None);
        };
        let Some(lease) = client.lease.as_mut() else {
            return Ok(None);
        };
        if lease.expires_at <= now || client.config_expires_at <= Utc::now() {
            client.lease = None;
            return Ok(None);
        }
        let sequence = lease.next_sequence;
        lease.next_sequence = lease
            .next_sequence
            .checked_add(1)
            .ok_or(AgentError::DataPlane)?;
        encode_relay_frame(RelayFrame {
            network_id: self.network_id,
            relay_id: client.relay_id,
            source_node_id: self.node_id,
            destination_node_id,
            lease_id: lease.lease_id,
            sequence,
            payload,
        })
        .map(Some)
        .map_err(|_| AgentError::DataPlane)
    }

    pub fn handle_datagram(
        &mut self,
        source: SocketAddr,
        datagram: &[u8],
        now: Instant,
        unix_now: u64,
    ) -> RelayInbound {
        let source = normalize_endpoint(source);
        let Some(client) = self.clients.get_mut(&source) else {
            return RelayInbound::NotRelay;
        };
        match datagram.get(5).copied() {
            Some(RELAY_REGISTER_RESPONSE_TYPE) => {
                let Some(pending) = client.pending.as_ref() else {
                    return RelayInbound::Consumed;
                };
                let Ok(response) = verify_relay_register_response(
                    datagram,
                    &client.verifying_key,
                    self.network_id,
                    self.node_id,
                    client.relay_id,
                    pending.request_id,
                    unix_now,
                ) else {
                    return RelayInbound::Consumed;
                };
                client.lease = Some(RelayLease {
                    lease_id: response.lease_id,
                    expires_at: response.expires_at,
                    next_sequence: 0,
                    last_keepalive_at: now,
                    last_authenticated_activity: now,
                });
                client.pending = None;
                client.failures = 0;
                client.next_registration_at = now;
                RelayInbound::Consumed
            }
            Some(RELAY_KEEPALIVE_RESPONSE_TYPE) => {
                let Some(lease) = client.lease.as_mut() else {
                    return RelayInbound::Consumed;
                };
                if verify_relay_keepalive_response(
                    datagram,
                    &client.verifying_key,
                    self.network_id,
                    client.relay_id,
                    self.node_id,
                    lease.lease_id,
                    unix_now,
                )
                .is_ok()
                {
                    lease.last_authenticated_activity = now;
                }
                RelayInbound::Consumed
            }
            Some(RELAY_DATA_TYPE) => {
                let Some(lease) = client.lease.as_ref() else {
                    return RelayInbound::Consumed;
                };
                if lease.expires_at <= unix_now {
                    return RelayInbound::Consumed;
                }
                let Ok(frame) = parse_relay_frame(datagram) else {
                    return RelayInbound::Consumed;
                };
                if frame.network_id != self.network_id
                    || frame.relay_id != client.relay_id
                    || frame.destination_node_id != self.node_id
                {
                    return RelayInbound::Consumed;
                }
                RelayInbound::Data {
                    source_node_id: frame.source_node_id,
                    payload: frame.payload.to_vec(),
                }
            }
            _ => RelayInbound::Consumed,
        }
    }

    pub fn authenticate_activity(&mut self, endpoint: SocketAddr, now: Instant) {
        if let Some(lease) = self
            .clients
            .get_mut(&normalize_endpoint(endpoint))
            .and_then(|client| client.lease.as_mut())
        {
            lease.last_authenticated_activity = now;
        }
    }

    pub fn maintain(&mut self, now: Instant, unix_now: u64) -> Result<RelayMaintenance> {
        let mut outbound = Vec::new();
        let mut failed_endpoints = Vec::new();
        for client in self.clients.values_mut() {
            if client.config_expires_at <= Utc::now() {
                if client.lease.take().is_some() {
                    failed_endpoints.push(client.endpoint);
                }
                client.pending = None;
                continue;
            }
            if client.lease.as_ref().is_some_and(|lease| {
                lease.expires_at <= unix_now
                    || now.duration_since(lease.last_authenticated_activity)
                        >= RELAY_FAILURE_TIMEOUT
            }) {
                client.lease = None;
                client.pending = None;
                client.next_registration_at = now;
                failed_endpoints.push(client.endpoint);
            }
            maintain_pending(client, now, &mut outbound);
            if client.pending.is_none()
                && registration_due(client, now, unix_now)
                && let Some(encoded) = begin_registration(
                    client,
                    self.network_id,
                    self.node_id,
                    self.credential,
                    &self.identity,
                    now,
                    unix_now,
                )?
            {
                outbound.push((client.endpoint, encoded));
            }
            if let Some(lease) = client.lease.as_mut()
                && now.duration_since(lease.last_keepalive_at) >= RELAY_KEEPALIVE_INTERVAL
            {
                let sequence = lease.next_sequence;
                lease.next_sequence = lease
                    .next_sequence
                    .checked_add(1)
                    .ok_or(AgentError::DataPlane)?;
                outbound.push((
                    client.endpoint,
                    encode_relay_keepalive(
                        self.network_id,
                        client.relay_id,
                        self.node_id,
                        lease.lease_id,
                        sequence,
                    )
                    .to_vec(),
                ));
                lease.last_keepalive_at = now;
            }
        }
        Ok(RelayMaintenance {
            outbound,
            failed_endpoints,
        })
    }
}

fn relay_clients(relays: &[ConfigurationRelay]) -> Result<HashMap<SocketAddr, RelayClient>> {
    if relays.len() > MAX_RELAYS {
        return Err(AgentError::ControllerTrust);
    }
    let now = Instant::now();
    let mut clients = HashMap::with_capacity(relays.len());
    let mut relay_ids = HashSet::new();
    for relay in relays {
        let relay_id = decode_fixed::<16>(&relay.relay_id_base64)?;
        let verifying_key =
            VerifyingKey::from_bytes(&decode_fixed::<32>(&relay.identity_public_key_base64)?)
                .map_err(|_| AgentError::ControllerTrust)?;
        let endpoint = normalize_endpoint(relay.endpoint);
        if relay_id == [0_u8; 16]
            || relay.priority == 0
            || relay.expires_at <= Utc::now()
            || !relay_ids.insert(relay_id)
            || clients.contains_key(&endpoint)
        {
            return Err(AgentError::ControllerTrust);
        }
        clients.insert(
            endpoint,
            RelayClient {
                relay_id,
                endpoint,
                verifying_key,
                priority: relay.priority,
                config_expires_at: relay.expires_at,
                lease: None,
                pending: None,
                next_registration_at: now,
                failures: 0,
            },
        );
    }
    Ok(clients)
}

fn maintain_pending(
    client: &mut RelayClient,
    now: Instant,
    outbound: &mut Vec<(SocketAddr, Vec<u8>)>,
) {
    let Some(pending) = client.pending.as_mut() else {
        return;
    };
    if now.duration_since(pending.last_sent) < REGISTRATION_RETRY_INTERVAL {
        return;
    }
    if pending.attempts >= REGISTRATION_MAX_ATTEMPTS {
        client.pending = None;
        client.failures = client.failures.saturating_add(1);
        let exponent = u32::from(client.failures.saturating_sub(1).min(6));
        let delay = (REGISTRATION_BACKOFF_BASE * (1_u32 << exponent)).min(REGISTRATION_BACKOFF_MAX);
        client.next_registration_at = now + delay;
        return;
    }
    pending.attempts = pending.attempts.saturating_add(1);
    pending.last_sent = now;
    outbound.push((client.endpoint, pending.encoded.clone()));
}

fn registration_due(client: &RelayClient, now: Instant, unix_now: u64) -> bool {
    now >= client.next_registration_at
        && client.lease.as_ref().is_none_or(|lease| {
            lease.expires_at.saturating_sub(unix_now) <= LEASE_RENEWAL_MARGIN_SECONDS
        })
}

fn begin_registration(
    client: &mut RelayClient,
    network_id: [u8; 16],
    node_id: [u8; 16],
    credential: [u8; CREDENTIAL_LENGTH],
    identity: &Identity,
    now: Instant,
    unix_now: u64,
) -> Result<Option<Vec<u8>>> {
    if client.pending.is_some() || now < client.next_registration_at {
        return Ok(None);
    }
    let request_id = random_array::<16>()?;
    let encoded = sign_relay_register_request(
        RelayRegisterRequest {
            network_id,
            node_id,
            relay_id: client.relay_id,
            request_id,
            client_time: unix_now,
            credential,
        },
        identity.signing_key(),
    )
    .to_vec();
    client.pending = Some(PendingRegistration {
        request_id,
        encoded: encoded.clone(),
        last_sent: now,
        attempts: 1,
    });
    Ok(Some(encoded))
}

fn random_array<const LENGTH: usize>() -> Result<[u8; LENGTH]> {
    let mut bytes = [0_u8; LENGTH];
    getrandom::fill(&mut bytes).map_err(|_| AgentError::DataPlane)?;
    if bytes == [0_u8; LENGTH] {
        return Err(AgentError::DataPlane);
    }
    Ok(bytes)
}
