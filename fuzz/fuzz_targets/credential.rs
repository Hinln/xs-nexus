#![no_main]

use ed25519_dalek::SigningKey;
use libfuzzer_sys::fuzz_target;
use xs_protocol::verify_credential;

fuzz_target!(|input: &[u8]| {
    let controller = SigningKey::from_bytes(&[11_u8; 32]);
    let _ = verify_credential(input, &controller.verifying_key(), 1_700_000_100);
});
