use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use ed25519_dalek::{Signature, Signer as _, SigningKey, VerifyingKey};
use sha2::{Digest as _, Sha256};

use crate::{
    CREDENTIAL_LENGTH, InvalidProtocolMessage, VerifiedCredential, node_id, verify_credential,
};

pub const DISCOVERY_REQUEST_LENGTH: usize = 334;
pub const DISCOVERY_RESPONSE_LENGTH: usize = 188;
const MAGIC: &[u8; 4] = b"XSD1";
const VERSION: u8 = 1;
const REQUEST_TYPE: u8 = 1;
const RESPONSE_TYPE: u8 = 2;
const REQUEST_SIGNED_LENGTH: usize = DISCOVERY_REQUEST_LENGTH - 64;
const RESPONSE_SIGNED_LENGTH: usize = DISCOVERY_RESPONSE_LENGTH - 64;
const REQUEST_DOMAIN: &[u8] = b"XS Nexus discovery request v1";
const RESPONSE_DOMAIN: &[u8] = b"XS Nexus discovery response v1";
const MAX_CLOCK_SKEW_SECONDS: u64 = 300;
const _: () = assert!(DISCOVERY_RESPONSE_LENGTH < DISCOVERY_REQUEST_LENGTH);

pub struct DiscoveryRequest {
    encoded: [u8; DISCOVERY_REQUEST_LENGTH],
    network_id: [u8; 16],
    node_id: [u8; 16],
    request_id: [u8; 16],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VerifiedDiscoveryRequest {
    pub network_id: [u8; 16],
    pub node_id: [u8; 16],
    pub identity_public_key: [u8; 32],
    pub credential_serial: u64,
    pub request_id: [u8; 16],
    pub request_hash: [u8; 32],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VerifiedDiscoveryResponse {
    pub observed_endpoint: SocketAddr,
    pub server_time: u64,
}

impl DiscoveryRequest {
    /// Creates one fixed-length authenticated UDP mapping-discovery request.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidProtocolMessage`] when identity, credential, time, or request fields do
    /// not match.
    pub fn new(
        network_id: [u8; 16],
        node_id: [u8; 16],
        request_id: [u8; 16],
        client_time: u64,
        credential: [u8; CREDENTIAL_LENGTH],
        identity_signing_key: &SigningKey,
    ) -> Result<Self, InvalidProtocolMessage> {
        if network_id == [0_u8; 16]
            || node_id == [0_u8; 16]
            || request_id == [0_u8; 16]
            || client_time == 0
            || node_id != crate::node_id(&identity_signing_key.verifying_key().to_bytes())
        {
            return Err(InvalidProtocolMessage);
        }
        let mut encoded = [0_u8; DISCOVERY_REQUEST_LENGTH];
        encode_header(&mut encoded, REQUEST_TYPE)?;
        encoded[12..28].copy_from_slice(&network_id);
        encoded[28..44].copy_from_slice(&node_id);
        encoded[44..60].copy_from_slice(&request_id);
        encoded[60..68].copy_from_slice(&client_time.to_be_bytes());
        encoded[68..70].copy_from_slice(
            &u16::try_from(CREDENTIAL_LENGTH)
                .map_err(|_| InvalidProtocolMessage)?
                .to_be_bytes(),
        );
        encoded[70..270].copy_from_slice(&credential);
        let signature = sign(
            REQUEST_DOMAIN,
            &encoded[..REQUEST_SIGNED_LENGTH],
            identity_signing_key,
        );
        encoded[REQUEST_SIGNED_LENGTH..].copy_from_slice(&signature);
        Ok(Self {
            encoded,
            network_id,
            node_id,
            request_id,
        })
    }

    #[must_use]
    pub fn encoded(&self) -> &[u8; DISCOVERY_REQUEST_LENGTH] {
        &self.encoded
    }
}

/// Verifies a discovery request without returning detailed rejection reasons.
///
/// # Errors
///
/// Returns [`InvalidProtocolMessage`] for any framing, time, credential, identity, or signature
/// failure.
pub fn verify_discovery_request(
    encoded: &[u8],
    credential_verifying_key: &VerifyingKey,
    now: u64,
) -> Result<VerifiedDiscoveryRequest, InvalidProtocolMessage> {
    validate_header(encoded, REQUEST_TYPE, DISCOVERY_REQUEST_LENGTH)?;
    let network_id = array::<16>(&encoded[12..28])?;
    let encoded_node_id = array::<16>(&encoded[28..44])?;
    let request_id = array::<16>(&encoded[44..60])?;
    let client_time = u64::from_be_bytes(array::<8>(&encoded[60..68])?);
    let credential_length = u16::from_be_bytes(array::<2>(&encoded[68..70])?);
    if network_id == [0_u8; 16]
        || encoded_node_id == [0_u8; 16]
        || request_id == [0_u8; 16]
        || client_time == 0
        || !time_is_fresh(client_time, now)
        || usize::from(credential_length) != CREDENTIAL_LENGTH
    {
        return Err(InvalidProtocolMessage);
    }
    let credential = &encoded[70..270];
    let claims = verify_credential(credential, credential_verifying_key, now)
        .map_err(|_| InvalidProtocolMessage)?;
    validate_request_identity(network_id, encoded_node_id, claims)?;
    let verifying_key = VerifyingKey::from_bytes(&claims.identity_public_key)
        .map_err(|_| InvalidProtocolMessage)?;
    verify_signature(
        REQUEST_DOMAIN,
        &encoded[..REQUEST_SIGNED_LENGTH],
        &encoded[REQUEST_SIGNED_LENGTH..],
        &verifying_key,
    )?;
    Ok(VerifiedDiscoveryRequest {
        network_id,
        node_id: encoded_node_id,
        identity_public_key: claims.identity_public_key,
        credential_serial: claims.serial,
        request_id,
        request_hash: Sha256::digest(encoded).into(),
    })
}

/// Creates a fixed-length Controller-signed discovery response.
///
/// # Errors
///
/// Returns [`InvalidProtocolMessage`] for an unusable observed endpoint or invalid time.
pub fn discovery_response(
    request: VerifiedDiscoveryRequest,
    observed_endpoint: SocketAddr,
    server_time: u64,
    configuration_signing_key: &SigningKey,
) -> Result<[u8; DISCOVERY_RESPONSE_LENGTH], InvalidProtocolMessage> {
    if server_time == 0 || !valid_observed_endpoint(observed_endpoint) {
        return Err(InvalidProtocolMessage);
    }
    let mut encoded = [0_u8; DISCOVERY_RESPONSE_LENGTH];
    encode_header(&mut encoded, RESPONSE_TYPE)?;
    encoded[12..28].copy_from_slice(&request.network_id);
    encoded[28..44].copy_from_slice(&request.node_id);
    encoded[44..60].copy_from_slice(&request.request_id);
    encoded[60..68].copy_from_slice(&server_time.to_be_bytes());
    encode_endpoint(observed_endpoint, &mut encoded[68..92])?;
    encoded[92..124].copy_from_slice(&request.request_hash);
    let signature = sign(
        RESPONSE_DOMAIN,
        &encoded[..RESPONSE_SIGNED_LENGTH],
        configuration_signing_key,
    );
    encoded[RESPONSE_SIGNED_LENGTH..].copy_from_slice(&signature);
    Ok(encoded)
}

/// Verifies one discovery response against the exact request that caused it.
///
/// # Errors
///
/// Returns [`InvalidProtocolMessage`] for any framing, request-binding, time, endpoint, or
/// Controller signature failure.
pub fn verify_discovery_response(
    encoded: &[u8],
    request: &DiscoveryRequest,
    configuration_verifying_key: &VerifyingKey,
    now: u64,
) -> Result<VerifiedDiscoveryResponse, InvalidProtocolMessage> {
    validate_header(encoded, RESPONSE_TYPE, DISCOVERY_RESPONSE_LENGTH)?;
    let network_id = array::<16>(&encoded[12..28])?;
    let encoded_node_id = array::<16>(&encoded[28..44])?;
    let request_id = array::<16>(&encoded[44..60])?;
    let server_time = u64::from_be_bytes(array::<8>(&encoded[60..68])?);
    let observed_endpoint = decode_endpoint(&encoded[68..92])?;
    let request_hash = array::<32>(&encoded[92..124])?;
    if network_id != request.network_id
        || encoded_node_id != request.node_id
        || request_id != request.request_id
        || request_hash != Sha256::digest(request.encoded).as_slice()
        || !time_is_fresh(server_time, now)
        || !valid_observed_endpoint(observed_endpoint)
    {
        return Err(InvalidProtocolMessage);
    }
    verify_signature(
        RESPONSE_DOMAIN,
        &encoded[..RESPONSE_SIGNED_LENGTH],
        &encoded[RESPONSE_SIGNED_LENGTH..],
        configuration_verifying_key,
    )?;
    Ok(VerifiedDiscoveryResponse {
        observed_endpoint,
        server_time,
    })
}

fn validate_request_identity(
    network_id: [u8; 16],
    encoded_node_id: [u8; 16],
    claims: VerifiedCredential,
) -> Result<(), InvalidProtocolMessage> {
    if claims.network_id != network_id
        || claims.node_id != encoded_node_id
        || node_id(&claims.identity_public_key) != encoded_node_id
    {
        return Err(InvalidProtocolMessage);
    }
    Ok(())
}

fn encode_header(encoded: &mut [u8], packet_type: u8) -> Result<(), InvalidProtocolMessage> {
    let length = u16::try_from(encoded.len()).map_err(|_| InvalidProtocolMessage)?;
    encoded[0..4].copy_from_slice(MAGIC);
    encoded[4] = VERSION;
    encoded[5] = packet_type;
    encoded[8..10].copy_from_slice(&length.to_be_bytes());
    Ok(())
}

fn validate_header(
    encoded: &[u8],
    packet_type: u8,
    expected_length: usize,
) -> Result<(), InvalidProtocolMessage> {
    if encoded.len() != expected_length
        || &encoded[0..4] != MAGIC
        || encoded[4] != VERSION
        || encoded[5] != packet_type
        || encoded[6..8] != [0_u8; 2]
        || usize::from(u16::from_be_bytes(array::<2>(&encoded[8..10])?)) != expected_length
        || encoded[10..12] != [0_u8; 2]
    {
        return Err(InvalidProtocolMessage);
    }
    Ok(())
}

fn encode_endpoint(endpoint: SocketAddr, encoded: &mut [u8]) -> Result<(), InvalidProtocolMessage> {
    if encoded.len() != 24 || !valid_observed_endpoint(endpoint) {
        return Err(InvalidProtocolMessage);
    }
    match endpoint.ip() {
        IpAddr::V4(address) => {
            encoded[0] = 4;
            encoded[4..8].copy_from_slice(&address.octets());
        }
        IpAddr::V6(address) => {
            encoded[0] = 6;
            encoded[4..20].copy_from_slice(&address.octets());
        }
    }
    encoded[20..22].copy_from_slice(&endpoint.port().to_be_bytes());
    Ok(())
}

fn decode_endpoint(encoded: &[u8]) -> Result<SocketAddr, InvalidProtocolMessage> {
    if encoded.len() != 24 || encoded[1..4] != [0_u8; 3] || encoded[22..24] != [0_u8; 2] {
        return Err(InvalidProtocolMessage);
    }
    let port = u16::from_be_bytes(array::<2>(&encoded[20..22])?);
    let address = match encoded[0] {
        4 if encoded[8..20] == [0_u8; 12] => {
            IpAddr::V4(Ipv4Addr::from(array::<4>(&encoded[4..8])?))
        }
        6 => IpAddr::V6(Ipv6Addr::from(array::<16>(&encoded[4..20])?)),
        _ => return Err(InvalidProtocolMessage),
    };
    Ok(SocketAddr::new(address, port))
}

fn valid_observed_endpoint(endpoint: SocketAddr) -> bool {
    if endpoint.port() == 0 {
        return false;
    }
    match endpoint.ip() {
        IpAddr::V4(address) => {
            !address.is_unspecified() && !address.is_multicast() && address != Ipv4Addr::BROADCAST
        }
        IpAddr::V6(address) => !address.is_unspecified() && !address.is_multicast(),
    }
}

fn time_is_fresh(encoded: u64, now: u64) -> bool {
    encoded.abs_diff(now) <= MAX_CLOCK_SKEW_SECONDS
}

fn sign(domain: &[u8], message: &[u8], key: &SigningKey) -> [u8; 64] {
    let mut input = Vec::with_capacity(domain.len() + message.len());
    input.extend_from_slice(domain);
    input.extend_from_slice(message);
    key.sign(&input).to_bytes()
}

fn verify_signature(
    domain: &[u8],
    message: &[u8],
    encoded_signature: &[u8],
    key: &VerifyingKey,
) -> Result<(), InvalidProtocolMessage> {
    let signature = Signature::from_bytes(&array::<64>(encoded_signature)?);
    let mut input = Vec::with_capacity(domain.len() + message.len());
    input.extend_from_slice(domain);
    input.extend_from_slice(message);
    key.verify_strict(&input, &signature)
        .map_err(|_| InvalidProtocolMessage)
}

fn array<const LENGTH: usize>(bytes: &[u8]) -> Result<[u8; LENGTH], InvalidProtocolMessage> {
    bytes.try_into().map_err(|_| InvalidProtocolMessage)
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, SocketAddrV4};

    use crate::{CredentialClaims, role_set_digest, sign_credential};

    use super::*;

    fn fixture() -> (
        DiscoveryRequest,
        SigningKey,
        SigningKey,
        VerifiedDiscoveryRequest,
    ) {
        let identity = SigningKey::from_bytes(&[11_u8; 32]);
        let credential_key = SigningKey::from_bytes(&[12_u8; 32]);
        let configuration_key = SigningKey::from_bytes(&[13_u8; 32]);
        let network_id = [21_u8; 16];
        let node_id = node_id(&identity.verifying_key().to_bytes());
        let credential = sign_credential(
            CredentialClaims {
                network_id,
                identity_public_key: identity.verifying_key().to_bytes(),
                virtual_ipv4: Ipv4Addr::new(100, 88, 0, 16),
                serial: 7,
                not_before: 1_699_999_900,
                not_after: 1_700_003_600,
                role_bitmap: 1,
                role_set_digest: role_set_digest(1, &["linux".to_owned()]).expect("role digest"),
            },
            &credential_key,
        );
        let request = DiscoveryRequest::new(
            network_id,
            node_id,
            [31_u8; 16],
            1_700_000_000,
            credential,
            &identity,
        )
        .expect("request");
        let verified = verify_discovery_request(
            request.encoded(),
            &credential_key.verifying_key(),
            1_700_000_010,
        )
        .expect("verified request");
        (request, credential_key, configuration_key, verified)
    }

    #[test]
    fn authenticated_discovery_round_trip_is_request_bound() {
        let (request, _, configuration_key, verified) = fixture();
        let observed = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(203, 0, 113, 7), 42001));
        let response = discovery_response(verified, observed, 1_700_000_020, &configuration_key)
            .expect("response");
        let opened = verify_discovery_response(
            &response,
            &request,
            &configuration_key.verifying_key(),
            1_700_000_030,
        )
        .expect("verified response");
        assert_eq!(opened.observed_endpoint, observed);
    }

    #[test]
    fn discovery_rejects_tamper_replay_and_amplification() {
        let (request, credential_key, configuration_key, verified) = fixture();

        let mut tampered_request = *request.encoded();
        tampered_request[44] ^= 1;
        assert!(
            verify_discovery_request(
                &tampered_request,
                &credential_key.verifying_key(),
                1_700_000_010
            )
            .is_err()
        );

        let response = discovery_response(
            verified,
            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(203, 0, 113, 7), 42001)),
            1_700_000_020,
            &configuration_key,
        )
        .expect("response");
        let mut tampered_response = response;
        tampered_response[88] ^= 1;
        assert!(
            verify_discovery_response(
                &tampered_response,
                &request,
                &configuration_key.verifying_key(),
                1_700_000_030
            )
            .is_err()
        );
        assert!(
            verify_discovery_response(
                &response,
                &request,
                &configuration_key.verifying_key(),
                1_700_000_400
            )
            .is_err()
        );
    }
}
