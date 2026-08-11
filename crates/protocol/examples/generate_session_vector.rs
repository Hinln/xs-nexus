use std::{error::Error, io::Write as _, net::Ipv4Addr};

use ed25519_dalek::SigningKey;
use serde_json::json;
use sha2::{Digest, Sha256};
use xs_protocol::{
    ClientHandshakeParameters, ClientHelloSent, CredentialClaims, DataFlags, EphemeralPrivateKey,
    HandshakeContext, ServerHandshakeParameters, ServerHelloSent, role_set_digest, sign_credential,
};

const NOW: u64 = 1_700_000_100;

#[derive(Clone, Copy)]
struct HandshakeInputs<'a> {
    client_context: HandshakeContext,
    server_context: HandshakeContext,
    client_credential: [u8; 200],
    server_credential: [u8; 200],
    client_ephemeral_seed: [u8; 32],
    server_ephemeral_seed: [u8; 32],
    client_nonce: [u8; 32],
    server_nonce: [u8; 32],
    session_id: [u8; 16],
    message_id: u32,
    client_identity: &'a SigningKey,
    server_identity: &'a SigningKey,
    controller: &'a SigningKey,
    client_ip: Ipv4Addr,
    server_ip: Ipv4Addr,
}

struct SessionArtifacts {
    client_hello: Vec<u8>,
    server_hello: Vec<u8>,
    client_finish: Vec<u8>,
    server_finish: Vec<u8>,
    sample_ipv4: Vec<u8>,
    data_packet: Vec<u8>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let controller_seed = [11_u8; 32];
    let client_identity_seed = [21_u8; 32];
    let server_identity_seed = [31_u8; 32];
    let client_ephemeral_seed = [51_u8; 32];
    let server_ephemeral_seed = [71_u8; 32];
    let client_nonce = [61_u8; 32];
    let server_nonce = [81_u8; 32];
    let session_id = [91_u8; 16];
    let message_id = 0x1020_3040;
    let network_id = [41_u8; 16];
    let client_ip = Ipv4Addr::new(100, 88, 0, 21);
    let server_ip = Ipv4Addr::new(100, 88, 0, 31);
    let controller = SigningKey::from_bytes(&controller_seed);
    let client_identity = SigningKey::from_bytes(&client_identity_seed);
    let server_identity = SigningKey::from_bytes(&server_identity_seed);
    let tags = vec!["linux".to_owned()];
    let client_credential = credential(
        network_id,
        &client_identity,
        client_ip,
        21,
        &tags,
        &controller,
    )?;
    let server_credential = credential(
        network_id,
        &server_identity,
        server_ip,
        31,
        &tags,
        &controller,
    )?;
    let client_context = HandshakeContext {
        network_id,
        local_node_id: xs_protocol::node_id(client_identity.verifying_key().as_bytes()),
        local_virtual_ip: client_ip,
        peer_node_id: xs_protocol::node_id(server_identity.verifying_key().as_bytes()),
        peer_virtual_ip: server_ip,
    };
    let server_context = HandshakeContext {
        network_id,
        local_node_id: client_context.peer_node_id,
        local_virtual_ip: server_ip,
        peer_node_id: client_context.local_node_id,
        peer_virtual_ip: client_ip,
    };

    let artifacts = run_handshake(&HandshakeInputs {
        client_context,
        server_context,
        client_credential,
        server_credential,
        client_ephemeral_seed,
        server_ephemeral_seed,
        client_nonce,
        server_nonce,
        session_id,
        message_id,
        client_identity: &client_identity,
        server_identity: &server_identity,
        controller: &controller,
        client_ip,
        server_ip,
    })?;

    let vector = json!({
        "schema": "xsp1-session-v1",
        "suite_id": 1,
        "now": NOW,
        "retry_semantics": {
            "encrypted_control": "same-payload-fresh-sequence",
            "finish": "exact-frame",
        },
        "replay_observability": {
            "classification": "authenticated-only",
            "exposure": "aggregate-counter",
            "wire_error": "invalid-protocol-message",
        },
        "message_id": message_id,
        "network_id_hex": hexadecimal(&network_id),
        "controller_seed_hex": hexadecimal(&controller_seed),
        "client_identity_seed_hex": hexadecimal(&client_identity_seed),
        "server_identity_seed_hex": hexadecimal(&server_identity_seed),
        "client_ephemeral_seed_hex": hexadecimal(&client_ephemeral_seed),
        "server_ephemeral_seed_hex": hexadecimal(&server_ephemeral_seed),
        "client_nonce_hex": hexadecimal(&client_nonce),
        "server_nonce_hex": hexadecimal(&server_nonce),
        "session_id_hex": hexadecimal(&session_id),
        "client_hello_hex": hexadecimal(&artifacts.client_hello),
        "client_hello_sha256": sha256(&artifacts.client_hello),
        "server_hello_hex": hexadecimal(&artifacts.server_hello),
        "server_hello_sha256": sha256(&artifacts.server_hello),
        "client_finish_hex": hexadecimal(&artifacts.client_finish),
        "client_finish_sha256": sha256(&artifacts.client_finish),
        "server_finish_hex": hexadecimal(&artifacts.server_finish),
        "server_finish_sha256": sha256(&artifacts.server_finish),
        "sample_ipv4_hex": hexadecimal(&artifacts.sample_ipv4),
        "data_packet_hex": hexadecimal(&artifacts.data_packet),
        "data_header_hex": hexadecimal(&artifacts.data_packet[..xs_protocol::DATA_HEADER_LENGTH]),
        "data_packet_sha256": sha256(&artifacts.data_packet),
    });
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    serde_json::to_writer_pretty(&mut output, &vector)?;
    writeln!(output)?;
    Ok(())
}

fn run_handshake(input: &HandshakeInputs<'_>) -> Result<SessionArtifacts, Box<dyn Error>> {
    let client = ClientHelloSent::start(
        ClientHandshakeParameters {
            context: input.client_context,
            credential: input.client_credential,
            ephemeral_private_key: EphemeralPrivateKey::from_bytes(input.client_ephemeral_seed),
            client_nonce: input.client_nonce,
            message_id: input.message_id,
            client_time: NOW,
        },
        input.client_identity,
    )?;
    let client_hello = client.encoded().to_vec();
    let server = ServerHelloSent::accept(
        &client_hello,
        ServerHandshakeParameters {
            context: input.server_context,
            credential: input.server_credential,
            ephemeral_private_key: EphemeralPrivateKey::from_bytes(input.server_ephemeral_seed),
            server_nonce: input.server_nonce,
            session_id: input.session_id,
            server_time: NOW,
        },
        input.server_identity,
        &input.controller.verifying_key(),
        NOW,
    )?;
    let server_hello = server.encoded().to_vec();
    let client =
        client.accept_server_hello(&server_hello, &input.controller.verifying_key(), NOW)?;
    let client_finish = client.encoded().to_vec();
    let (server_session, server_finish) = server.accept_client_finish(&client_finish)?;
    let client_session = client.accept_server_finish(&server_finish)?;
    let (mut client_sender, _) = client_session.into_data_plane()?;
    let (_server_sender, _server_receiver) = server_session.into_data_plane()?;
    let sample_ipv4 = ipv4_packet(input.client_ip, input.server_ip);
    let data_packet = client_sender.seal_ipv4(DataFlags::ACK_ELICITING, 7, &sample_ipv4)?;

    Ok(SessionArtifacts {
        client_hello,
        server_hello,
        client_finish,
        server_finish: server_finish.clone(),
        sample_ipv4,
        data_packet,
    })
}

fn ipv4_packet(source: Ipv4Addr, destination: Ipv4Addr) -> Vec<u8> {
    let mut packet = vec![0_u8; 20];
    packet[0] = 0x45;
    packet[2..4].copy_from_slice(&20_u16.to_be_bytes());
    packet[8] = 64;
    packet[9] = 1;
    packet[12..16].copy_from_slice(&source.octets());
    packet[16..20].copy_from_slice(&destination.octets());
    packet
}

fn credential(
    network_id: [u8; 16],
    identity: &SigningKey,
    virtual_ipv4: Ipv4Addr,
    serial: u64,
    tags: &[String],
    controller: &SigningKey,
) -> Result<[u8; 200], Box<dyn Error>> {
    Ok(sign_credential(
        CredentialClaims {
            network_id,
            identity_public_key: identity.verifying_key().to_bytes(),
            virtual_ipv4,
            serial,
            not_before: NOW - 60,
            not_after: NOW + 3_600,
            role_bitmap: 1,
            role_set_digest: role_set_digest(1, tags)?,
        },
        controller,
    ))
}

fn hexadecimal(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(DIGITS[usize::from(byte >> 4)]));
        encoded.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn sha256(bytes: &[u8]) -> String {
    hexadecimal(&Sha256::digest(bytes))
}
