use std::{
    collections::HashSet,
    net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4},
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use ed25519_dalek::{Signature, VerifyingKey};
use ipnet::Ipv4Net;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use xs_core::{
    ConfigurationPayload, EndpointCandidate, EndpointCandidateKind, EnrollResponse,
    SignedConfiguration,
};
use xs_protocol::{controller_key_id, node_id, role_set_digest, verify_credential};

use crate::{
    error::{AgentError, Result},
    storage::Identity,
};

const CONFIGURATION_DOMAIN: &[u8] = b"XS Nexus configuration v1";
const MAX_CONFIGURATION_BYTES: usize = 256 * 1024;
const MAX_CONFIGURATION_NODES: usize = 65_535;
const MAX_DIRECT_ENDPOINTS_PER_NODE: usize = 8;
const MAX_DISCOVERY_ENDPOINTS: usize = 8;
const MAX_CANDIDATES_PER_NODE: usize = 16;
const MAX_CONFIGURED_RELAYS: usize = 16;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeState {
    pub schema_version: u8,
    pub controller_url: String,
    pub network_id: Uuid,
    pub node_id_base64: String,
    pub virtual_ip: Ipv4Addr,
    pub credential_base64: String,
    pub credential_key_id: u32,
    pub credential_signing_public_key_base64: String,
    pub configuration_signing_public_key_base64: String,
    pub configuration: SignedConfiguration,
    pub configuration_payload: ConfigurationPayload,
    pub configuration_sha256: String,
    pub credential_serial: u64,
    #[serde(default)]
    pub candidate_generation: u64,
}

impl NodeState {
    /// Builds node state from an enrollment response after validating the entire trust chain.
    ///
    /// # Errors
    ///
    /// Returns an Agent error when credentials, signatures, identities, addresses, or keys do
    /// not match the local identity and enrollment response.
    pub fn from_enrollment(
        response: EnrollResponse,
        identity: &Identity,
        controller_url: &str,
    ) -> Result<Self> {
        let credential_key = verifying_key(&response.credential_signing_public_key_base64)?;
        let configuration_key = verifying_key(&response.configuration_signing_public_key_base64)?;
        if credential_key == configuration_key {
            return Err(AgentError::ControllerTrust);
        }

        let credential = decode_bounded(&response.credential_base64, 200)?;
        if credential.len() != 200 {
            return Err(AgentError::ControllerTrust);
        }
        let now = u64::try_from(Utc::now().timestamp()).map_err(|_| AgentError::ControllerTrust)?;
        let claims = verify_credential(&credential, &credential_key, now)
            .map_err(|_| AgentError::ControllerTrust)?;
        let local_public_key = identity.public_key();
        let local_node_id = node_id(&local_public_key);
        let encoded_node_id = decode_fixed::<16>(&response.node_id_base64)?;
        let virtual_ip = response
            .virtual_ip
            .parse::<Ipv4Addr>()
            .map_err(|_| AgentError::ControllerTrust)?;

        if claims.network_id != *response.network_id.as_bytes()
            || claims.identity_public_key != local_public_key
            || claims.node_id != local_node_id
            || encoded_node_id != local_node_id
            || claims.virtual_ipv4 != virtual_ip
            || claims.controller_key_id != response.credential_key_id
        {
            return Err(AgentError::ControllerTrust);
        }

        let (payload, configuration_sha256) = validate_configuration(
            &response.configuration,
            &configuration_key,
            response.network_id,
            local_node_id,
            local_public_key,
            virtual_ip,
            claims.serial,
        )?;

        Ok(Self {
            schema_version: 1,
            controller_url: controller_url.to_owned(),
            network_id: response.network_id,
            node_id_base64: response.node_id_base64,
            virtual_ip,
            credential_base64: response.credential_base64,
            credential_key_id: response.credential_key_id,
            credential_signing_public_key_base64: response.credential_signing_public_key_base64,
            configuration_signing_public_key_base64: response
                .configuration_signing_public_key_base64,
            configuration: response.configuration,
            configuration_payload: payload,
            configuration_sha256,
            credential_serial: claims.serial,
            candidate_generation: 0,
        })
    }

    /// Revalidates persisted state against the local identity and configured Controller.
    ///
    /// # Errors
    ///
    /// Returns an Agent error when persisted state is malformed, expired, or untrusted.
    pub fn validate(&self, identity: &Identity, controller_url: &str) -> Result<()> {
        if self.schema_version != 1 || self.controller_url != controller_url {
            return Err(AgentError::State);
        }
        let credential_key = verifying_key(&self.credential_signing_public_key_base64)?;
        let configuration_key = verifying_key(&self.configuration_signing_public_key_base64)?;
        if credential_key == configuration_key {
            return Err(AgentError::ControllerTrust);
        }
        let credential = decode_bounded(&self.credential_base64, 200)?;
        let now = u64::try_from(Utc::now().timestamp()).map_err(|_| AgentError::State)?;
        let claims = verify_credential(&credential, &credential_key, now)
            .map_err(|_| AgentError::ControllerTrust)?;
        let local_public_key = identity.public_key();
        let local_node_id = node_id(&local_public_key);

        if claims.network_id != *self.network_id.as_bytes()
            || claims.node_id != local_node_id
            || claims.identity_public_key != local_public_key
            || claims.virtual_ipv4 != self.virtual_ip
            || claims.serial != self.credential_serial
            || claims.controller_key_id != self.credential_key_id
            || decode_fixed::<16>(&self.node_id_base64)? != local_node_id
        {
            return Err(AgentError::ControllerTrust);
        }

        let (payload, hash) = validate_configuration(
            &self.configuration,
            &configuration_key,
            self.network_id,
            local_node_id,
            local_public_key,
            self.virtual_ip,
            self.credential_serial,
        )?;
        if payload != self.configuration_payload || hash != self.configuration_sha256 {
            return Err(AgentError::ControllerTrust);
        }
        Ok(())
    }

    /// Applies a newer signed configuration while rejecting rollback and equivocation.
    ///
    /// # Errors
    ///
    /// Returns an Agent error when the configuration is invalid, untrusted, or not monotonic.
    pub fn apply_configuration(
        &mut self,
        configuration: SignedConfiguration,
        identity: &Identity,
    ) -> Result<bool> {
        let configuration_key = verifying_key(&self.configuration_signing_public_key_base64)?;
        let local_public_key = identity.public_key();
        let local_node_id = node_id(&local_public_key);
        let (payload, hash) = validate_configuration(
            &configuration,
            &configuration_key,
            self.network_id,
            local_node_id,
            local_public_key,
            self.virtual_ip,
            self.credential_serial,
        )?;

        if configuration.version < self.configuration.version
            || (configuration.version == self.configuration.version
                && (configuration != self.configuration || hash != self.configuration_sha256))
        {
            return Err(AgentError::ControllerTrust);
        }
        if configuration.version == self.configuration.version {
            return Ok(false);
        }

        self.configuration = configuration;
        self.configuration_payload = payload;
        self.configuration_sha256 = hash;
        Ok(true)
    }
}

fn validate_configuration(
    configuration: &SignedConfiguration,
    verifying_key: &VerifyingKey,
    network_id: Uuid,
    local_node_id: [u8; 16],
    local_public_key: [u8; 32],
    virtual_ip: Ipv4Addr,
    credential_serial: u64,
) -> Result<(ConfigurationPayload, String)> {
    if configuration.version == 0 || configuration.signer_key_id != controller_key_id(verifying_key)
    {
        return Err(AgentError::ControllerTrust);
    }
    let payload_bytes = decode_bounded(&configuration.payload_base64, MAX_CONFIGURATION_BYTES)?;
    let signature_bytes = decode_fixed::<64>(&configuration.signature_base64)?;
    let signature = Signature::from_bytes(&signature_bytes);
    let mut signing_input = Vec::with_capacity(CONFIGURATION_DOMAIN.len() + payload_bytes.len());
    signing_input.extend_from_slice(CONFIGURATION_DOMAIN);
    signing_input.extend_from_slice(&payload_bytes);
    verifying_key
        .verify_strict(&signing_input, &signature)
        .map_err(|_| AgentError::ControllerTrust)?;

    let payload: ConfigurationPayload =
        serde_json::from_slice(&payload_bytes).map_err(|_| AgentError::ControllerTrust)?;
    let address_pool = payload
        .address_pool
        .parse::<Ipv4Net>()
        .map_err(|_| AgentError::ControllerTrust)?;
    if payload.schema_version != 1
        || payload.network_id != network_id
        || payload.version != configuration.version
        || !(8..=30).contains(&address_pool.prefix_len())
        || !address_pool.contains(&virtual_ip)
        || payload.discovery_endpoints.len() > MAX_DISCOVERY_ENDPOINTS
        || payload.nodes.len() > MAX_CONFIGURATION_NODES
        || payload.relays.len() > MAX_CONFIGURED_RELAYS
        || payload.policies.len() > 4096
    {
        return Err(AgentError::ControllerTrust);
    }

    validate_discovery_endpoints(&payload.discovery_endpoints)?;
    let relay_endpoints = validate_relays(&payload)?;

    let mut node_ids = HashSet::with_capacity(payload.nodes.len());
    let mut public_keys = HashSet::with_capacity(payload.nodes.len());
    let mut virtual_ips = HashSet::with_capacity(payload.nodes.len());
    let mut direct_endpoints = HashSet::new();
    let mut local_match = false;
    for node in &payload.nodes {
        let mut node_direct_endpoints = HashSet::new();
        let mut node_candidate_endpoints = HashSet::new();
        let node_id = decode_fixed::<16>(&node.node_id_base64)?;
        let public_key = decode_fixed::<32>(&node.identity_public_key_base64)?;
        let assigned_ip = node
            .virtual_ip
            .parse::<Ipv4Addr>()
            .map_err(|_| AgentError::ControllerTrust)?;
        if node.direct_endpoints.len() > MAX_DIRECT_ENDPOINTS_PER_NODE
            || node.candidates.len() > MAX_CANDIDATES_PER_NODE
            || node
                .candidates
                .windows(2)
                .any(|pair| pair[0].priority <= pair[1].priority)
            || node_id != node_id_for_key(&public_key)
            || !address_pool.contains(&assigned_ip)
            || role_set_digest(node.role_bitmap, &node.tags).is_err()
            || !node_ids.insert(node_id)
            || !public_keys.insert(public_key)
            || !virtual_ips.insert(assigned_ip)
        {
            return Err(AgentError::ControllerTrust);
        }
        for encoded_endpoint in &node.direct_endpoints {
            let endpoint = encoded_endpoint
                .parse::<SocketAddrV4>()
                .map_err(|_| AgentError::ControllerTrust)?;
            if endpoint.port() == 0
                || endpoint.ip().is_unspecified()
                || endpoint.ip().is_multicast()
                || *endpoint.ip() == Ipv4Addr::BROADCAST
                || relay_endpoints.contains(&SocketAddr::V4(endpoint))
                || !node_direct_endpoints.insert(SocketAddr::V4(endpoint))
                || !direct_endpoints.insert(SocketAddr::V4(endpoint))
            {
                return Err(AgentError::ControllerTrust);
            }
        }
        for candidate in &node.candidates {
            if candidate.priority == 0
                || candidate.expires_at <= payload.generated_at
                || !valid_candidate(candidate)
                || relay_endpoints.contains(&candidate.endpoint)
                || !node_candidate_endpoints.insert(candidate.endpoint)
                || (!node_direct_endpoints.contains(&candidate.endpoint)
                    && !direct_endpoints.insert(candidate.endpoint))
            {
                return Err(AgentError::ControllerTrust);
            }
        }
        if node_id == local_node_id {
            local_match = public_key == local_public_key
                && assigned_ip == virtual_ip
                && node.credential_serial == credential_serial;
        }
    }
    if !local_match {
        return Err(AgentError::ControllerTrust);
    }

    let hash = Sha256::digest(&payload_bytes);
    Ok((payload, hex(&hash)))
}

fn validate_discovery_endpoints(endpoints: &[SocketAddr]) -> Result<()> {
    let mut unique = HashSet::new();
    if endpoints
        .iter()
        .any(|endpoint| !valid_service_endpoint(*endpoint) || !unique.insert(*endpoint))
    {
        return Err(AgentError::ControllerTrust);
    }
    Ok(())
}

fn validate_relays(payload: &ConfigurationPayload) -> Result<HashSet<SocketAddr>> {
    let mut relay_ids = HashSet::new();
    let mut endpoints = HashSet::new();
    let mut public_keys = HashSet::new();
    let mut previous: Option<(u32, &str)> = None;
    for relay in &payload.relays {
        let relay_id = decode_fixed::<16>(&relay.relay_id_base64)?;
        let public_key = decode_fixed::<32>(&relay.identity_public_key_base64)?;
        if relay_id == [0_u8; 16]
            || VerifyingKey::from_bytes(&public_key).is_err()
            || relay.priority == 0
            || relay.expires_at <= payload.generated_at
            || !valid_service_endpoint(relay.endpoint)
            || payload.discovery_endpoints.contains(&relay.endpoint)
            || !relay_ids.insert(relay_id)
            || !endpoints.insert(relay.endpoint)
            || !public_keys.insert(public_key)
        {
            return Err(AgentError::ControllerTrust);
        }
        if let Some((priority, relay_id_base64)) = previous
            && (priority < relay.priority
                || (priority == relay.priority
                    && relay_id_base64 >= relay.relay_id_base64.as_str()))
        {
            return Err(AgentError::ControllerTrust);
        }
        previous = Some((relay.priority, relay.relay_id_base64.as_str()));
    }
    Ok(endpoints)
}

fn valid_service_endpoint(endpoint: SocketAddr) -> bool {
    if endpoint.port() == 0 {
        return false;
    }
    match endpoint {
        SocketAddr::V4(endpoint) => {
            let address = *endpoint.ip();
            !address.is_unspecified() && !address.is_multicast() && address != Ipv4Addr::BROADCAST
        }
        SocketAddr::V6(endpoint) => {
            let address = *endpoint.ip();
            endpoint.flowinfo() == 0
                && !address.is_unspecified()
                && !address.is_multicast()
                && (!address.is_unicast_link_local() || endpoint.scope_id() != 0)
                && (address.is_unicast_link_local() || endpoint.scope_id() == 0)
        }
    }
}

fn valid_candidate(candidate: &EndpointCandidate) -> bool {
    if !valid_service_endpoint(candidate.endpoint) {
        return false;
    }
    match candidate.endpoint {
        SocketAddr::V4(endpoint) => {
            !endpoint.ip().is_loopback() && candidate.kind != EndpointCandidateKind::PublicIpv6
        }
        SocketAddr::V6(endpoint) => {
            let address = *endpoint.ip();
            if address.is_loopback() {
                return false;
            }
            match candidate.kind {
                EndpointCandidateKind::PublicIpv6 => {
                    public_ipv6(address) && endpoint.scope_id() == 0
                }
                EndpointCandidateKind::Local
                | EndpointCandidateKind::Mapped
                | EndpointCandidateKind::Static
                | EndpointCandidateKind::Relay => true,
            }
        }
    }
}

fn public_ipv6(address: Ipv6Addr) -> bool {
    !address.is_unique_local() && !address.is_unicast_link_local()
}

fn node_id_for_key(public_key: &[u8; 32]) -> [u8; 16] {
    node_id(public_key)
}

fn verifying_key(encoded: &str) -> Result<VerifyingKey> {
    VerifyingKey::from_bytes(&decode_fixed::<32>(encoded)?).map_err(|_| AgentError::ControllerTrust)
}

/// Decodes an exact-length canonical `Base64URL` value.
///
/// # Errors
///
/// Returns [`AgentError::ControllerTrust`] when the value is malformed or has another length.
pub fn decode_fixed<const LENGTH: usize>(encoded: &str) -> Result<[u8; LENGTH]> {
    let decoded = decode_bounded(encoded, LENGTH)?;
    decoded.try_into().map_err(|_| AgentError::ControllerTrust)
}

fn decode_bounded(encoded: &str, maximum: usize) -> Result<Vec<u8>> {
    if encoded.is_empty() || encoded.len() > maximum.saturating_mul(2).saturating_add(16) {
        return Err(AgentError::ControllerTrust);
    }
    let decoded = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| AgentError::ControllerTrust)?;
    if decoded.len() > maximum || URL_SAFE_NO_PAD.encode(&decoded) != encoded {
        return Err(AgentError::ControllerTrust);
    }
    Ok(decoded)
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut output, "{byte:02x}").expect("writing to a String cannot fail");
    }
    output
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::{Signer as _, SigningKey};
    use xs_core::ConfigurationNode;
    use xs_protocol::{CredentialClaims, sign_credential};

    use super::*;

    fn enrollment_fixture() -> (EnrollResponse, Identity) {
        let directory = tempfile::tempdir().expect("tempdir");
        let identity =
            Identity::load_or_create(&directory.path().join("identity.key")).expect("identity");
        let credential_key = SigningKey::from_bytes(&[7_u8; 32]);
        let configuration_key = SigningKey::from_bytes(&[8_u8; 32]);
        let network_id = Uuid::from_bytes([3_u8; 16]);
        let virtual_ip = Ipv4Addr::new(100, 88, 0, 10);
        let serial = 42;
        let role_bitmap = 1;
        let tags = vec!["linux".to_owned()];
        let now = u64::try_from(Utc::now().timestamp()).expect("current timestamp");
        let not_after = now + 3600;
        let credential = sign_credential(
            CredentialClaims {
                network_id: *network_id.as_bytes(),
                identity_public_key: identity.public_key(),
                virtual_ipv4: virtual_ip,
                serial,
                not_before: now.saturating_sub(1),
                not_after,
                role_bitmap,
                role_set_digest: role_set_digest(role_bitmap, &tags).expect("valid tags"),
            },
            &credential_key,
        );
        let node_identifier = node_id(&identity.public_key());
        let payload = ConfigurationPayload {
            schema_version: 1,
            network_id,
            version: 1,
            generated_at: Utc::now(),
            address_pool: "100.88.0.0/16".to_owned(),
            discovery_endpoints: Vec::new(),
            nodes: vec![ConfigurationNode {
                node_id_base64: URL_SAFE_NO_PAD.encode(node_identifier),
                identity_public_key_base64: URL_SAFE_NO_PAD.encode(identity.public_key()),
                virtual_ip: virtual_ip.to_string(),
                direct_endpoints: Vec::new(),
                candidates: Vec::new(),
                credential_serial: serial,
                credential_not_after: chrono::DateTime::from_timestamp(
                    i64::try_from(not_after).expect("timestamp fits"),
                    0,
                )
                .expect("valid timestamp"),
                role_bitmap,
                tags,
            }],
            relays: Vec::new(),
            policies: Vec::new(),
        };
        let payload_bytes = serde_json::to_vec(&payload).expect("serialize configuration");
        let mut signing_input = CONFIGURATION_DOMAIN.to_vec();
        signing_input.extend_from_slice(&payload_bytes);
        let signature = configuration_key.sign(&signing_input).to_bytes();

        (
            EnrollResponse {
                network_id,
                node_id_base64: URL_SAFE_NO_PAD.encode(node_identifier),
                virtual_ip: virtual_ip.to_string(),
                credential_base64: URL_SAFE_NO_PAD.encode(credential),
                credential_key_id: controller_key_id(&credential_key.verifying_key()),
                credential_signing_public_key_base64: URL_SAFE_NO_PAD
                    .encode(credential_key.verifying_key().to_bytes()),
                configuration_signing_public_key_base64: URL_SAFE_NO_PAD
                    .encode(configuration_key.verifying_key().to_bytes()),
                configuration: SignedConfiguration {
                    version: 1,
                    payload_base64: URL_SAFE_NO_PAD.encode(payload_bytes),
                    signature_base64: URL_SAFE_NO_PAD.encode(signature),
                    signer_key_id: controller_key_id(&configuration_key.verifying_key()),
                },
            },
            identity,
        )
    }

    #[test]
    fn fixed_decoder_requires_canonical_base64url() {
        let canonical = URL_SAFE_NO_PAD.encode([7_u8; 16]);
        assert_eq!(decode_fixed::<16>(&canonical).expect("decode"), [7_u8; 16]);
        assert!(decode_fixed::<16>(&format!("{canonical}=")).is_err());
        assert!(decode_fixed::<32>(&canonical).is_err());
    }

    #[test]
    fn enrollment_accepts_a_matching_credential_and_configuration_chain() {
        let (response, identity) = enrollment_fixture();
        let state = NodeState::from_enrollment(response, &identity, "https://controller.example/")
            .expect("trusted enrollment");

        state
            .validate(&identity, "https://controller.example/")
            .expect("persisted state remains trusted");
    }

    #[test]
    fn enrollment_rejects_a_tampered_configuration_signature() {
        let (mut response, identity) = enrollment_fixture();
        response.configuration.signature_base64 = URL_SAFE_NO_PAD.encode([0_u8; 64]);

        assert!(
            NodeState::from_enrollment(response, &identity, "https://controller.example/").is_err()
        );
    }
}
