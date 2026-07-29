use std::{fmt::Write as _, net::Ipv4Addr};

use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};
use xs_protocol::{CredentialClaims, controller_key_id, node_id, role_set_digest, sign_credential};

fn main() {
    let credential_signing_seed = [7_u8; 32];
    let identity_signing_seed = [9_u8; 32];
    let credential_signing_key = SigningKey::from_bytes(&credential_signing_seed);
    let identity_signing_key = SigningKey::from_bytes(&identity_signing_seed);
    let tags = vec!["gateway".to_owned(), "linux".to_owned()];
    let role_digest = role_set_digest(5, &tags).expect("fixed tags are valid");
    let claims = CredentialClaims {
        network_id: [3_u8; 16],
        identity_public_key: identity_signing_key.verifying_key().to_bytes(),
        virtual_ipv4: Ipv4Addr::new(100, 88, 0, 16),
        serial: 42,
        not_before: 1_700_000_000,
        not_after: 1_700_086_400,
        role_bitmap: 5,
        role_set_digest: role_digest,
    };
    let credential = sign_credential(claims, &credential_signing_key);
    let credential_hash = Sha256::digest(credential);

    println!("{{");
    println!("  \"note\": \"deterministic public test vector; seeds are not production secrets\",");
    println!(
        "  \"credential_signing_seed_hex\": \"{}\",",
        hexadecimal(&credential_signing_seed)
    );
    println!(
        "  \"credential_signing_public_key_hex\": \"{}\",",
        hexadecimal(credential_signing_key.verifying_key().as_bytes())
    );
    println!(
        "  \"controller_key_id\": {},",
        controller_key_id(&credential_signing_key.verifying_key())
    );
    println!(
        "  \"identity_signing_seed_hex\": \"{}\",",
        hexadecimal(&identity_signing_seed)
    );
    println!(
        "  \"identity_public_key_hex\": \"{}\",",
        hexadecimal(identity_signing_key.verifying_key().as_bytes())
    );
    println!(
        "  \"network_id_hex\": \"{}\",",
        hexadecimal(&claims.network_id)
    );
    println!(
        "  \"node_id_hex\": \"{}\",",
        hexadecimal(&node_id(&claims.identity_public_key))
    );
    println!("  \"virtual_ipv4\": \"{}\",", claims.virtual_ipv4);
    println!("  \"serial\": {},", claims.serial);
    println!("  \"not_before\": {},", claims.not_before);
    println!("  \"not_after\": {},", claims.not_after);
    println!("  \"role_bitmap\": {},", claims.role_bitmap);
    println!("  \"tags\": [\"gateway\", \"linux\"],");
    println!(
        "  \"role_set_digest_hex\": \"{}\",",
        hexadecimal(&claims.role_set_digest)
    );
    println!("  \"credential_hex\": \"{}\",", hexadecimal(&credential));
    println!(
        "  \"credential_sha256\": \"{}\"",
        hexadecimal(&credential_hash)
    );
    println!("}}");
}

fn hexadecimal(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut encoded, "{byte:02x}").expect("writing to a string cannot fail");
    }
    encoded
}
