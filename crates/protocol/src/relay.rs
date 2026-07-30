use std::convert::TryInto;

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};

use crate::{
    CLIENT_FINISH_TYPE, CLIENT_HELLO_TYPE, CREDENTIAL_LENGTH, DATA_HEADER_LENGTH, DATA_TAG_LENGTH,
    DataHeader, InvalidProtocolMessage, SERVER_FINISH_TYPE, SERVER_HELLO_TYPE, VerifiedCredential,
    handshake::{CLIENT_HELLO_BODY_LENGTH, FINISH_BODY_LENGTH, SERVER_HELLO_BODY_LENGTH},
    verify_credential,
};

pub const RELAY_REGISTER_REQUEST_LENGTH: usize = 352;
pub const RELAY_REGISTER_RESPONSE_LENGTH: usize = 168;
pub const RELAY_FRAME_HEADER_LENGTH: usize = 104;
pub const RELAY_KEEPALIVE_RESPONSE_LENGTH: usize = 152;
pub const RELAY_MAX_FRAME_LENGTH: usize = 1500;
pub const RELAY_MAX_LEASE_SECONDS: u64 = 300;

pub const RELAY_REGISTER_REQUEST_TYPE: u8 = 1;
pub const RELAY_REGISTER_RESPONSE_TYPE: u8 = 2;
pub const RELAY_DATA_TYPE: u8 = 3;
pub const RELAY_KEEPALIVE_TYPE: u8 = 4;
pub const RELAY_KEEPALIVE_RESPONSE_TYPE: u8 = 5;

const RELAY_MAGIC: &[u8; 4] = b"XSR1";
const RELAY_VERSION: u8 = 1;
const RELAY_REGISTER_REQUEST_DOMAIN: &[u8] = b"XSR/1 register request v1";
const RELAY_REGISTER_RESPONSE_DOMAIN: &[u8] = b"XSR/1 register response v1";
const RELAY_KEEPALIVE_RESPONSE_DOMAIN: &[u8] = b"XSR/1 keepalive response v1";
const COMMON_HEADER_LENGTH: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RelayRegisterRequest {
    pub network_id: [u8; 16],
    pub node_id: [u8; 16],
    pub relay_id: [u8; 16],
    pub request_id: [u8; 16],
    pub client_time: u64,
    pub credential: [u8; CREDENTIAL_LENGTH],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VerifiedRelayRegisterRequest {
    pub request: RelayRegisterRequest,
    pub credential: VerifiedCredential,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RelayRegisterResponse {
    pub network_id: [u8; 16],
    pub node_id: [u8; 16],
    pub relay_id: [u8; 16],
    pub request_id: [u8; 16],
    pub lease_id: [u8; 16],
    pub expires_at: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RelayFrame<'a> {
    pub network_id: [u8; 16],
    pub relay_id: [u8; 16],
    pub source_node_id: [u8; 16],
    pub destination_node_id: [u8; 16],
    pub lease_id: [u8; 16],
    pub sequence: u64,
    pub payload: &'a [u8],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RelayKeepaliveResponse {
    pub network_id: [u8; 16],
    pub relay_id: [u8; 16],
    pub node_id: [u8; 16],
    pub lease_id: [u8; 16],
    pub server_time: u64,
}

#[must_use]
pub fn sign_relay_register_request(
    request: RelayRegisterRequest,
    identity_key: &SigningKey,
) -> [u8; RELAY_REGISTER_REQUEST_LENGTH] {
    let mut encoded = [0_u8; RELAY_REGISTER_REQUEST_LENGTH];
    encode_common(
        &mut encoded,
        RELAY_REGISTER_REQUEST_TYPE,
        RELAY_REGISTER_REQUEST_LENGTH,
        0,
    );
    encoded[16..32].copy_from_slice(&request.network_id);
    encoded[32..48].copy_from_slice(&request.node_id);
    encoded[48..64].copy_from_slice(&request.relay_id);
    encoded[64..80].copy_from_slice(&request.request_id);
    encoded[80..88].copy_from_slice(&request.client_time.to_be_bytes());
    encoded[88..288].copy_from_slice(&request.credential);
    let signature = sign_bytes(RELAY_REGISTER_REQUEST_DOMAIN, &encoded[..288], identity_key);
    encoded[288..352].copy_from_slice(&signature);
    encoded
}

/// Verifies a canonical node-signed Relay registration request and Controller credential.
///
/// # Errors
///
/// Returns [`InvalidProtocolMessage`] for any framing, credential, identity, time, or signature
/// failure.
pub fn verify_relay_register_request(
    encoded: &[u8],
    controller_key: &VerifyingKey,
    now: u64,
) -> Result<VerifiedRelayRegisterRequest, InvalidProtocolMessage> {
    if encoded.len() != RELAY_REGISTER_REQUEST_LENGTH
        || !valid_zero_payload_message(
            encoded,
            RELAY_REGISTER_REQUEST_TYPE,
            RELAY_REGISTER_REQUEST_LENGTH,
        )
    {
        return Err(InvalidProtocolMessage);
    }
    let request = RelayRegisterRequest {
        network_id: array(&encoded[16..32])?,
        node_id: array(&encoded[32..48])?,
        relay_id: array(&encoded[48..64])?,
        request_id: array(&encoded[64..80])?,
        client_time: u64::from_be_bytes(array(&encoded[80..88])?),
        credential: array(&encoded[88..288])?,
    };
    let credential = verify_credential(&request.credential, controller_key, now)
        .map_err(|_| InvalidProtocolMessage)?;
    if credential.network_id != request.network_id
        || credential.node_id != request.node_id
        || request.relay_id == [0_u8; 16]
        || request.request_id == [0_u8; 16]
        || !time_is_fresh(request.client_time, now)
    {
        return Err(InvalidProtocolMessage);
    }
    let verifying_key = VerifyingKey::from_bytes(&credential.identity_public_key)
        .map_err(|_| InvalidProtocolMessage)?;
    verify_signature(
        RELAY_REGISTER_REQUEST_DOMAIN,
        &encoded[..288],
        &encoded[288..352],
        &verifying_key,
    )?;
    Ok(VerifiedRelayRegisterRequest {
        request,
        credential,
    })
}

#[must_use]
pub fn sign_relay_register_response(
    response: RelayRegisterResponse,
    relay_key: &SigningKey,
) -> [u8; RELAY_REGISTER_RESPONSE_LENGTH] {
    let mut encoded = [0_u8; RELAY_REGISTER_RESPONSE_LENGTH];
    encode_common(
        &mut encoded,
        RELAY_REGISTER_RESPONSE_TYPE,
        RELAY_REGISTER_RESPONSE_LENGTH,
        0,
    );
    encoded[16..32].copy_from_slice(&response.network_id);
    encoded[32..48].copy_from_slice(&response.node_id);
    encoded[48..64].copy_from_slice(&response.relay_id);
    encoded[64..80].copy_from_slice(&response.request_id);
    encoded[80..96].copy_from_slice(&response.lease_id);
    encoded[96..104].copy_from_slice(&response.expires_at.to_be_bytes());
    let signature = sign_bytes(RELAY_REGISTER_RESPONSE_DOMAIN, &encoded[..104], relay_key);
    encoded[104..168].copy_from_slice(&signature);
    encoded
}

/// Verifies a Relay-signed short-lived lease response against the pending registration.
///
/// # Errors
///
/// Returns [`InvalidProtocolMessage`] for malformed framing, an invalid signature, mismatched
/// identifiers, a zero lease, or an expiry outside the accepted short-lived window.
pub fn verify_relay_register_response(
    encoded: &[u8],
    relay_key: &VerifyingKey,
    expected_network_id: [u8; 16],
    expected_node_id: [u8; 16],
    expected_relay_id: [u8; 16],
    expected_request_id: [u8; 16],
    now: u64,
) -> Result<RelayRegisterResponse, InvalidProtocolMessage> {
    if encoded.len() != RELAY_REGISTER_RESPONSE_LENGTH
        || !valid_zero_payload_message(
            encoded,
            RELAY_REGISTER_RESPONSE_TYPE,
            RELAY_REGISTER_RESPONSE_LENGTH,
        )
    {
        return Err(InvalidProtocolMessage);
    }
    verify_signature(
        RELAY_REGISTER_RESPONSE_DOMAIN,
        &encoded[..104],
        &encoded[104..168],
        relay_key,
    )?;
    let response = RelayRegisterResponse {
        network_id: array(&encoded[16..32])?,
        node_id: array(&encoded[32..48])?,
        relay_id: array(&encoded[48..64])?,
        request_id: array(&encoded[64..80])?,
        lease_id: array(&encoded[80..96])?,
        expires_at: u64::from_be_bytes(array(&encoded[96..104])?),
    };
    if response.network_id != expected_network_id
        || response.node_id != expected_node_id
        || response.relay_id != expected_relay_id
        || response.request_id != expected_request_id
        || response.lease_id == [0_u8; 16]
        || response.expires_at <= now
        || response.expires_at > now.saturating_add(RELAY_MAX_LEASE_SECONDS)
    {
        return Err(InvalidProtocolMessage);
    }
    Ok(response)
}

/// Encodes a bounded Relay data envelope around an opaque non-empty XSP/1 datagram.
///
/// # Errors
///
/// Returns [`InvalidProtocolMessage`] for zero or ambiguous identifiers, self-forwarding, an empty
/// payload, or a datagram that would exceed [`RELAY_MAX_FRAME_LENGTH`].
pub fn encode_relay_frame(frame: RelayFrame<'_>) -> Result<Vec<u8>, InvalidProtocolMessage> {
    if frame.network_id == [0_u8; 16]
        || frame.relay_id == [0_u8; 16]
        || frame.source_node_id == [0_u8; 16]
        || frame.destination_node_id == [0_u8; 16]
        || frame.source_node_id == frame.destination_node_id
        || frame.lease_id == [0_u8; 16]
        || frame.payload.is_empty()
        || frame.payload.len() > RELAY_MAX_FRAME_LENGTH.saturating_sub(RELAY_FRAME_HEADER_LENGTH)
    {
        return Err(InvalidProtocolMessage);
    }
    validate_relay_payload(frame)?;
    let total_length = RELAY_FRAME_HEADER_LENGTH
        .checked_add(frame.payload.len())
        .ok_or(InvalidProtocolMessage)?;
    let mut encoded = vec![0_u8; total_length];
    encode_common(
        &mut encoded,
        RELAY_DATA_TYPE,
        RELAY_FRAME_HEADER_LENGTH,
        u16::try_from(frame.payload.len()).map_err(|_| InvalidProtocolMessage)?,
    );
    encoded[16..32].copy_from_slice(&frame.network_id);
    encoded[32..48].copy_from_slice(&frame.relay_id);
    encoded[48..64].copy_from_slice(&frame.source_node_id);
    encoded[64..80].copy_from_slice(&frame.destination_node_id);
    encoded[80..96].copy_from_slice(&frame.lease_id);
    encoded[96..104].copy_from_slice(&frame.sequence.to_be_bytes());
    encoded[104..].copy_from_slice(frame.payload);
    Ok(encoded)
}

/// Parses a canonical bounded Relay data envelope without inspecting or decrypting its payload.
///
/// # Errors
///
/// Returns [`InvalidProtocolMessage`] for invalid framing, length, reserved values, identifiers,
/// self-forwarding, or an empty payload.
pub fn parse_relay_frame(encoded: &[u8]) -> Result<RelayFrame<'_>, InvalidProtocolMessage> {
    if encoded.len() < RELAY_FRAME_HEADER_LENGTH
        || encoded.len() > RELAY_MAX_FRAME_LENGTH
        || !valid_common(encoded, RELAY_DATA_TYPE, RELAY_FRAME_HEADER_LENGTH)
    {
        return Err(InvalidProtocolMessage);
    }
    let payload_length = usize::from(u16::from_be_bytes(array(&encoded[10..12])?));
    if encoded.len() != RELAY_FRAME_HEADER_LENGTH + payload_length {
        return Err(InvalidProtocolMessage);
    }
    let frame = RelayFrame {
        network_id: array(&encoded[16..32])?,
        relay_id: array(&encoded[32..48])?,
        source_node_id: array(&encoded[48..64])?,
        destination_node_id: array(&encoded[64..80])?,
        lease_id: array(&encoded[80..96])?,
        sequence: u64::from_be_bytes(array(&encoded[96..104])?),
        payload: &encoded[104..],
    };
    if frame.network_id == [0_u8; 16]
        || frame.relay_id == [0_u8; 16]
        || frame.source_node_id == [0_u8; 16]
        || frame.destination_node_id == [0_u8; 16]
        || frame.source_node_id == frame.destination_node_id
        || frame.lease_id == [0_u8; 16]
        || frame.payload.is_empty()
    {
        return Err(InvalidProtocolMessage);
    }
    validate_relay_payload(frame)?;
    Ok(frame)
}

/// Validates that a Relay payload is a canonical XSP/1 datagram whose visible routing fields
/// agree with the outer envelope.
///
/// This function validates only framing and cleartext routing fields. It intentionally does not
/// verify XSP/1 signatures, AEAD tags, session state, or ACLs.
///
/// # Errors
///
/// Returns [`InvalidProtocolMessage`] for unknown or noncanonical XSP/1 messages, inconsistent
/// lengths, or visible Network/Source/Destination identifiers that disagree with the envelope.
pub fn validate_relay_payload(frame: RelayFrame<'_>) -> Result<(), InvalidProtocolMessage> {
    let payload = frame.payload;
    if payload.len() < COMMON_HEADER_LENGTH || &payload[0..4] != b"XSP1" || payload[4] != 1 {
        return Err(InvalidProtocolMessage);
    }
    match payload[5] {
        0x01..=0x07 => validate_data_payload(frame),
        CLIENT_HELLO_TYPE => {
            validate_handshake_payload(payload, CLIENT_HELLO_BODY_LENGTH)?;
            validate_visible_route(frame)
        }
        SERVER_HELLO_TYPE => {
            validate_handshake_payload(payload, SERVER_HELLO_BODY_LENGTH)?;
            validate_visible_route(frame)
        }
        CLIENT_FINISH_TYPE | SERVER_FINISH_TYPE => {
            validate_handshake_payload(payload, FINISH_BODY_LENGTH)
        }
        _ => Err(InvalidProtocolMessage),
    }
}

fn validate_data_payload(frame: RelayFrame<'_>) -> Result<(), InvalidProtocolMessage> {
    let header = DataHeader::parse(frame.payload)?;
    let expected_length = DATA_HEADER_LENGTH
        .checked_add(usize::from(header.payload_length))
        .and_then(|length| length.checked_add(DATA_TAG_LENGTH))
        .ok_or(InvalidProtocolMessage)?;
    if frame.payload.len() != expected_length
        || header.network_id != frame.network_id
        || header.source_node_id != frame.source_node_id
        || header.destination_node_id != frame.destination_node_id
    {
        return Err(InvalidProtocolMessage);
    }
    Ok(())
}

fn validate_handshake_payload(
    payload: &[u8],
    expected_body_length: usize,
) -> Result<(), InvalidProtocolMessage> {
    let body_length = usize::try_from(u32::from_be_bytes(array(&payload[8..12])?))
        .map_err(|_| InvalidProtocolMessage)?;
    if payload[6..8] != [0_u8; 2]
        || body_length != expected_body_length
        || payload.len()
            != COMMON_HEADER_LENGTH
                .checked_add(expected_body_length)
                .ok_or(InvalidProtocolMessage)?
    {
        return Err(InvalidProtocolMessage);
    }
    Ok(())
}

fn validate_visible_route(frame: RelayFrame<'_>) -> Result<(), InvalidProtocolMessage> {
    if frame.payload[16..32] != frame.network_id
        || frame.payload[32..48] != frame.source_node_id
        || frame.payload[48..64] != frame.destination_node_id
    {
        return Err(InvalidProtocolMessage);
    }
    Ok(())
}

#[must_use]
pub fn sign_relay_keepalive_response(
    response: RelayKeepaliveResponse,
    relay_key: &SigningKey,
) -> [u8; RELAY_KEEPALIVE_RESPONSE_LENGTH] {
    let mut encoded = [0_u8; RELAY_KEEPALIVE_RESPONSE_LENGTH];
    encode_common(
        &mut encoded,
        RELAY_KEEPALIVE_RESPONSE_TYPE,
        RELAY_KEEPALIVE_RESPONSE_LENGTH,
        0,
    );
    encoded[16..32].copy_from_slice(&response.network_id);
    encoded[32..48].copy_from_slice(&response.relay_id);
    encoded[48..64].copy_from_slice(&response.node_id);
    encoded[64..80].copy_from_slice(&response.lease_id);
    encoded[80..88].copy_from_slice(&response.server_time.to_be_bytes());
    let signature = sign_bytes(RELAY_KEEPALIVE_RESPONSE_DOMAIN, &encoded[..88], relay_key);
    encoded[88..152].copy_from_slice(&signature);
    encoded
}

/// Verifies a Relay-signed keepalive acknowledgement for an active lease.
///
/// # Errors
///
/// Returns [`InvalidProtocolMessage`] for malformed framing, an invalid signature, mismatched
/// identifiers, a zero lease, or a stale server timestamp.
pub fn verify_relay_keepalive_response(
    encoded: &[u8],
    relay_key: &VerifyingKey,
    expected_network_id: [u8; 16],
    expected_relay_id: [u8; 16],
    expected_node_id: [u8; 16],
    expected_lease_id: [u8; 16],
    now: u64,
) -> Result<RelayKeepaliveResponse, InvalidProtocolMessage> {
    if encoded.len() != RELAY_KEEPALIVE_RESPONSE_LENGTH
        || !valid_zero_payload_message(
            encoded,
            RELAY_KEEPALIVE_RESPONSE_TYPE,
            RELAY_KEEPALIVE_RESPONSE_LENGTH,
        )
    {
        return Err(InvalidProtocolMessage);
    }
    verify_signature(
        RELAY_KEEPALIVE_RESPONSE_DOMAIN,
        &encoded[..88],
        &encoded[88..152],
        relay_key,
    )?;
    let response = RelayKeepaliveResponse {
        network_id: array(&encoded[16..32])?,
        relay_id: array(&encoded[32..48])?,
        node_id: array(&encoded[48..64])?,
        lease_id: array(&encoded[64..80])?,
        server_time: u64::from_be_bytes(array(&encoded[80..88])?),
    };
    if response.network_id != expected_network_id
        || response.relay_id != expected_relay_id
        || response.node_id != expected_node_id
        || response.lease_id != expected_lease_id
        || response.lease_id == [0_u8; 16]
        || !time_is_fresh(response.server_time, now)
    {
        return Err(InvalidProtocolMessage);
    }
    Ok(response)
}

#[must_use]
pub fn encode_relay_keepalive(
    network_id: [u8; 16],
    relay_id: [u8; 16],
    node_id: [u8; 16],
    lease_id: [u8; 16],
    sequence: u64,
) -> [u8; RELAY_FRAME_HEADER_LENGTH] {
    debug_assert!(network_id != [0_u8; 16]);
    debug_assert!(relay_id != [0_u8; 16]);
    debug_assert!(node_id != [0_u8; 16]);
    debug_assert!(lease_id != [0_u8; 16]);
    let mut encoded = [0_u8; RELAY_FRAME_HEADER_LENGTH];
    encode_common(
        &mut encoded,
        RELAY_KEEPALIVE_TYPE,
        RELAY_FRAME_HEADER_LENGTH,
        0,
    );
    encoded[16..32].copy_from_slice(&network_id);
    encoded[32..48].copy_from_slice(&relay_id);
    encoded[48..64].copy_from_slice(&node_id);
    encoded[64..80].copy_from_slice(&[0_u8; 16]);
    encoded[80..96].copy_from_slice(&lease_id);
    encoded[96..104].copy_from_slice(&sequence.to_be_bytes());
    encoded
}

/// Parses a canonical Relay keepalive for subsequent endpoint, lease, and replay validation.
///
/// # Errors
///
/// Returns [`InvalidProtocolMessage`] for malformed framing, nonzero payload or reserved
/// destination fields, or zero identifiers.
pub fn parse_relay_keepalive(encoded: &[u8]) -> Result<RelayFrame<'_>, InvalidProtocolMessage> {
    if encoded.len() != RELAY_FRAME_HEADER_LENGTH
        || !valid_zero_payload_message(encoded, RELAY_KEEPALIVE_TYPE, RELAY_FRAME_HEADER_LENGTH)
    {
        return Err(InvalidProtocolMessage);
    }
    let frame = RelayFrame {
        network_id: array(&encoded[16..32])?,
        relay_id: array(&encoded[32..48])?,
        source_node_id: array(&encoded[48..64])?,
        destination_node_id: [0_u8; 16],
        lease_id: array(&encoded[80..96])?,
        sequence: u64::from_be_bytes(array(&encoded[96..104])?),
        payload: &[],
    };
    if frame.network_id == [0_u8; 16]
        || frame.relay_id == [0_u8; 16]
        || frame.source_node_id == [0_u8; 16]
        || frame.lease_id == [0_u8; 16]
        || encoded[64..80] != [0_u8; 16]
    {
        return Err(InvalidProtocolMessage);
    }
    Ok(frame)
}

fn encode_common(encoded: &mut [u8], message_type: u8, header_length: usize, payload_length: u16) {
    encoded[0..4].copy_from_slice(RELAY_MAGIC);
    encoded[4] = RELAY_VERSION;
    encoded[5] = message_type;
    encoded[6..8].copy_from_slice(&0_u16.to_be_bytes());
    encoded[8..10].copy_from_slice(
        &u16::try_from(header_length)
            .expect("relay header length is statically bounded")
            .to_be_bytes(),
    );
    encoded[10..12].copy_from_slice(&payload_length.to_be_bytes());
    encoded[12..16].copy_from_slice(&[0_u8; 4]);
}

fn valid_common(encoded: &[u8], message_type: u8, header_length: usize) -> bool {
    encoded.len() >= COMMON_HEADER_LENGTH
        && &encoded[0..4] == RELAY_MAGIC
        && encoded[4] == RELAY_VERSION
        && encoded[5] == message_type
        && encoded[6..8] == [0_u8; 2]
        && usize::from(u16::from_be_bytes([encoded[8], encoded[9]])) == header_length
        && encoded[12..16] == [0_u8; 4]
}

fn valid_zero_payload_message(encoded: &[u8], message_type: u8, header_length: usize) -> bool {
    valid_common(encoded, message_type, header_length) && encoded[10..12] == [0_u8; 2]
}

fn sign_bytes(domain: &[u8], encoded: &[u8], signing_key: &SigningKey) -> [u8; 64] {
    let mut input = Vec::with_capacity(domain.len() + encoded.len());
    input.extend_from_slice(domain);
    input.extend_from_slice(encoded);
    signing_key.sign(&input).to_bytes()
}

fn verify_signature(
    domain: &[u8],
    encoded: &[u8],
    signature: &[u8],
    verifying_key: &VerifyingKey,
) -> Result<(), InvalidProtocolMessage> {
    let signature: [u8; 64] = signature.try_into().map_err(|_| InvalidProtocolMessage)?;
    let mut input = Vec::with_capacity(domain.len() + encoded.len());
    input.extend_from_slice(domain);
    input.extend_from_slice(encoded);
    verifying_key
        .verify_strict(&input, &Signature::from_bytes(&signature))
        .map_err(|_| InvalidProtocolMessage)
}

fn array<const LENGTH: usize>(encoded: &[u8]) -> Result<[u8; LENGTH], InvalidProtocolMessage> {
    encoded.try_into().map_err(|_| InvalidProtocolMessage)
}

fn time_is_fresh(value: u64, now: u64) -> bool {
    value.abs_diff(now) <= 300
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn request_fixture() -> (RelayRegisterRequest, SigningKey, SigningKey) {
        let identity_key = SigningKey::from_bytes(&[11_u8; 32]);
        let controller_key = SigningKey::from_bytes(&[12_u8; 32]);
        let credential = crate::sign_credential(
            crate::CredentialClaims {
                network_id: [21_u8; 16],
                identity_public_key: identity_key.verifying_key().to_bytes(),
                virtual_ipv4: Ipv4Addr::new(100, 127, 0, 2),
                serial: 7,
                not_before: 1_700_000_000,
                not_after: 1_700_010_000,
                role_bitmap: 1,
                role_set_digest: [31_u8; 32],
            },
            &controller_key,
        );
        (
            RelayRegisterRequest {
                network_id: [21_u8; 16],
                node_id: crate::node_id(&identity_key.verifying_key().to_bytes()),
                relay_id: [41_u8; 16],
                request_id: [51_u8; 16],
                client_time: 1_700_000_100,
                credential,
            },
            identity_key,
            controller_key,
        )
    }

    #[test]
    fn registration_is_fixed_length_and_signed() {
        let (request, identity_key, controller_key) = request_fixture();
        let encoded = sign_relay_register_request(request, &identity_key);
        let verified =
            verify_relay_register_request(&encoded, &controller_key.verifying_key(), 1_700_000_101)
                .expect("registration verifies");
        assert_eq!(verified.request, request);
        assert_eq!(encoded.len(), RELAY_REGISTER_REQUEST_LENGTH);
    }

    #[test]
    fn registration_tampering_and_wrong_framing_fail() {
        let (request, identity_key, controller_key) = request_fixture();
        let encoded = sign_relay_register_request(request, &identity_key);
        for offset in [0, 5, 10, 16, 88, 288] {
            let mut tampered = encoded.to_vec();
            tampered[offset] ^= 1;
            assert!(
                verify_relay_register_request(
                    &tampered,
                    &controller_key.verifying_key(),
                    1_700_000_101
                )
                .is_err()
            );
        }
        assert!(
            verify_relay_register_request(
                &encoded[..encoded.len() - 1],
                &controller_key.verifying_key(),
                1_700_000_101
            )
            .is_err()
        );
    }

    #[test]
    fn response_requires_short_nonzero_lease_and_exact_framing() {
        let relay_key = SigningKey::from_bytes(&[13_u8; 32]);
        let response = RelayRegisterResponse {
            network_id: [21_u8; 16],
            node_id: [31_u8; 16],
            relay_id: [41_u8; 16],
            request_id: [51_u8; 16],
            lease_id: [61_u8; 16],
            expires_at: 1_700_000_300,
        };
        let encoded = sign_relay_register_response(response, &relay_key);
        assert_eq!(
            verify_relay_register_response(
                &encoded,
                &relay_key.verifying_key(),
                response.network_id,
                response.node_id,
                response.relay_id,
                response.request_id,
                1_700_000_100,
            ),
            Ok(response)
        );

        let mut nonzero_payload = encoded;
        nonzero_payload[11] = 1;
        assert!(
            verify_relay_register_response(
                &nonzero_payload,
                &relay_key.verifying_key(),
                response.network_id,
                response.node_id,
                response.relay_id,
                response.request_id,
                1_700_000_100,
            )
            .is_err()
        );

        let long_lease = sign_relay_register_response(
            RelayRegisterResponse {
                expires_at: 1_700_000_401,
                ..response
            },
            &relay_key,
        );
        assert!(
            verify_relay_register_response(
                &long_lease,
                &relay_key.verifying_key(),
                response.network_id,
                response.node_id,
                response.relay_id,
                response.request_id,
                1_700_000_100,
            )
            .is_err()
        );
    }

    #[test]
    fn relay_frame_round_trip_bounds_payload() {
        let payload = client_hello_fixture();
        let network_id = array::<16>(&payload[16..32]).expect("network ID");
        let source_node_id = array::<16>(&payload[32..48]).expect("source node ID");
        let destination_node_id = array::<16>(&payload[48..64]).expect("destination node ID");
        let encoded = encode_relay_frame(RelayFrame {
            network_id,
            relay_id: [2_u8; 16],
            source_node_id,
            destination_node_id,
            lease_id: [5_u8; 16],
            sequence: 8,
            payload: &payload,
        })
        .expect("frame encodes");
        let parsed = parse_relay_frame(&encoded).expect("frame parses");
        assert_eq!(parsed.payload, payload);
        let mut mismatched_route = encoded.clone();
        mismatched_route[48] ^= 1;
        assert!(parse_relay_frame(&mismatched_route).is_err());
        assert!(
            encode_relay_frame(RelayFrame {
                network_id: [1_u8; 16],
                relay_id: [2_u8; 16],
                source_node_id: [3_u8; 16],
                destination_node_id: [4_u8; 16],
                lease_id: [5_u8; 16],
                sequence: 0,
                payload: &[0_u8; RELAY_MAX_FRAME_LENGTH],
            })
            .is_err()
        );
        assert!(
            encode_relay_frame(RelayFrame {
                network_id: [1_u8; 16],
                relay_id: [2_u8; 16],
                source_node_id: [3_u8; 16],
                destination_node_id: [4_u8; 16],
                lease_id: [5_u8; 16],
                sequence: 0,
                payload: &[],
            })
            .is_err()
        );
    }

    fn client_hello_fixture() -> Vec<u8> {
        let vector: serde_json::Value =
            serde_json::from_str(include_str!("../../../tests/vectors/xsp1/session-v1.json"))
                .expect("valid session vector");
        let encoded = vector["client_hello_hex"].as_str().expect("client hello");
        (0..encoded.len())
            .step_by(2)
            .map(|index| {
                u8::from_str_radix(&encoded[index..index + 2], 16).expect("valid hexadecimal")
            })
            .collect()
    }

    #[test]
    fn keepalive_requires_zero_payload_and_reserved_destination() {
        let encoded = encode_relay_keepalive([1_u8; 16], [2_u8; 16], [3_u8; 16], [4_u8; 16], 5);
        assert!(parse_relay_keepalive(&encoded).is_ok());

        let mut payload = encoded;
        payload[11] = 1;
        assert!(parse_relay_keepalive(&payload).is_err());

        let mut destination = encoded;
        destination[64] = 1;
        assert!(parse_relay_keepalive(&destination).is_err());
    }

    #[test]
    fn signature_domains_are_not_interchangeable() {
        let signing_key = SigningKey::from_bytes(&[14_u8; 32]);
        let message = [15_u8; 32];
        let signature = sign_bytes(RELAY_REGISTER_REQUEST_DOMAIN, &message, &signing_key);

        assert!(
            verify_signature(
                RELAY_REGISTER_RESPONSE_DOMAIN,
                &message,
                &signature,
                &signing_key.verifying_key(),
            )
            .is_err()
        );
    }
}
