#![forbid(unsafe_code)]

mod data;
mod discovery;
mod handshake;
mod relay;

pub use data::{
    DATA_HEADER_LENGTH, DATA_TAG_LENGTH, DataFlags, DataHeader, DataReceiver, DataSender,
    MAX_DATAGRAM_LENGTH, MAX_ENCRYPTED_PAYLOAD_LENGTH, OpenedPacket, PacketType,
    key_update_payload, verify_key_update_payload,
};
pub use discovery::{
    DISCOVERY_REQUEST_LENGTH, DISCOVERY_RESPONSE_LENGTH, DiscoveryRequest,
    VerifiedDiscoveryRequest, VerifiedDiscoveryResponse, discovery_response,
    verify_discovery_request, verify_discovery_response,
};
pub use handshake::{
    CLIENT_FINISH_TYPE, CLIENT_HELLO_TYPE, ClientFinishSent, ClientHandshakeParameters,
    ClientHelloSent, EphemeralPrivateKey, EstablishedSession, HandshakeContext, SERVER_FINISH_TYPE,
    SERVER_HELLO_TYPE, ServerHandshakeParameters, ServerHelloSent,
};
pub use relay::{
    RELAY_DATA_TYPE, RELAY_FRAME_HEADER_LENGTH, RELAY_KEEPALIVE_RESPONSE_LENGTH,
    RELAY_KEEPALIVE_RESPONSE_TYPE, RELAY_KEEPALIVE_TYPE, RELAY_MAX_FRAME_LENGTH,
    RELAY_MAX_LEASE_SECONDS, RELAY_REGISTER_REQUEST_LENGTH, RELAY_REGISTER_REQUEST_TYPE,
    RELAY_REGISTER_RESPONSE_LENGTH, RELAY_REGISTER_RESPONSE_TYPE, RelayFrame,
    RelayKeepaliveResponse, RelayRegisterRequest, RelayRegisterResponse,
    VerifiedRelayRegisterRequest, encode_relay_frame, encode_relay_keepalive, parse_relay_frame,
    parse_relay_keepalive, sign_relay_keepalive_response, sign_relay_register_request,
    sign_relay_register_response, validate_relay_payload, verify_relay_keepalive_response,
    verify_relay_register_request, verify_relay_register_response,
};

use std::net::Ipv4Addr;

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const CREDENTIAL_LENGTH: usize = 200;
pub const CREDENTIAL_SIGNED_LENGTH: usize = 136;
pub const CREDENTIAL_DOMAIN: &[u8] = b"XSP/1 credential v1";
pub const NODE_ID_DOMAIN: &[u8] = b"XSP/1 node id v1";
pub const ROLE_SET_DOMAIN: &[u8] = b"XSP/1 role set v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CredentialClaims {
    pub network_id: [u8; 16],
    pub identity_public_key: [u8; 32],
    pub virtual_ipv4: Ipv4Addr,
    pub serial: u64,
    pub not_before: u64,
    pub not_after: u64,
    pub role_bitmap: u32,
    pub role_set_digest: [u8; 32],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VerifiedCredential {
    pub network_id: [u8; 16],
    pub node_id: [u8; 16],
    pub identity_public_key: [u8; 32],
    pub virtual_ipv4: Ipv4Addr,
    pub serial: u64,
    pub not_before: u64,
    pub not_after: u64,
    pub role_bitmap: u32,
    pub role_set_digest: [u8; 32],
    pub controller_key_id: u32,
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error("invalid credential")]
pub struct InvalidCredential;

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error("invalid role tag")]
pub struct InvalidRoleTag;

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error("invalid protocol message")]
pub struct InvalidProtocolMessage;

#[must_use]
pub fn node_id(identity_public_key: &[u8; 32]) -> [u8; 16] {
    let mut hash = Sha256::new();
    hash.update(NODE_ID_DOMAIN);
    hash.update(identity_public_key);
    let digest = hash.finalize();

    let mut identifier = [0_u8; 16];
    identifier.copy_from_slice(&digest[..16]);
    identifier
}

#[must_use]
pub fn controller_key_id(verifying_key: &VerifyingKey) -> u32 {
    let digest = Sha256::digest(verifying_key.as_bytes());
    u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]])
}

/// Computes the canonical digest for a role bitmap and validated role tags.
///
/// # Errors
///
/// Returns [`InvalidRoleTag`] when tag count, syntax, length, or uniqueness is invalid.
pub fn role_set_digest(role_bitmap: u32, tags: &[String]) -> Result<[u8; 32], InvalidRoleTag> {
    if tags.len() > 32 {
        return Err(InvalidRoleTag);
    }

    let mut ordered = tags.iter().map(String::as_str).collect::<Vec<_>>();
    ordered.sort_unstable();

    if ordered.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(InvalidRoleTag);
    }

    let mut hash = Sha256::new();
    hash.update(ROLE_SET_DOMAIN);
    hash.update(role_bitmap.to_be_bytes());
    hash.update(
        u16::try_from(ordered.len())
            .map_err(|_| InvalidRoleTag)?
            .to_be_bytes(),
    );

    for tag in ordered {
        if !valid_role_tag(tag) {
            return Err(InvalidRoleTag);
        }
        let bytes = tag.as_bytes();
        hash.update(
            u16::try_from(bytes.len())
                .map_err(|_| InvalidRoleTag)?
                .to_be_bytes(),
        );
        hash.update(bytes);
    }

    Ok(hash.finalize().into())
}

fn valid_role_tag(tag: &str) -> bool {
    let bytes = tag.as_bytes();
    if bytes.is_empty() || bytes.len() > 63 || !bytes[0].is_ascii_lowercase() {
        return false;
    }

    bytes.iter().all(|byte| {
        byte.is_ascii_lowercase()
            || byte.is_ascii_digit()
            || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/')
    })
}

#[must_use]
pub fn sign_credential(claims: CredentialClaims, signing_key: &SigningKey) -> [u8; 200] {
    let mut credential = [0_u8; CREDENTIAL_LENGTH];
    credential[0] = 1;
    credential[4..20].copy_from_slice(&claims.network_id);

    let derived_node_id = node_id(&claims.identity_public_key);
    credential[20..36].copy_from_slice(&derived_node_id);
    credential[36..68].copy_from_slice(&claims.identity_public_key);
    credential[68..72].copy_from_slice(&claims.virtual_ipv4.octets());
    credential[72..80].copy_from_slice(&claims.serial.to_be_bytes());
    credential[80..88].copy_from_slice(&claims.not_before.to_be_bytes());
    credential[88..96].copy_from_slice(&claims.not_after.to_be_bytes());
    credential[96..100].copy_from_slice(&claims.role_bitmap.to_be_bytes());
    credential[100..132].copy_from_slice(&claims.role_set_digest);
    credential[132..136]
        .copy_from_slice(&controller_key_id(&signing_key.verifying_key()).to_be_bytes());

    let mut signing_input = Vec::with_capacity(CREDENTIAL_DOMAIN.len() + CREDENTIAL_SIGNED_LENGTH);
    signing_input.extend_from_slice(CREDENTIAL_DOMAIN);
    signing_input.extend_from_slice(&credential[..CREDENTIAL_SIGNED_LENGTH]);
    let signature = signing_key.sign(&signing_input);
    credential[CREDENTIAL_SIGNED_LENGTH..].copy_from_slice(&signature.to_bytes());
    credential
}

/// Validates a complete credential without exposing the rejection reason.
///
/// # Errors
///
/// Returns [`InvalidCredential`] for any encoding, identity, time, key, or signature failure.
pub fn verify_credential(
    credential: &[u8],
    verifying_key: &VerifyingKey,
    now: u64,
) -> Result<VerifiedCredential, InvalidCredential> {
    let bytes: &[u8; CREDENTIAL_LENGTH] = credential.try_into().map_err(|_| InvalidCredential)?;

    if bytes[0] != 1 || bytes[1..4] != [0_u8; 3] {
        return Err(InvalidCredential);
    }

    let network_id: [u8; 16] = bytes[4..20].try_into().map_err(|_| InvalidCredential)?;
    let encoded_node_id: [u8; 16] = bytes[20..36].try_into().map_err(|_| InvalidCredential)?;
    let identity_public_key: [u8; 32] = bytes[36..68].try_into().map_err(|_| InvalidCredential)?;
    let virtual_ipv4 =
        Ipv4Addr::from(<[u8; 4]>::try_from(&bytes[68..72]).map_err(|_| InvalidCredential)?);
    let serial = read_u64(&bytes[72..80])?;
    let not_before = read_u64(&bytes[80..88])?;
    let not_after = read_u64(&bytes[88..96])?;
    let role_bitmap = read_u32(&bytes[96..100])?;
    let role_digest: [u8; 32] = bytes[100..132].try_into().map_err(|_| InvalidCredential)?;
    let encoded_key_id = read_u32(&bytes[132..136])?;

    if network_id == [0_u8; 16]
        || encoded_node_id != node_id(&identity_public_key)
        || serial == 0
        || not_before > now
        || now >= not_after
        || not_after <= not_before
        || virtual_ipv4.is_unspecified()
        || virtual_ipv4.is_multicast()
        || virtual_ipv4 == Ipv4Addr::BROADCAST
        || encoded_key_id != controller_key_id(verifying_key)
    {
        return Err(InvalidCredential);
    }

    let signature = Signature::from_bytes(
        &bytes[CREDENTIAL_SIGNED_LENGTH..]
            .try_into()
            .map_err(|_| InvalidCredential)?,
    );
    let mut signing_input = Vec::with_capacity(CREDENTIAL_DOMAIN.len() + CREDENTIAL_SIGNED_LENGTH);
    signing_input.extend_from_slice(CREDENTIAL_DOMAIN);
    signing_input.extend_from_slice(&bytes[..CREDENTIAL_SIGNED_LENGTH]);
    verifying_key
        .verify_strict(&signing_input, &signature)
        .map_err(|_| InvalidCredential)?;

    Ok(VerifiedCredential {
        network_id,
        node_id: encoded_node_id,
        identity_public_key,
        virtual_ipv4,
        serial,
        not_before,
        not_after,
        role_bitmap,
        role_set_digest: role_digest,
        controller_key_id: encoded_key_id,
    })
}

fn read_u64(bytes: &[u8]) -> Result<u64, InvalidCredential> {
    Ok(u64::from_be_bytes(
        bytes.try_into().map_err(|_| InvalidCredential)?,
    ))
}

fn read_u32(bytes: &[u8]) -> Result<u32, InvalidCredential> {
    Ok(u32::from_be_bytes(
        bytes.try_into().map_err(|_| InvalidCredential)?,
    ))
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use super::*;

    fn fixture() -> (CredentialClaims, SigningKey) {
        let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
        let identity_key = SigningKey::from_bytes(&[9_u8; 32]);
        let tags = vec!["gateway".to_owned(), "linux".to_owned()];
        let claims = CredentialClaims {
            network_id: [3_u8; 16],
            identity_public_key: identity_key.verifying_key().to_bytes(),
            virtual_ipv4: Ipv4Addr::new(100, 88, 0, 16),
            serial: 42,
            not_before: 1_700_000_000,
            not_after: 1_700_086_400,
            role_bitmap: 5,
            role_set_digest: role_set_digest(5, &tags).expect("valid tags"),
        };
        (claims, signing_key)
    }

    #[test]
    fn credential_is_exactly_200_bytes_and_verifies() {
        let (claims, signing_key) = fixture();
        let encoded = sign_credential(claims, &signing_key);
        let verified = verify_credential(&encoded, &signing_key.verifying_key(), 1_700_000_001)
            .expect("credential verifies");

        assert_eq!(encoded.len(), CREDENTIAL_LENGTH);
        assert_eq!(verified.network_id, claims.network_id);
        assert_eq!(verified.identity_public_key, claims.identity_public_key);
        assert_eq!(verified.virtual_ipv4, claims.virtual_ipv4);
        assert_eq!(verified.serial, claims.serial);
    }

    #[test]
    fn credential_rejects_every_single_byte_tamper() {
        let (claims, signing_key) = fixture();
        let encoded = sign_credential(claims, &signing_key);

        for index in 0..encoded.len() {
            let mut tampered = encoded;
            tampered[index] ^= 1;
            assert_eq!(
                verify_credential(&tampered, &signing_key.verifying_key(), 1_700_000_001),
                Err(InvalidCredential),
                "tamper at byte {index} was accepted"
            );
        }
    }

    #[test]
    fn credential_rejects_wrong_key_and_time_bounds() {
        let (claims, signing_key) = fixture();
        let encoded = sign_credential(claims, &signing_key);
        let other_key = SigningKey::from_bytes(&[8_u8; 32]);

        assert_eq!(
            verify_credential(&encoded, &other_key.verifying_key(), 1_700_000_001),
            Err(InvalidCredential)
        );
        assert_eq!(
            verify_credential(
                &encoded,
                &signing_key.verifying_key(),
                claims.not_before - 1
            ),
            Err(InvalidCredential)
        );
        assert_eq!(
            verify_credential(&encoded, &signing_key.verifying_key(), claims.not_after),
            Err(InvalidCredential)
        );
    }

    #[test]
    fn checked_in_credential_vector_matches_implementation() {
        let vector: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/vectors/xsp1/credential-v1.json"
        ))
        .expect("valid credential vector JSON");
        let credential_signing_seed = decode_hex_array::<32>(
            vector["credential_signing_seed_hex"]
                .as_str()
                .expect("credential seed"),
        );
        let identity_signing_seed = decode_hex_array::<32>(
            vector["identity_signing_seed_hex"]
                .as_str()
                .expect("identity seed"),
        );
        let signing_key = SigningKey::from_bytes(&credential_signing_seed);
        let identity_key = SigningKey::from_bytes(&identity_signing_seed);
        let tags = vector["tags"]
            .as_array()
            .expect("tags")
            .iter()
            .map(|tag| tag.as_str().expect("tag string").to_owned())
            .collect::<Vec<_>>();
        let claims = CredentialClaims {
            network_id: decode_hex_array::<16>(
                vector["network_id_hex"].as_str().expect("network ID"),
            ),
            identity_public_key: identity_key.verifying_key().to_bytes(),
            virtual_ipv4: vector["virtual_ipv4"]
                .as_str()
                .expect("virtual IP")
                .parse()
                .expect("valid virtual IP"),
            serial: vector["serial"].as_u64().expect("serial"),
            not_before: vector["not_before"].as_u64().expect("not before"),
            not_after: vector["not_after"].as_u64().expect("not after"),
            role_bitmap: u32::try_from(vector["role_bitmap"].as_u64().expect("role bitmap"))
                .expect("32-bit role bitmap"),
            role_set_digest: role_set_digest(
                u32::try_from(vector["role_bitmap"].as_u64().expect("role bitmap"))
                    .expect("32-bit role bitmap"),
                &tags,
            )
            .expect("valid role set"),
        };
        let credential = sign_credential(claims, &signing_key);

        assert_eq!(
            hexadecimal(&credential),
            vector["credential_hex"].as_str().expect("credential")
        );
        assert_eq!(
            controller_key_id(&signing_key.verifying_key()),
            u32::try_from(vector["controller_key_id"].as_u64().expect("key ID"))
                .expect("32-bit key ID")
        );
        assert_eq!(
            hexadecimal(&Sha256::digest(credential)),
            vector["credential_sha256"]
                .as_str()
                .expect("credential hash")
        );
        verify_credential(
            &credential,
            &signing_key.verifying_key(),
            claims.not_before + 1,
        )
        .expect("checked-in credential verifies");
    }

    fn decode_hex_array<const LENGTH: usize>(encoded: &str) -> [u8; LENGTH] {
        let decoded = (0..encoded.len())
            .step_by(2)
            .map(|index| {
                u8::from_str_radix(&encoded[index..index + 2], 16).expect("valid hexadecimal")
            })
            .collect::<Vec<_>>();
        decoded
            .try_into()
            .unwrap_or_else(|_| panic!("hexadecimal value must contain exactly {LENGTH} bytes"))
    }

    fn hexadecimal(bytes: &[u8]) -> String {
        let mut encoded = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            write!(&mut encoded, "{byte:02x}").expect("writing to a string cannot fail");
        }
        encoded
    }

    #[test]
    fn role_digest_is_order_independent_but_rejects_ambiguous_tags() {
        let first = vec!["linux".to_owned(), "gateway".to_owned()];
        let second = vec!["gateway".to_owned(), "linux".to_owned()];

        assert_eq!(
            role_set_digest(1, &first).expect("valid"),
            role_set_digest(1, &second).expect("valid")
        );
        assert_eq!(
            role_set_digest(1, &["linux".to_owned(), "linux".to_owned()]),
            Err(InvalidRoleTag)
        );
        assert_eq!(
            role_set_digest(1, &["UPPER".to_owned()]),
            Err(InvalidRoleTag)
        );
    }
}
