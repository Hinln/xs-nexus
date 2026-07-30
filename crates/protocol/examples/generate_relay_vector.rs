use std::{error::Error, io::Write as _};

use ed25519_dalek::SigningKey;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use xs_protocol::{
    RELAY_KEEPALIVE_RESPONSE_LENGTH, RELAY_REGISTER_REQUEST_LENGTH, RELAY_REGISTER_RESPONSE_LENGTH,
    RelayFrame, RelayKeepaliveResponse, RelayRegisterRequest, RelayRegisterResponse,
    encode_relay_frame, encode_relay_keepalive, parse_relay_frame, parse_relay_keepalive,
    sign_relay_keepalive_response, sign_relay_register_request, sign_relay_register_response,
    verify_relay_keepalive_response, verify_relay_register_request, verify_relay_register_response,
};

const RELAY_SEED: [u8; 32] = [71_u8; 32];
const RELAY_ID: [u8; 16] = [72_u8; 16];
const REQUEST_ID: [u8; 16] = [73_u8; 16];
const LEASE_ID: [u8; 16] = [74_u8; 16];

struct Fixture {
    now: u64,
    client_hello: Vec<u8>,
    controller_seed: [u8; 32],
    identity_seed: [u8; 32],
    network_id: [u8; 16],
    source_node_id: [u8; 16],
    destination_node_id: [u8; 16],
    credential: [u8; 200],
}

struct Messages {
    lease_expires_at: u64,
    register_request: [u8; RELAY_REGISTER_REQUEST_LENGTH],
    register_response: [u8; RELAY_REGISTER_RESPONSE_LENGTH],
    data_frame: Vec<u8>,
    keepalive: [u8; 104],
    keepalive_response: [u8; RELAY_KEEPALIVE_RESPONSE_LENGTH],
}

fn main() -> Result<(), Box<dyn Error>> {
    let fixture = load_fixture()?;
    let messages = encode_messages(&fixture)?;
    let vector = vector_json(&fixture, &messages);
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    serde_json::to_writer_pretty(&mut output, &vector)?;
    writeln!(output)?;
    Ok(())
}

fn load_fixture() -> Result<Fixture, Box<dyn Error>> {
    let session: Value =
        serde_json::from_str(include_str!("../../../tests/vectors/xsp1/session-v1.json"))?;
    let now = session["now"].as_u64().ok_or("session vector now")?;
    let client_hello = decode_hex(session["client_hello_hex"].as_str().ok_or("client hello")?)?;
    let controller_seed = decode_hex_array::<32>(
        session["controller_seed_hex"]
            .as_str()
            .ok_or("controller seed")?,
    )?;
    let identity_seed = decode_hex_array::<32>(
        session["client_identity_seed_hex"]
            .as_str()
            .ok_or("identity seed")?,
    )?;
    let network_id = array::<16>(&client_hello[16..32])?;
    let source_node_id = array::<16>(&client_hello[32..48])?;
    let destination_node_id = array::<16>(&client_hello[48..64])?;
    let credential = array::<200>(&client_hello[138..338])?;
    Ok(Fixture {
        now,
        client_hello,
        controller_seed,
        identity_seed,
        network_id,
        source_node_id,
        destination_node_id,
        credential,
    })
}

fn encode_messages(fixture: &Fixture) -> Result<Messages, Box<dyn Error>> {
    let controller_key = SigningKey::from_bytes(&fixture.controller_seed);
    let identity_key = SigningKey::from_bytes(&fixture.identity_seed);
    let relay_key = SigningKey::from_bytes(&RELAY_SEED);
    let request = RelayRegisterRequest {
        network_id: fixture.network_id,
        node_id: fixture.source_node_id,
        relay_id: RELAY_ID,
        request_id: REQUEST_ID,
        client_time: fixture.now,
        credential: fixture.credential,
    };
    let register_request = sign_relay_register_request(request, &identity_key);
    verify_relay_register_request(
        &register_request,
        &controller_key.verifying_key(),
        fixture.now,
    )?;

    let response = RelayRegisterResponse {
        network_id: fixture.network_id,
        node_id: fixture.source_node_id,
        relay_id: RELAY_ID,
        request_id: REQUEST_ID,
        lease_id: LEASE_ID,
        expires_at: fixture.now + 120,
    };
    let register_response = sign_relay_register_response(response, &relay_key);
    verify_relay_register_response(
        &register_response,
        &relay_key.verifying_key(),
        fixture.network_id,
        fixture.source_node_id,
        RELAY_ID,
        REQUEST_ID,
        fixture.now,
    )?;

    let data_frame = encode_relay_frame(RelayFrame {
        network_id: fixture.network_id,
        relay_id: RELAY_ID,
        source_node_id: fixture.source_node_id,
        destination_node_id: fixture.destination_node_id,
        lease_id: LEASE_ID,
        sequence: 1,
        payload: &fixture.client_hello,
    })?;
    parse_relay_frame(&data_frame)?;

    let keepalive = encode_relay_keepalive(
        fixture.network_id,
        RELAY_ID,
        fixture.source_node_id,
        LEASE_ID,
        2,
    );
    parse_relay_keepalive(&keepalive)?;

    let keepalive_response = sign_relay_keepalive_response(
        RelayKeepaliveResponse {
            network_id: fixture.network_id,
            relay_id: RELAY_ID,
            node_id: fixture.source_node_id,
            lease_id: LEASE_ID,
            server_time: fixture.now + 1,
        },
        &relay_key,
    );
    verify_relay_keepalive_response(
        &keepalive_response,
        &relay_key.verifying_key(),
        fixture.network_id,
        RELAY_ID,
        fixture.source_node_id,
        LEASE_ID,
        fixture.now,
    )?;
    Ok(Messages {
        lease_expires_at: response.expires_at,
        register_request,
        register_response,
        data_frame,
        keepalive,
        keepalive_response,
    })
}

fn vector_json(fixture: &Fixture, messages: &Messages) -> Value {
    json!({
        "schema": "xs-relay-v1",
        "now": fixture.now,
        "lease_expires_at": messages.lease_expires_at,
        "network_id_hex": hexadecimal(&fixture.network_id),
        "source_node_id_hex": hexadecimal(&fixture.source_node_id),
        "destination_node_id_hex": hexadecimal(&fixture.destination_node_id),
        "relay_id_hex": hexadecimal(&RELAY_ID),
        "request_id_hex": hexadecimal(&REQUEST_ID),
        "lease_id_hex": hexadecimal(&LEASE_ID),
        "controller_seed_hex": hexadecimal(&fixture.controller_seed),
        "identity_seed_hex": hexadecimal(&fixture.identity_seed),
        "relay_seed_hex": hexadecimal(&RELAY_SEED),
        "register_request_hex": hexadecimal(&messages.register_request),
        "register_request_sha256": hexadecimal(&Sha256::digest(messages.register_request)),
        "register_response_hex": hexadecimal(&messages.register_response),
        "register_response_sha256": hexadecimal(&Sha256::digest(messages.register_response)),
        "inner_xsp_hex": hexadecimal(&fixture.client_hello),
        "inner_xsp_sha256": hexadecimal(&Sha256::digest(&fixture.client_hello)),
        "data_frame_hex": hexadecimal(&messages.data_frame),
        "data_frame_sha256": hexadecimal(&Sha256::digest(&messages.data_frame)),
        "keepalive_hex": hexadecimal(&messages.keepalive),
        "keepalive_sha256": hexadecimal(&Sha256::digest(messages.keepalive)),
        "keepalive_response_hex": hexadecimal(&messages.keepalive_response),
        "keepalive_response_sha256": hexadecimal(&Sha256::digest(messages.keepalive_response)),
    })
}

fn decode_hex(encoded: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    if !encoded.len().is_multiple_of(2) {
        return Err("hexadecimal input must contain complete bytes".into());
    }
    (0..encoded.len())
        .step_by(2)
        .map(|index| Ok(u8::from_str_radix(&encoded[index..index + 2], 16)?))
        .collect()
}

fn decode_hex_array<const LENGTH: usize>(encoded: &str) -> Result<[u8; LENGTH], Box<dyn Error>> {
    array(&decode_hex(encoded)?)
}

fn array<const LENGTH: usize>(encoded: &[u8]) -> Result<[u8; LENGTH], Box<dyn Error>> {
    encoded
        .try_into()
        .map_err(|_| format!("expected exactly {LENGTH} bytes").into())
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
