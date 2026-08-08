#![no_main]

use std::net::Ipv4Addr;

use ed25519_dalek::SigningKey;
use libfuzzer_sys::fuzz_target;
use xs_protocol::{
    ClientHandshakeParameters, ClientHelloSent, CredentialClaims, EphemeralPrivateKey,
    HandshakeContext, ServerHandshakeParameters, ServerHelloSent, role_set_digest,
    sign_credential,
};

const NOW: u64 = 1_700_000_100;

struct Fixture {
    controller: SigningKey,
    client_identity: SigningKey,
    server_identity: SigningKey,
    client_credential: [u8; 200],
    server_credential: [u8; 200],
    client_context: HandshakeContext,
    server_context: HandshakeContext,
}

fn fixture() -> Fixture {
    let controller = SigningKey::from_bytes(&[11_u8; 32]);
    let client_identity = SigningKey::from_bytes(&[21_u8; 32]);
    let server_identity = SigningKey::from_bytes(&[31_u8; 32]);
    let network_id = [41_u8; 16];
    let client_ip = Ipv4Addr::new(100, 88, 0, 21);
    let server_ip = Ipv4Addr::new(100, 88, 0, 31);
    let role_digest = role_set_digest(1, &["linux".to_owned()]).expect("fixed tag");
    let client_credential = sign_credential(
        CredentialClaims {
            network_id,
            identity_public_key: client_identity.verifying_key().to_bytes(),
            virtual_ipv4: client_ip,
            serial: 21,
            not_before: NOW - 60,
            not_after: NOW + 3_600,
            role_bitmap: 1,
            role_set_digest: role_digest,
        },
        &controller,
    );
    let server_credential = sign_credential(
        CredentialClaims {
            network_id,
            identity_public_key: server_identity.verifying_key().to_bytes(),
            virtual_ipv4: server_ip,
            serial: 31,
            not_before: NOW - 60,
            not_after: NOW + 3_600,
            role_bitmap: 1,
            role_set_digest: role_digest,
        },
        &controller,
    );
    let client_node_id = xs_protocol::node_id(client_identity.verifying_key().as_bytes());
    let server_node_id = xs_protocol::node_id(server_identity.verifying_key().as_bytes());

    Fixture {
        controller,
        client_identity,
        server_identity,
        client_credential,
        server_credential,
        client_context: HandshakeContext {
            network_id,
            local_node_id: client_node_id,
            local_virtual_ip: client_ip,
            peer_node_id: server_node_id,
            peer_virtual_ip: server_ip,
        },
        server_context: HandshakeContext {
            network_id,
            local_node_id: server_node_id,
            local_virtual_ip: server_ip,
            peer_node_id: client_node_id,
            peer_virtual_ip: client_ip,
        },
    }
}

fn client(fixture: &Fixture) -> ClientHelloSent {
    ClientHelloSent::start(
        ClientHandshakeParameters {
            context: fixture.client_context,
            credential: fixture.client_credential,
            ephemeral_private_key: EphemeralPrivateKey::from_bytes([51_u8; 32]),
            client_nonce: [61_u8; 32],
            message_id: 0x1020_3040,
            client_time: NOW,
        },
        &fixture.client_identity,
    )
    .expect("fixed client hello")
}

fn server_parameters(fixture: &Fixture) -> ServerHandshakeParameters {
    ServerHandshakeParameters {
        context: fixture.server_context,
        credential: fixture.server_credential,
        ephemeral_private_key: EphemeralPrivateKey::from_bytes([71_u8; 32]),
        server_nonce: [81_u8; 32],
        session_id: [91_u8; 16],
        server_time: NOW,
    }
}

fuzz_target!(|input: &[u8]| {
    let fixture = fixture();
    let (mode, message) = input.split_first().map_or((0, &[][..]), |(mode, rest)| (*mode, rest));

    match mode % 4 {
        0 => {
            let _ = ServerHelloSent::accept(
                message,
                server_parameters(&fixture),
                &fixture.server_identity,
                &fixture.controller.verifying_key(),
                NOW,
            );
        }
        1 => {
            let _ = client(&fixture).accept_server_hello(
                message,
                &fixture.controller.verifying_key(),
                NOW,
            );
        }
        2 => {
            let client = client(&fixture);
            let server = ServerHelloSent::accept(
                client.encoded(),
                server_parameters(&fixture),
                &fixture.server_identity,
                &fixture.controller.verifying_key(),
                NOW,
            )
            .expect("fixed server hello");
            let _ = server.accept_client_finish(message);
        }
        _ => {
            let client = client(&fixture);
            let server = ServerHelloSent::accept(
                client.encoded(),
                server_parameters(&fixture),
                &fixture.server_identity,
                &fixture.controller.verifying_key(),
                NOW,
            )
            .expect("fixed server hello");
            let client_finish = client
                .accept_server_hello(
                    server.encoded(),
                    &fixture.controller.verifying_key(),
                    NOW,
                )
                .expect("fixed client finish");
            let _ = client_finish.accept_server_finish(message);
        }
    }
});
