use std::net::Ipv4Addr;

use ed25519_dalek::SigningKey;
use xs_protocol::{
    ClientHandshakeParameters, ClientHelloSent, CredentialClaims, DataFlags, EphemeralPrivateKey,
    HandshakeContext, PacketType, ServerHandshakeParameters, ServerHelloSent, role_set_digest,
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
    let tags = vec!["linux".to_owned()];
    let client_credential = sign_credential(
        CredentialClaims {
            network_id,
            identity_public_key: client_identity.verifying_key().to_bytes(),
            virtual_ipv4: client_ip,
            serial: 21,
            not_before: NOW - 60,
            not_after: NOW + 3_600,
            role_bitmap: 1,
            role_set_digest: role_set_digest(1, &tags).expect("valid role set"),
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
            role_set_digest: role_set_digest(1, &tags).expect("valid role set"),
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

fn client_parameters(fixture: &Fixture, message_id: u32) -> ClientHandshakeParameters {
    ClientHandshakeParameters {
        context: fixture.client_context,
        credential: fixture.client_credential,
        ephemeral_private_key: EphemeralPrivateKey::from_bytes([51_u8; 32]),
        client_nonce: [61_u8; 32],
        message_id,
        client_time: NOW,
    }
}

fn server_parameters(fixture: &Fixture, context: HandshakeContext) -> ServerHandshakeParameters {
    ServerHandshakeParameters {
        context,
        credential: fixture.server_credential,
        ephemeral_private_key: EphemeralPrivateKey::from_bytes([71_u8; 32]),
        server_nonce: [81_u8; 32],
        session_id: [91_u8; 16],
        server_time: NOW,
    }
}

fn complete_handshake() -> (
    xs_protocol::EstablishedSession,
    xs_protocol::EstablishedSession,
) {
    let fixture = fixture();
    let client = ClientHelloSent::start(
        client_parameters(&fixture, 0x1020_3040),
        &fixture.client_identity,
    )
    .expect("client hello");
    let server = ServerHelloSent::accept(
        client.encoded(),
        server_parameters(&fixture, fixture.server_context),
        &fixture.server_identity,
        &fixture.controller.verifying_key(),
        NOW,
    )
    .expect("server hello");
    let client = client
        .accept_server_hello(server.encoded(), &fixture.controller.verifying_key(), NOW)
        .expect("client finish");
    let (server_session, server_finish) = server
        .accept_client_finish(client.encoded())
        .expect("server finish");
    let client_session = client
        .accept_server_finish(&server_finish)
        .expect("established client");
    (client_session, server_session)
}

fn client_and_server_hello(fixture: &Fixture, message_id: u32) -> (ClientHelloSent, Vec<u8>) {
    let client = ClientHelloSent::start(
        client_parameters(fixture, message_id),
        &fixture.client_identity,
    )
    .expect("client hello");
    let server = ServerHelloSent::accept(
        client.encoded(),
        server_parameters(fixture, fixture.server_context),
        &fixture.server_identity,
        &fixture.controller.verifying_key(),
        NOW,
    )
    .expect("server hello");
    (client, server.encoded().to_vec())
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

#[test]
fn four_message_handshake_establishes_bidirectional_encrypted_data() {
    let fixture = fixture();
    let (client_session, server_session) = complete_handshake();
    let (mut client_sender, mut client_receiver) =
        client_session.into_data_plane().expect("client data plane");
    let (mut server_sender, mut server_receiver) =
        server_session.into_data_plane().expect("server data plane");

    let client_packet = ipv4_packet(
        fixture.client_context.local_virtual_ip,
        fixture.client_context.peer_virtual_ip,
    );
    let encrypted = client_sender
        .seal_ipv4(DataFlags::ACK_ELICITING, 7, &client_packet)
        .expect("client data encrypted");
    assert_ne!(&encrypted[96..112], &client_packet[..16]);
    let opened = server_receiver.open(&encrypted).expect("server decrypts");
    assert_eq!(opened.packet_type, PacketType::Data);
    assert_eq!(opened.plaintext, client_packet);

    let server_packet = ipv4_packet(
        fixture.server_context.local_virtual_ip,
        fixture.server_context.peer_virtual_ip,
    );
    let encrypted = server_sender
        .seal_ipv4(DataFlags::ACK_ELICITING, 9, &server_packet)
        .expect("server data encrypted");
    let opened = client_receiver.open(&encrypted).expect("client decrypts");
    assert_eq!(opened.plaintext, server_packet);
}

#[test]
fn handshake_rejects_signature_context_and_finish_tampering() {
    let fixture = fixture();
    let client = ClientHelloSent::start(client_parameters(&fixture, 7), &fixture.client_identity)
        .expect("client hello");
    let mut tampered = client.encoded().to_vec();
    *tampered.last_mut().expect("signature byte") ^= 1;
    assert!(
        ServerHelloSent::accept(
            &tampered,
            server_parameters(&fixture, fixture.server_context),
            &fixture.server_identity,
            &fixture.controller.verifying_key(),
            NOW,
        )
        .is_err()
    );

    let mut wrong_context = fixture.server_context;
    wrong_context.network_id[0] ^= 1;
    assert!(
        ServerHelloSent::accept(
            client.encoded(),
            server_parameters(&fixture, wrong_context),
            &fixture.server_identity,
            &fixture.controller.verifying_key(),
            NOW,
        )
        .is_err()
    );

    let server = ServerHelloSent::accept(
        client.encoded(),
        server_parameters(&fixture, fixture.server_context),
        &fixture.server_identity,
        &fixture.controller.verifying_key(),
        NOW,
    )
    .expect("server hello");
    let client = client
        .accept_server_hello(server.encoded(), &fixture.controller.verifying_key(), NOW)
        .expect("client finish");
    let mut finish = client.encoded().to_vec();
    *finish.last_mut().expect("tag byte") ^= 1;
    assert!(server.accept_client_finish(&finish).is_err());
}

#[test]
fn handshake_rejects_noncanonical_framing_and_unknown_versions() {
    let fixture = fixture();
    let client = ClientHelloSent::start(client_parameters(&fixture, 19), &fixture.client_identity)
        .expect("client hello");
    let canonical = client.encoded().to_vec();
    let client_mutators: [fn(&mut Vec<u8>); 8] = [
        |bytes| bytes[0] ^= 1,
        |bytes| bytes[4] = 2,
        |bytes| bytes[5] = 0x11,
        |bytes| bytes[6] = 1,
        |bytes| bytes[11] ^= 1,
        |bytes| bytes[338] = 0,
        |bytes| {
            bytes.pop();
        },
        |bytes| bytes.push(0),
    ];
    for mutate in client_mutators {
        let mut malformed = canonical.clone();
        mutate(&mut malformed);
        assert!(
            ServerHelloSent::accept(
                &malformed,
                server_parameters(&fixture, fixture.server_context),
                &fixture.server_identity,
                &fixture.controller.verifying_key(),
                NOW,
            )
            .is_err()
        );
    }

    let server_mutators: [fn(&mut Vec<u8>); 6] = [
        |bytes| bytes[4] = 2,
        |bytes| bytes[5] = 0x10,
        |bytes| bytes[7] = 1,
        |bytes| bytes[8] ^= 1,
        |bytes| {
            bytes.pop();
        },
        |bytes| bytes.push(0),
    ];
    for (index, mutate) in server_mutators.into_iter().enumerate() {
        let message_id = 100 + u32::try_from(index).expect("small index");
        let (client, mut malformed) = client_and_server_hello(&fixture, message_id);
        mutate(&mut malformed);
        assert!(
            client
                .accept_server_hello(&malformed, &fixture.controller.verifying_key(), NOW,)
                .is_err()
        );
    }
}

#[test]
fn replay_tamper_and_epoch_windows_fail_closed() {
    let fixture = fixture();
    let (client_session, server_session) = complete_handshake();
    let (mut sender, _) = client_session.into_data_plane().expect("client data plane");
    let (_, mut receiver) = server_session.into_data_plane().expect("server data plane");
    let packet = ipv4_packet(
        fixture.client_context.local_virtual_ip,
        fixture.client_context.peer_virtual_ip,
    );

    let old_first = sender
        .seal_ipv4(DataFlags::NONE, 1, &packet)
        .expect("old epoch packet");
    let old_second = sender
        .seal_ipv4(DataFlags::NONE, 1, &packet)
        .expect("second old epoch packet");
    let mut tampered = old_first.clone();
    *tampered.last_mut().expect("tag") ^= 1;
    assert!(receiver.open(&tampered).is_err());
    assert!(receiver.open(&old_first).is_ok());
    assert!(receiver.open(&old_first).is_err());

    sender.rotate_epoch(1).expect("sender rotates");
    receiver.install_next_epoch(1).expect("receiver rotates");
    let new_epoch = sender
        .seal_ipv4(DataFlags::NONE, 2, &packet)
        .expect("new epoch packet");
    assert!(receiver.open(&new_epoch).is_ok());
    assert!(receiver.open(&old_second).is_ok());
    receiver.retire_previous_epoch();
    let stale = old_second;
    assert!(receiver.open(&stale).is_err());
}

#[test]
fn authenticated_header_and_virtual_source_are_enforced() {
    let fixture = fixture();
    let (client_session, server_session) = complete_handshake();
    let (mut sender, _) = client_session.into_data_plane().expect("client data plane");
    let (_, mut receiver) = server_session.into_data_plane().expect("server data plane");
    let packet = ipv4_packet(
        fixture.client_context.local_virtual_ip,
        fixture.client_context.peer_virtual_ip,
    );
    let encrypted = sender
        .seal_ipv4(DataFlags::NONE, 3, &packet)
        .expect("valid packet");
    let mut header_tamper = encrypted.clone();
    header_tamper[88] ^= 1;
    assert!(receiver.open(&header_tamper).is_err());
    assert!(receiver.open(&encrypted).is_ok());

    let forged_source = ipv4_packet(
        Ipv4Addr::new(100, 88, 0, 99),
        fixture.client_context.peer_virtual_ip,
    );
    assert!(
        sender
            .seal_ipv4(DataFlags::NONE, 3, &forged_source)
            .is_err()
    );
}

#[test]
fn routed_packets_require_the_explicit_policy_guarded_api() {
    let fixture = fixture();
    let (client_session, server_session) = complete_handshake();
    let (mut sender, _) = client_session.into_data_plane().expect("client data plane");
    let (_, mut receiver) = server_session.into_data_plane().expect("server data plane");
    let packet = ipv4_packet(
        fixture.client_context.local_virtual_ip,
        Ipv4Addr::new(192, 168, 232, 2),
    );

    assert!(sender.seal_ipv4(DataFlags::NONE, 4, &packet).is_err());
    let encrypted = sender
        .seal_routed_ipv4(DataFlags::NONE, 4, &packet)
        .expect("authorized routed packet");
    assert!(receiver.open(&encrypted).is_err());
    let opened = receiver
        .open_routed(&encrypted)
        .expect("policy-guarded routed receive");
    assert_eq!(opened.packet_type, PacketType::Data);
    assert_eq!(opened.plaintext, packet);
}
