use std::{fmt, net::Ipv4Addr};

use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce, Tag, aead::AeadInPlace};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use hkdf::Hkdf;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use x25519_dalek::{X25519_BASEPOINT_BYTES, x25519};
use zeroize::Zeroizing;

use crate::{
    CREDENTIAL_LENGTH, InvalidProtocolMessage, VerifiedCredential,
    data::{DataReceiver, DataSender},
    verify_credential,
};

const MAGIC: &[u8; 4] = b"XSP1";
const VERSION: u8 = 1;
const SUITE_ID: u16 = 1;
const COMMON_HEADER_LENGTH: usize = 16;
pub(crate) const CLIENT_HELLO_BODY_LENGTH: usize = 389;
pub(crate) const SERVER_HELLO_BODY_LENGTH: usize = 468;
pub(crate) const FINISH_BODY_LENGTH: usize = 76;
pub const CLIENT_HELLO_TYPE: u8 = 0x10;
pub const SERVER_HELLO_TYPE: u8 = 0x11;
pub const CLIENT_FINISH_TYPE: u8 = 0x12;
pub const SERVER_FINISH_TYPE: u8 = 0x13;
const CLIENT_AUTH_DOMAIN: &[u8] = b"XSP/1 client auth v1";
const SERVER_AUTH_DOMAIN: &[u8] = b"XSP/1 server auth v1";
const HELLO_TRANSCRIPT_DOMAIN: &[u8] = b"XSP/1 hello transcript v1";
const SERVER_FINISH_DOMAIN: &[u8] = b"XSP/1 server finish v1";
const FULL_TRANSCRIPT_DOMAIN: &[u8] = b"XSP/1 full transcript v1";
const HANDSHAKE_SALT_DOMAIN: &[u8] = b"XSP/1 handshake salt v1";
const CLIENT_HANDSHAKE_KEY_DOMAIN: &[u8] = b"XSP/1 client handshake key v1";
const SERVER_HANDSHAKE_KEY_DOMAIN: &[u8] = b"XSP/1 server handshake key v1";
const CLIENT_HANDSHAKE_NONCE_DOMAIN: &[u8] = b"XSP/1 client handshake nonce v1";
const SERVER_HANDSHAKE_NONCE_DOMAIN: &[u8] = b"XSP/1 server handshake nonce v1";
const CLIENT_TRAFFIC_SECRET_DOMAIN: &[u8] = b"XSP/1 client traffic secret v1";
const SERVER_TRAFFIC_SECRET_DOMAIN: &[u8] = b"XSP/1 server traffic secret v1";
const MAX_CLOCK_SKEW_SECONDS: u64 = 300;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HandshakeContext {
    pub network_id: [u8; 16],
    pub local_node_id: [u8; 16],
    pub local_virtual_ip: Ipv4Addr,
    pub peer_node_id: [u8; 16],
    pub peer_virtual_ip: Ipv4Addr,
}

pub struct EphemeralPrivateKey(Zeroizing<[u8; 32]>);

impl EphemeralPrivateKey {
    #[must_use]
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    #[must_use]
    pub fn public_key(&self) -> [u8; 32] {
        x25519(*self.0, X25519_BASEPOINT_BYTES)
    }

    fn shared_secret(
        self,
        peer_public_key: [u8; 32],
    ) -> Result<Zeroizing<[u8; 32]>, InvalidProtocolMessage> {
        let shared = x25519(*self.0, peer_public_key);
        if bool::from(shared.ct_eq(&[0_u8; 32])) {
            return Err(InvalidProtocolMessage);
        }
        Ok(Zeroizing::new(shared))
    }
}

impl fmt::Debug for EphemeralPrivateKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("EphemeralPrivateKey([redacted])")
    }
}

pub struct ClientHandshakeParameters {
    pub context: HandshakeContext,
    pub credential: [u8; CREDENTIAL_LENGTH],
    pub ephemeral_private_key: EphemeralPrivateKey,
    pub client_nonce: [u8; 32],
    pub message_id: u32,
    pub client_time: u64,
}

pub struct ServerHandshakeParameters {
    pub context: HandshakeContext,
    pub credential: [u8; CREDENTIAL_LENGTH],
    pub ephemeral_private_key: EphemeralPrivateKey,
    pub server_nonce: [u8; 32],
    pub session_id: [u8; 16],
    pub server_time: u64,
}

pub struct ClientHelloSent {
    context: HandshakeContext,
    encoded: Vec<u8>,
    ephemeral_private_key: EphemeralPrivateKey,
    client_nonce: [u8; 32],
}

pub struct ClientFinishSent {
    context: HandshakeContext,
    encoded: Vec<u8>,
    client_hello: Vec<u8>,
    server_hello: Vec<u8>,
    message_id: u32,
    session_id: [u8; 16],
    keys: HandshakeKeys,
}

pub struct ServerHelloSent {
    context: HandshakeContext,
    encoded: Vec<u8>,
    client_hello: Vec<u8>,
    message_id: u32,
    session_id: [u8; 16],
    keys: HandshakeKeys,
}

pub struct EstablishedSession {
    role: SessionRole,
    context: HandshakeContext,
    session_id: [u8; 16],
    client_to_server_secret: Zeroizing<[u8; 32]>,
    server_to_client_secret: Zeroizing<[u8; 32]>,
}

#[derive(Clone, Copy)]
enum SessionRole {
    Client,
    Server,
}

struct HandshakeKeys {
    handshake_prk: Zeroizing<[u8; 32]>,
    client_key: Zeroizing<[u8; 32]>,
    server_key: Zeroizing<[u8; 32]>,
    client_nonce_salt: [u8; 4],
    server_nonce_salt: [u8; 4],
    hello_transcript_hash: [u8; 32],
}

struct ParsedClientHello {
    message_id: u32,
    client_nonce: [u8; 32],
    ephemeral_public_key: [u8; 32],
}

struct ParsedServerHello {
    message_id: u32,
    server_nonce: [u8; 32],
    ephemeral_public_key: [u8; 32],
    session_id: [u8; 16],
}

impl ClientHelloSent {
    /// Creates and signs a canonical `ClientHello`.
    ///
    /// # Errors
    ///
    /// Returns `InvalidProtocolMessage` when local identity material or fixed fields are invalid.
    pub fn start(
        parameters: ClientHandshakeParameters,
        identity_signing_key: &SigningKey,
    ) -> Result<Self, InvalidProtocolMessage> {
        validate_context(parameters.context)?;
        validate_local_credential(
            &parameters.credential,
            parameters.context,
            identity_signing_key,
        )?;
        if parameters.message_id == 0
            || parameters.client_time == 0
            || parameters.client_nonce == [0_u8; 32]
        {
            return Err(InvalidProtocolMessage);
        }
        let ephemeral_public_key = parameters.ephemeral_private_key.public_key();
        if ephemeral_public_key == [0_u8; 32] {
            return Err(InvalidProtocolMessage);
        }

        let header = encode_common_header(
            CLIENT_HELLO_TYPE,
            CLIENT_HELLO_BODY_LENGTH,
            parameters.message_id,
        )?;
        let mut encoded = Vec::with_capacity(COMMON_HEADER_LENGTH + CLIENT_HELLO_BODY_LENGTH);
        encoded.extend_from_slice(&header);
        encoded.extend_from_slice(&parameters.context.network_id);
        encoded.extend_from_slice(&parameters.context.local_node_id);
        encoded.extend_from_slice(&parameters.context.peer_node_id);
        encoded.extend_from_slice(&parameters.client_nonce);
        encoded.extend_from_slice(&ephemeral_public_key);
        encoded.extend_from_slice(&parameters.client_time.to_be_bytes());
        encoded.extend_from_slice(
            &u16::try_from(CREDENTIAL_LENGTH)
                .map_err(|_| InvalidProtocolMessage)?
                .to_be_bytes(),
        );
        encoded.extend_from_slice(&parameters.credential);
        encoded.push(1);
        encoded.extend_from_slice(&SUITE_ID.to_be_bytes());

        let mut signing_input = Vec::with_capacity(CLIENT_AUTH_DOMAIN.len() + encoded.len());
        signing_input.extend_from_slice(CLIENT_AUTH_DOMAIN);
        signing_input.extend_from_slice(&encoded);
        encoded.extend_from_slice(&identity_signing_key.sign(&signing_input).to_bytes());
        if encoded.len() != COMMON_HEADER_LENGTH + CLIENT_HELLO_BODY_LENGTH {
            return Err(InvalidProtocolMessage);
        }

        Ok(Self {
            context: parameters.context,
            encoded,
            ephemeral_private_key: parameters.ephemeral_private_key,
            client_nonce: parameters.client_nonce,
        })
    }

    #[must_use]
    pub fn encoded(&self) -> &[u8] {
        &self.encoded
    }

    /// Validates `ServerHello` and produces the canonical `ClientFinish`.
    ///
    /// # Errors
    ///
    /// Returns `InvalidProtocolMessage` for framing, identity, credential, signature, time, or key failures.
    pub fn accept_server_hello(
        self,
        server_hello: &[u8],
        controller_verifying_key: &VerifyingKey,
        now: u64,
    ) -> Result<ClientFinishSent, InvalidProtocolMessage> {
        let parsed = parse_server_hello(
            server_hello,
            self.context,
            &self.encoded,
            self.client_nonce,
            controller_verifying_key,
            now,
        )?;
        let hello_transcript_hash = transcript_hash(
            HELLO_TRANSCRIPT_DOMAIN,
            &[self.encoded.as_slice(), server_hello],
        );
        let shared_secret = self
            .ephemeral_private_key
            .shared_secret(parsed.ephemeral_public_key)?;
        let keys = derive_handshake_keys(
            &shared_secret,
            self.context.network_id,
            self.client_nonce,
            parsed.server_nonce,
            hello_transcript_hash,
        )?;
        let client_finish = encode_finish(
            CLIENT_FINISH_TYPE,
            parsed.message_id,
            parsed.session_id,
            &keys.client_key,
            keys.client_nonce_salt,
            keys.hello_transcript_hash,
        )?;

        Ok(ClientFinishSent {
            context: self.context,
            encoded: client_finish,
            client_hello: self.encoded,
            server_hello: server_hello.to_vec(),
            message_id: parsed.message_id,
            session_id: parsed.session_id,
            keys,
        })
    }
}
impl ClientFinishSent {
    #[must_use]
    pub fn encoded(&self) -> &[u8] {
        &self.encoded
    }

    /// Validates `ServerFinish` and returns established directional traffic secrets.
    ///
    /// # Errors
    ///
    /// Returns `InvalidProtocolMessage` when the finish frame, tag, transcript, or session binding is invalid.
    pub fn accept_server_finish(
        self,
        server_finish: &[u8],
    ) -> Result<EstablishedSession, InvalidProtocolMessage> {
        let expected = transcript_hash(
            SERVER_FINISH_DOMAIN,
            &[
                self.client_hello.as_slice(),
                self.server_hello.as_slice(),
                self.encoded.as_slice(),
            ],
        );
        let plaintext = decode_finish(
            server_finish,
            SERVER_FINISH_TYPE,
            self.message_id,
            self.session_id,
            &self.keys.server_key,
            self.keys.server_nonce_salt,
        )?;
        if !bool::from(plaintext.ct_eq(&expected)) {
            return Err(InvalidProtocolMessage);
        }
        let full_hash = transcript_hash(
            FULL_TRANSCRIPT_DOMAIN,
            &[
                self.client_hello.as_slice(),
                self.server_hello.as_slice(),
                self.encoded.as_slice(),
                server_finish,
            ],
        );
        established_session(
            SessionRole::Client,
            self.context,
            self.session_id,
            &self.keys.handshake_prk,
            full_hash,
        )
    }
}

impl ServerHelloSent {
    /// Validates `ClientHello` and creates a signed canonical `ServerHello`.
    ///
    /// # Errors
    ///
    /// Returns `InvalidProtocolMessage` for framing, identity, credential, signature, time, or key failures.
    pub fn accept(
        client_hello: &[u8],
        parameters: ServerHandshakeParameters,
        identity_signing_key: &SigningKey,
        controller_verifying_key: &VerifyingKey,
        now: u64,
    ) -> Result<Self, InvalidProtocolMessage> {
        validate_context(parameters.context)?;
        validate_local_credential(
            &parameters.credential,
            parameters.context,
            identity_signing_key,
        )?;
        if parameters.server_nonce == [0_u8; 32]
            || parameters.session_id == [0_u8; 16]
            || parameters.server_time == 0
        {
            return Err(InvalidProtocolMessage);
        }
        let parsed = parse_client_hello(
            client_hello,
            parameters.context,
            controller_verifying_key,
            now,
        )?;
        let ephemeral_public_key = parameters.ephemeral_private_key.public_key();
        if ephemeral_public_key == [0_u8; 32] {
            return Err(InvalidProtocolMessage);
        }
        let client_hello_hash = Sha256::digest(client_hello);
        let header = encode_common_header(
            SERVER_HELLO_TYPE,
            SERVER_HELLO_BODY_LENGTH,
            parsed.message_id,
        )?;
        let mut encoded = Vec::with_capacity(COMMON_HEADER_LENGTH + SERVER_HELLO_BODY_LENGTH);
        encoded.extend_from_slice(&header);
        encoded.extend_from_slice(&parameters.context.network_id);
        encoded.extend_from_slice(&parameters.context.local_node_id);
        encoded.extend_from_slice(&parameters.context.peer_node_id);
        encoded.extend_from_slice(&parsed.client_nonce);
        encoded.extend_from_slice(&parameters.server_nonce);
        encoded.extend_from_slice(&ephemeral_public_key);
        encoded.extend_from_slice(&client_hello_hash);
        encoded.extend_from_slice(&parameters.session_id);
        encoded.extend_from_slice(&parameters.server_time.to_be_bytes());
        encoded.extend_from_slice(&SUITE_ID.to_be_bytes());
        encoded.extend_from_slice(
            &u16::try_from(CREDENTIAL_LENGTH)
                .map_err(|_| InvalidProtocolMessage)?
                .to_be_bytes(),
        );
        encoded.extend_from_slice(&parameters.credential);

        let mut signing_input =
            Vec::with_capacity(SERVER_AUTH_DOMAIN.len() + client_hello_hash.len() + encoded.len());
        signing_input.extend_from_slice(SERVER_AUTH_DOMAIN);
        signing_input.extend_from_slice(&client_hello_hash);
        signing_input.extend_from_slice(&encoded);
        encoded.extend_from_slice(&identity_signing_key.sign(&signing_input).to_bytes());
        if encoded.len() != COMMON_HEADER_LENGTH + SERVER_HELLO_BODY_LENGTH {
            return Err(InvalidProtocolMessage);
        }

        let hello_transcript_hash =
            transcript_hash(HELLO_TRANSCRIPT_DOMAIN, &[client_hello, encoded.as_slice()]);
        let shared_secret = parameters
            .ephemeral_private_key
            .shared_secret(parsed.ephemeral_public_key)?;
        let keys = derive_handshake_keys(
            &shared_secret,
            parameters.context.network_id,
            parsed.client_nonce,
            parameters.server_nonce,
            hello_transcript_hash,
        )?;

        Ok(Self {
            context: parameters.context,
            encoded,
            client_hello: client_hello.to_vec(),
            message_id: parsed.message_id,
            session_id: parameters.session_id,
            keys,
        })
    }

    #[must_use]
    pub fn encoded(&self) -> &[u8] {
        &self.encoded
    }

    /// Validates `ClientFinish`, creates `ServerFinish`, and returns the established server session.
    ///
    /// # Errors
    ///
    /// Returns `InvalidProtocolMessage` when the finish frame, tag, transcript, or session binding is invalid.
    pub fn accept_client_finish(
        self,
        client_finish: &[u8],
    ) -> Result<(EstablishedSession, Vec<u8>), InvalidProtocolMessage> {
        let plaintext = decode_finish(
            client_finish,
            CLIENT_FINISH_TYPE,
            self.message_id,
            self.session_id,
            &self.keys.client_key,
            self.keys.client_nonce_salt,
        )?;
        if !bool::from(plaintext.ct_eq(&self.keys.hello_transcript_hash)) {
            return Err(InvalidProtocolMessage);
        }
        let server_plaintext = transcript_hash(
            SERVER_FINISH_DOMAIN,
            &[
                self.client_hello.as_slice(),
                self.encoded.as_slice(),
                client_finish,
            ],
        );
        let server_finish = encode_finish(
            SERVER_FINISH_TYPE,
            self.message_id,
            self.session_id,
            &self.keys.server_key,
            self.keys.server_nonce_salt,
            server_plaintext,
        )?;
        let full_hash = transcript_hash(
            FULL_TRANSCRIPT_DOMAIN,
            &[
                self.client_hello.as_slice(),
                self.encoded.as_slice(),
                client_finish,
                server_finish.as_slice(),
            ],
        );
        let session = established_session(
            SessionRole::Server,
            self.context,
            self.session_id,
            &self.keys.handshake_prk,
            full_hash,
        )?;
        Ok((session, server_finish))
    }
}

impl EstablishedSession {
    #[must_use]
    pub fn session_id(&self) -> [u8; 16] {
        self.session_id
    }

    /// Consumes the handshake result and creates directional data ciphers.
    ///
    /// # Errors
    ///
    /// Returns `InvalidProtocolMessage` when the fixed epoch-zero key derivation fails.
    pub fn into_data_plane(self) -> Result<(DataSender, DataReceiver), InvalidProtocolMessage> {
        let pair = match self.role {
            SessionRole::Client => (
                DataSender::new(
                    self.client_to_server_secret,
                    self.context.network_id,
                    self.context.local_node_id,
                    self.context.peer_node_id,
                    self.session_id,
                    self.context.local_virtual_ip,
                    self.context.peer_virtual_ip,
                )?,
                DataReceiver::new(
                    self.server_to_client_secret,
                    self.context.network_id,
                    self.context.peer_node_id,
                    self.context.local_node_id,
                    self.session_id,
                    self.context.peer_virtual_ip,
                    self.context.local_virtual_ip,
                )?,
            ),
            SessionRole::Server => (
                DataSender::new(
                    self.server_to_client_secret,
                    self.context.network_id,
                    self.context.local_node_id,
                    self.context.peer_node_id,
                    self.session_id,
                    self.context.local_virtual_ip,
                    self.context.peer_virtual_ip,
                )?,
                DataReceiver::new(
                    self.client_to_server_secret,
                    self.context.network_id,
                    self.context.peer_node_id,
                    self.context.local_node_id,
                    self.session_id,
                    self.context.peer_virtual_ip,
                    self.context.local_virtual_ip,
                )?,
            ),
        };
        Ok(pair)
    }
}

impl fmt::Debug for EstablishedSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EstablishedSession")
            .field("session_id", &self.session_id)
            .field("secrets", &"[redacted]")
            .finish_non_exhaustive()
    }
}

fn parse_client_hello(
    encoded: &[u8],
    context: HandshakeContext,
    controller_verifying_key: &VerifyingKey,
    now: u64,
) -> Result<ParsedClientHello, InvalidProtocolMessage> {
    let message_id = parse_common_header(encoded, CLIENT_HELLO_TYPE, CLIENT_HELLO_BODY_LENGTH)?;
    let body = &encoded[COMMON_HEADER_LENGTH..];
    let network_id = array::<16>(&body[0..16])?;
    let source_node_id = array::<16>(&body[16..32])?;
    let destination_node_id = array::<16>(&body[32..48])?;
    let client_nonce = array::<32>(&body[48..80])?;
    let ephemeral_public_key = array::<32>(&body[80..112])?;
    let client_time = u64::from_be_bytes(array::<8>(&body[112..120])?);
    let credential_length = u16::from_be_bytes(array::<2>(&body[120..122])?);
    let credential = array::<CREDENTIAL_LENGTH>(&body[122..322])?;
    if network_id != context.network_id
        || source_node_id != context.peer_node_id
        || destination_node_id != context.local_node_id
        || client_nonce == [0_u8; 32]
        || ephemeral_public_key == [0_u8; 32]
        || credential_length
            != u16::try_from(CREDENTIAL_LENGTH).map_err(|_| InvalidProtocolMessage)?
        || body[322] != 1
        || u16::from_be_bytes(array::<2>(&body[323..325])?) != SUITE_ID
        || client_time.abs_diff(now) > MAX_CLOCK_SKEW_SECONDS
    {
        return Err(InvalidProtocolMessage);
    }
    verify_peer_credential(&credential, context, controller_verifying_key, now)?;
    let signature = Signature::from_bytes(&array::<64>(&body[325..389])?);
    let peer_key = VerifyingKey::from_bytes(&array::<32>(&credential[36..68])?)
        .map_err(|_| InvalidProtocolMessage)?;
    let mut signing_input = Vec::with_capacity(CLIENT_AUTH_DOMAIN.len() + encoded.len() - 64);
    signing_input.extend_from_slice(CLIENT_AUTH_DOMAIN);
    signing_input.extend_from_slice(&encoded[..encoded.len() - 64]);
    peer_key
        .verify_strict(&signing_input, &signature)
        .map_err(|_| InvalidProtocolMessage)?;
    Ok(ParsedClientHello {
        message_id,
        client_nonce,
        ephemeral_public_key,
    })
}

fn parse_server_hello(
    encoded: &[u8],
    context: HandshakeContext,
    client_hello: &[u8],
    client_nonce: [u8; 32],
    controller_verifying_key: &VerifyingKey,
    now: u64,
) -> Result<ParsedServerHello, InvalidProtocolMessage> {
    let message_id = parse_common_header(encoded, SERVER_HELLO_TYPE, SERVER_HELLO_BODY_LENGTH)?;
    let expected_message_id =
        parse_common_header(client_hello, CLIENT_HELLO_TYPE, CLIENT_HELLO_BODY_LENGTH)?;
    let body = &encoded[COMMON_HEADER_LENGTH..];
    let network_id = array::<16>(&body[0..16])?;
    let source_node_id = array::<16>(&body[16..32])?;
    let destination_node_id = array::<16>(&body[32..48])?;
    let echoed_client_nonce = array::<32>(&body[48..80])?;
    let server_nonce = array::<32>(&body[80..112])?;
    let ephemeral_public_key = array::<32>(&body[112..144])?;
    let client_hello_hash = array::<32>(&body[144..176])?;
    let session_id = array::<16>(&body[176..192])?;
    let server_time = u64::from_be_bytes(array::<8>(&body[192..200])?);
    let suite_id = u16::from_be_bytes(array::<2>(&body[200..202])?);
    let credential_length = u16::from_be_bytes(array::<2>(&body[202..204])?);
    let credential = array::<CREDENTIAL_LENGTH>(&body[204..404])?;
    let expected_client_hash: [u8; 32] = Sha256::digest(client_hello).into();
    if message_id != expected_message_id
        || network_id != context.network_id
        || source_node_id != context.peer_node_id
        || destination_node_id != context.local_node_id
        || echoed_client_nonce != client_nonce
        || server_nonce == [0_u8; 32]
        || ephemeral_public_key == [0_u8; 32]
        || client_hello_hash != expected_client_hash
        || session_id == [0_u8; 16]
        || server_time.abs_diff(now) > MAX_CLOCK_SKEW_SECONDS
        || suite_id != SUITE_ID
        || credential_length
            != u16::try_from(CREDENTIAL_LENGTH).map_err(|_| InvalidProtocolMessage)?
    {
        return Err(InvalidProtocolMessage);
    }
    verify_peer_credential(&credential, context, controller_verifying_key, now)?;
    let signature = Signature::from_bytes(&array::<64>(&body[404..468])?);
    let peer_key = VerifyingKey::from_bytes(&array::<32>(&credential[36..68])?)
        .map_err(|_| InvalidProtocolMessage)?;
    let mut signing_input = Vec::with_capacity(
        SERVER_AUTH_DOMAIN.len() + expected_client_hash.len() + encoded.len() - 64,
    );
    signing_input.extend_from_slice(SERVER_AUTH_DOMAIN);
    signing_input.extend_from_slice(&expected_client_hash);
    signing_input.extend_from_slice(&encoded[..encoded.len() - 64]);
    peer_key
        .verify_strict(&signing_input, &signature)
        .map_err(|_| InvalidProtocolMessage)?;
    Ok(ParsedServerHello {
        message_id,
        server_nonce,
        ephemeral_public_key,
        session_id,
    })
}

fn validate_context(context: HandshakeContext) -> Result<(), InvalidProtocolMessage> {
    if context.network_id == [0_u8; 16]
        || context.local_node_id == [0_u8; 16]
        || context.peer_node_id == [0_u8; 16]
        || context.local_node_id == context.peer_node_id
        || invalid_virtual_ip(context.local_virtual_ip)
        || invalid_virtual_ip(context.peer_virtual_ip)
        || context.local_virtual_ip == context.peer_virtual_ip
    {
        return Err(InvalidProtocolMessage);
    }
    Ok(())
}

fn invalid_virtual_ip(address: Ipv4Addr) -> bool {
    address.is_unspecified() || address.is_multicast() || address == Ipv4Addr::BROADCAST
}

fn validate_local_credential(
    credential: &[u8; CREDENTIAL_LENGTH],
    context: HandshakeContext,
    identity_signing_key: &SigningKey,
) -> Result<(), InvalidProtocolMessage> {
    let network_id = array::<16>(&credential[4..20])?;
    let encoded_node_id = array::<16>(&credential[20..36])?;
    let identity_public_key = array::<32>(&credential[36..68])?;
    let virtual_ip = Ipv4Addr::from(array::<4>(&credential[68..72])?);
    if credential[0] != 1
        || credential[1..4] != [0_u8; 3]
        || network_id != context.network_id
        || encoded_node_id != context.local_node_id
        || identity_public_key != identity_signing_key.verifying_key().to_bytes()
        || virtual_ip != context.local_virtual_ip
    {
        return Err(InvalidProtocolMessage);
    }
    Ok(())
}

fn verify_peer_credential(
    credential: &[u8; CREDENTIAL_LENGTH],
    context: HandshakeContext,
    controller_verifying_key: &VerifyingKey,
    now: u64,
) -> Result<VerifiedCredential, InvalidProtocolMessage> {
    let verified = verify_credential(credential, controller_verifying_key, now)
        .map_err(|_| InvalidProtocolMessage)?;
    if verified.network_id != context.network_id
        || verified.node_id != context.peer_node_id
        || verified.virtual_ipv4 != context.peer_virtual_ip
    {
        return Err(InvalidProtocolMessage);
    }
    Ok(verified)
}

fn encode_common_header(
    message_type: u8,
    body_length: usize,
    message_id: u32,
) -> Result<[u8; COMMON_HEADER_LENGTH], InvalidProtocolMessage> {
    let mut header = [0_u8; COMMON_HEADER_LENGTH];
    header[0..4].copy_from_slice(MAGIC);
    header[4] = VERSION;
    header[5] = message_type;
    header[8..12].copy_from_slice(
        &u32::try_from(body_length)
            .map_err(|_| InvalidProtocolMessage)?
            .to_be_bytes(),
    );
    header[12..16].copy_from_slice(&message_id.to_be_bytes());
    Ok(header)
}

fn parse_common_header(
    encoded: &[u8],
    expected_type: u8,
    expected_body_length: usize,
) -> Result<u32, InvalidProtocolMessage> {
    let expected_total = COMMON_HEADER_LENGTH
        .checked_add(expected_body_length)
        .ok_or(InvalidProtocolMessage)?;
    if encoded.len() != expected_total
        || &encoded[0..4] != MAGIC
        || encoded[4] != VERSION
        || encoded[5] != expected_type
        || encoded[6..8] != [0_u8; 2]
        || usize::try_from(u32::from_be_bytes(array::<4>(&encoded[8..12])?))
            .map_err(|_| InvalidProtocolMessage)?
            != expected_body_length
    {
        return Err(InvalidProtocolMessage);
    }
    Ok(u32::from_be_bytes(array::<4>(&encoded[12..16])?))
}

fn derive_handshake_keys(
    shared_secret: &[u8; 32],
    network_id: [u8; 16],
    client_nonce: [u8; 32],
    server_nonce: [u8; 32],
    hello_transcript_hash: [u8; 32],
) -> Result<HandshakeKeys, InvalidProtocolMessage> {
    let handshake_salt = transcript_hash(
        HANDSHAKE_SALT_DOMAIN,
        &[&network_id, &client_nonce, &server_nonce],
    );
    let handshake_prk = hkdf_extract(&handshake_salt, shared_secret);
    let client_key = Zeroizing::new(hkdf_expand::<32>(
        &handshake_prk,
        CLIENT_HANDSHAKE_KEY_DOMAIN,
        &hello_transcript_hash,
    )?);
    let server_key = Zeroizing::new(hkdf_expand::<32>(
        &handshake_prk,
        SERVER_HANDSHAKE_KEY_DOMAIN,
        &hello_transcript_hash,
    )?);
    let client_nonce_salt = hkdf_expand::<4>(
        &handshake_prk,
        CLIENT_HANDSHAKE_NONCE_DOMAIN,
        &hello_transcript_hash,
    )?;
    let server_nonce_salt = hkdf_expand::<4>(
        &handshake_prk,
        SERVER_HANDSHAKE_NONCE_DOMAIN,
        &hello_transcript_hash,
    )?;
    Ok(HandshakeKeys {
        handshake_prk,
        client_key,
        server_key,
        client_nonce_salt,
        server_nonce_salt,
        hello_transcript_hash,
    })
}

fn established_session(
    role: SessionRole,
    context: HandshakeContext,
    session_id: [u8; 16],
    handshake_prk: &[u8; 32],
    full_transcript_hash: [u8; 32],
) -> Result<EstablishedSession, InvalidProtocolMessage> {
    let application_prk = hkdf_extract(&full_transcript_hash, handshake_prk);
    let (client_node_id, server_node_id) = match role {
        SessionRole::Client => (context.local_node_id, context.peer_node_id),
        SessionRole::Server => (context.peer_node_id, context.local_node_id),
    };
    let mut client_info = Vec::with_capacity(64);
    client_info.extend_from_slice(&context.network_id);
    client_info.extend_from_slice(&session_id);
    client_info.extend_from_slice(&client_node_id);
    client_info.extend_from_slice(&server_node_id);
    let mut server_info = Vec::with_capacity(64);
    server_info.extend_from_slice(&context.network_id);
    server_info.extend_from_slice(&session_id);
    server_info.extend_from_slice(&server_node_id);
    server_info.extend_from_slice(&client_node_id);
    let client_to_server_secret = Zeroizing::new(hkdf_expand::<32>(
        &application_prk,
        CLIENT_TRAFFIC_SECRET_DOMAIN,
        &client_info,
    )?);
    let server_to_client_secret = Zeroizing::new(hkdf_expand::<32>(
        &application_prk,
        SERVER_TRAFFIC_SECRET_DOMAIN,
        &server_info,
    )?);
    Ok(EstablishedSession {
        role,
        context,
        session_id,
        client_to_server_secret,
        server_to_client_secret,
    })
}

fn encode_finish(
    message_type: u8,
    message_id: u32,
    session_id: [u8; 16],
    key: &[u8; 32],
    nonce_salt: [u8; 4],
    plaintext: [u8; 32],
) -> Result<Vec<u8>, InvalidProtocolMessage> {
    let header = encode_common_header(message_type, FINISH_BODY_LENGTH, message_id)?;
    let mut prefix = Vec::with_capacity(28);
    prefix.extend_from_slice(&session_id);
    prefix.extend_from_slice(&0_u64.to_be_bytes());
    prefix.extend_from_slice(&32_u16.to_be_bytes());
    prefix.extend_from_slice(&0_u16.to_be_bytes());
    let mut aad = Vec::with_capacity(COMMON_HEADER_LENGTH + prefix.len());
    aad.extend_from_slice(&header);
    aad.extend_from_slice(&prefix);
    let mut ciphertext = plaintext.to_vec();
    let cipher = ChaCha20Poly1305::new_from_slice(key).map_err(|_| InvalidProtocolMessage)?;
    let nonce = finish_nonce(nonce_salt);
    let tag = cipher
        .encrypt_in_place_detached(Nonce::from_slice(&nonce), &aad, &mut ciphertext)
        .map_err(|_| InvalidProtocolMessage)?;
    let mut encoded = Vec::with_capacity(COMMON_HEADER_LENGTH + FINISH_BODY_LENGTH);
    encoded.extend_from_slice(&header);
    encoded.extend_from_slice(&prefix);
    encoded.extend_from_slice(&ciphertext);
    encoded.extend_from_slice(&tag);
    Ok(encoded)
}

fn decode_finish(
    encoded: &[u8],
    expected_type: u8,
    expected_message_id: u32,
    expected_session_id: [u8; 16],
    key: &[u8; 32],
    nonce_salt: [u8; 4],
) -> Result<[u8; 32], InvalidProtocolMessage> {
    let message_id = parse_common_header(encoded, expected_type, FINISH_BODY_LENGTH)?;
    let body = &encoded[COMMON_HEADER_LENGTH..];
    let session_id = array::<16>(&body[0..16])?;
    let sequence = u64::from_be_bytes(array::<8>(&body[16..24])?);
    let ciphertext_length = u16::from_be_bytes(array::<2>(&body[24..26])?);
    if message_id != expected_message_id
        || session_id != expected_session_id
        || sequence != 0
        || ciphertext_length != 32
        || body[26..28] != [0_u8; 2]
    {
        return Err(InvalidProtocolMessage);
    }
    let mut plaintext = body[28..60].to_vec();
    let tag = Tag::from_slice(&body[60..76]);
    let cipher = ChaCha20Poly1305::new_from_slice(key).map_err(|_| InvalidProtocolMessage)?;
    let nonce = finish_nonce(nonce_salt);
    cipher
        .decrypt_in_place_detached(
            Nonce::from_slice(&nonce),
            &encoded[..COMMON_HEADER_LENGTH + 28],
            &mut plaintext,
            tag,
        )
        .map_err(|_| InvalidProtocolMessage)?;
    array::<32>(&plaintext)
}

fn finish_nonce(nonce_salt: [u8; 4]) -> [u8; 12] {
    let mut nonce = [0_u8; 12];
    nonce[..4].copy_from_slice(&nonce_salt);
    nonce
}

fn transcript_hash(domain: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(domain);
    for part in parts {
        hash.update(part);
    }
    hash.finalize().into()
}

fn hkdf_extract(salt: &[u8], input: &[u8]) -> Zeroizing<[u8; 32]> {
    let (prk, _) = Hkdf::<Sha256>::extract(Some(salt), input);
    let mut bytes = [0_u8; 32];
    bytes.copy_from_slice(prk.as_slice());
    Zeroizing::new(bytes)
}

pub(crate) fn hkdf_expand<const LENGTH: usize>(
    prk: &[u8; 32],
    domain: &[u8],
    suffix: &[u8],
) -> Result<[u8; LENGTH], InvalidProtocolMessage> {
    let hkdf = Hkdf::<Sha256>::from_prk(prk).map_err(|_| InvalidProtocolMessage)?;
    let mut info = Vec::with_capacity(domain.len() + suffix.len());
    info.extend_from_slice(domain);
    info.extend_from_slice(suffix);
    let mut output = [0_u8; LENGTH];
    hkdf.expand(&info, &mut output)
        .map_err(|_| InvalidProtocolMessage)?;
    Ok(output)
}

fn array<const LENGTH: usize>(bytes: &[u8]) -> Result<[u8; LENGTH], InvalidProtocolMessage> {
    bytes.try_into().map_err(|_| InvalidProtocolMessage)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_zero_x25519_shared_secret_is_rejected() {
        let private = EphemeralPrivateKey::from_bytes([9_u8; 32]);
        assert!(private.shared_secret([0_u8; 32]).is_err());
    }
}
