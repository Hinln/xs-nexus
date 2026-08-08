#![no_main]

use ed25519_dalek::SigningKey;
use libfuzzer_sys::fuzz_target;
use xs_protocol::{
    parse_relay_frame, parse_relay_keepalive, verify_relay_keepalive_response,
    verify_relay_register_request, verify_relay_register_response,
};

const NOW: u64 = 1_700_000_100;

fuzz_target!(|input: &[u8]| {
    let controller = SigningKey::from_bytes(&[11_u8; 32]);
    let relay = SigningKey::from_bytes(&[31_u8; 32]);
    let network_id = [41_u8; 16];
    let node_id = [51_u8; 16];
    let relay_id = [61_u8; 16];
    let request_id = [71_u8; 16];
    let lease_id = [81_u8; 16];

    let _ = parse_relay_frame(input);
    let _ = parse_relay_keepalive(input);
    let _ = verify_relay_register_request(input, &controller.verifying_key(), NOW);
    let _ = verify_relay_register_response(
        input,
        &relay.verifying_key(),
        network_id,
        node_id,
        relay_id,
        request_id,
        NOW,
    );
    let _ = verify_relay_keepalive_response(
        input,
        &relay.verifying_key(),
        network_id,
        relay_id,
        node_id,
        lease_id,
        NOW,
    );
});
