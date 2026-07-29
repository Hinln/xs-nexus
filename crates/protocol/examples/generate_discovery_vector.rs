use std::{
    error::Error,
    io::Write as _,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
};

use ed25519_dalek::SigningKey;
use serde_json::json;
use sha2::{Digest as _, Sha256};
use xs_protocol::{
    CredentialClaims, DiscoveryRequest, discovery_response, node_id, role_set_digest,
    sign_credential, verify_discovery_request,
};

const NOW: u64 = 1_700_000_100;

fn main() -> Result<(), Box<dyn Error>> {
    let credential_seed = [11_u8; 32];
    let configuration_seed = [21_u8; 32];
    let identity_seed = [31_u8; 32];
    let network_id = [41_u8; 16];
    let request_id = [51_u8; 16];
    let credential_key = SigningKey::from_bytes(&credential_seed);
    let configuration_key = SigningKey::from_bytes(&configuration_seed);
    let identity = SigningKey::from_bytes(&identity_seed);
    let node_id = node_id(&identity.verifying_key().to_bytes());
    let credential = sign_credential(
        CredentialClaims {
            network_id,
            identity_public_key: identity.verifying_key().to_bytes(),
            virtual_ipv4: Ipv4Addr::new(100, 88, 0, 31),
            serial: 31,
            not_before: NOW - 60,
            not_after: NOW + 3_600,
            role_bitmap: 1,
            role_set_digest: role_set_digest(1, &["linux".to_owned()])?,
        },
        &credential_key,
    );
    let request =
        DiscoveryRequest::new(network_id, node_id, request_id, NOW, credential, &identity)?;
    let verified =
        verify_discovery_request(request.encoded(), &credential_key.verifying_key(), NOW)?;
    let observed = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(203, 0, 113, 31), 42001));
    let response = discovery_response(verified, observed, NOW, &configuration_key)?;
    let vector = json!({
        "schema": "xs-discovery-v1",
        "now": NOW,
        "network_id_hex": hexadecimal(&network_id),
        "request_id_hex": hexadecimal(&request_id),
        "credential_seed_hex": hexadecimal(&credential_seed),
        "configuration_seed_hex": hexadecimal(&configuration_seed),
        "identity_seed_hex": hexadecimal(&identity_seed),
        "node_id_hex": hexadecimal(&node_id),
        "observed_endpoint": observed.to_string(),
        "request_hex": hexadecimal(request.encoded()),
        "request_sha256": hexadecimal(&Sha256::digest(request.encoded())),
        "response_hex": hexadecimal(&response),
        "response_sha256": hexadecimal(&Sha256::digest(response)),
    });
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    serde_json::to_writer_pretty(&mut output, &vector)?;
    writeln!(output)?;
    Ok(())
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
