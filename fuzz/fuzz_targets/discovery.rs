#![no_main]

use std::net::Ipv4Addr;

use ed25519_dalek::SigningKey;
use libfuzzer_sys::fuzz_target;
use xs_protocol::{
    CredentialClaims, DiscoveryRequest, role_set_digest, sign_credential, verify_discovery_request,
    verify_discovery_response,
};

const NOW: u64 = 1_700_000_100;

fuzz_target!(|input: &[u8]| {
    let controller = SigningKey::from_bytes(&[11_u8; 32]);
    let identity = SigningKey::from_bytes(&[21_u8; 32]);
    let network_id = [41_u8; 16];
    let node_id = xs_protocol::node_id(identity.verifying_key().as_bytes());
    let credential = sign_credential(
        CredentialClaims {
            network_id,
            identity_public_key: identity.verifying_key().to_bytes(),
            virtual_ipv4: Ipv4Addr::new(100, 88, 0, 21),
            serial: 21,
            not_before: NOW - 60,
            not_after: NOW + 3_600,
            role_bitmap: 1,
            role_set_digest: role_set_digest(1, &["linux".to_owned()]).expect("fixed tag"),
        },
        &controller,
    );
    let request =
        DiscoveryRequest::new(network_id, node_id, [51_u8; 16], NOW, credential, &identity)
            .expect("fixed discovery request");

    let _ = verify_discovery_request(input, &controller.verifying_key(), NOW);
    let _ = verify_discovery_response(input, &request, &controller.verifying_key(), NOW);
});
