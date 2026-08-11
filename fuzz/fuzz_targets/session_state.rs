#![no_main]

use std::net::Ipv4Addr;

use ed25519_dalek::SigningKey;
use libfuzzer_sys::fuzz_target;
use xs_protocol::{
    ClientHandshakeParameters, ClientHelloSent, CredentialClaims, DataFlags, EphemeralPrivateKey,
    HandshakeContext, PacketType, ServerHandshakeParameters, ServerHelloSent, key_update_payload,
    role_set_digest, sign_credential, verify_key_update_payload,
};

const NOW: u64 = 1_700_000_100;
const MAX_FUZZ_PAYLOAD_LENGTH: usize = 256;

struct Fixture {
    controller: SigningKey,
    client_identity: SigningKey,
    server_identity: SigningKey,
    client_credential: [u8; 200],
    server_credential: [u8; 200],
    client_context: HandshakeContext,
    server_context: HandshakeContext,
}

struct InputCursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> InputCursor<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn byte(&mut self) -> u8 {
        let value = self.bytes.get(self.offset).copied().unwrap_or(0);
        self.offset = self.offset.saturating_add(1);
        value
    }

    fn next_u32(&mut self) -> u32 {
        u32::from_be_bytes([self.byte(), self.byte(), self.byte(), self.byte()])
    }

    fn remaining(&mut self, maximum: usize) -> &'a [u8] {
        let start = self.offset.min(self.bytes.len());
        let end = start.saturating_add(maximum).min(self.bytes.len());
        self.offset = end;
        &self.bytes[start..end]
    }
}

fn fixture() -> Fixture {
    let controller = SigningKey::from_bytes(&[11_u8; 32]);
    let client_identity = SigningKey::from_bytes(&[21_u8; 32]);
    let server_identity = SigningKey::from_bytes(&[31_u8; 32]);
    let network_id = [41_u8; 16];
    let client_ip = Ipv4Addr::new(100, 88, 0, 21);
    let server_ip = Ipv4Addr::new(100, 88, 0, 31);
    let role_digest = role_set_digest(1, &["linux".to_owned()]).expect("fixed role set");
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

fn client_parameters(fixture: &Fixture) -> ClientHandshakeParameters {
    ClientHandshakeParameters {
        context: fixture.client_context,
        credential: fixture.client_credential,
        ephemeral_private_key: EphemeralPrivateKey::from_bytes([51_u8; 32]),
        client_nonce: [61_u8; 32],
        message_id: 0x1020_3040,
        client_time: NOW,
    }
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

fn mutate_message(canonical: &[u8], enabled: bool, cursor: &mut InputCursor<'_>) -> Vec<u8> {
    let mut message = canonical.to_vec();
    if !enabled || message.is_empty() {
        return message;
    }

    match cursor.byte() % 4 {
        0 => {
            let index = usize::from(cursor.byte()) % message.len();
            message[index] ^= cursor.byte() | 1;
        }
        1 => {
            let length = usize::from(cursor.byte()) % message.len();
            message.truncate(length);
        }
        2 => {
            let extra_length = usize::from(cursor.byte() % 17);
            message.extend((0..extra_length).map(|_| cursor.byte()));
        }
        _ => {
            let start = usize::from(cursor.byte()) % message.len();
            let available = message.len() - start;
            let length = (usize::from(cursor.byte()) % available).saturating_add(1);
            for byte in &mut message[start..start + length] {
                *byte ^= cursor.byte() | 1;
            }
        }
    }
    message
}

fn ipv4_packet(source: Ipv4Addr, destination: Ipv4Addr, payload: &[u8]) -> Vec<u8> {
    let mut packet = vec![0_u8; 20 + payload.len()];
    let packet_length = u16::try_from(packet.len()).expect("bounded packet length");
    packet[0] = 0x45;
    packet[2..4].copy_from_slice(&packet_length.to_be_bytes());
    packet[8] = 64;
    packet[9] = 17;
    packet[12..16].copy_from_slice(&source.octets());
    packet[16..20].copy_from_slice(&destination.octets());
    packet[20..].copy_from_slice(payload);
    packet
}

fn tamper(encoded: &[u8], selector: u8) -> Vec<u8> {
    let mut tampered = encoded.to_vec();
    let index = usize::from(selector) % tampered.len();
    tampered[index] ^= 1;
    tampered
}

fuzz_target!(|input: &[u8]| {
    let (mutation_mask, remaining) = input
        .split_first()
        .map_or((0, &[][..]), |(mask, rest)| (*mask & 0x0f, rest));
    let mut cursor = InputCursor::new(remaining);
    let fixture = fixture();

    let client = ClientHelloSent::start(client_parameters(&fixture), &fixture.client_identity)
        .expect("fixed client hello");
    let client_hello = mutate_message(client.encoded(), mutation_mask & 0x01 != 0, &mut cursor);
    let Ok(server) = ServerHelloSent::accept(
        &client_hello,
        server_parameters(&fixture),
        &fixture.server_identity,
        &fixture.controller.verifying_key(),
        NOW,
    ) else {
        return;
    };

    let server_hello = mutate_message(server.encoded(), mutation_mask & 0x02 != 0, &mut cursor);
    let Ok(client_finish) =
        client.accept_server_hello(&server_hello, &fixture.controller.verifying_key(), NOW)
    else {
        return;
    };

    let client_finish_message = mutate_message(
        client_finish.encoded(),
        mutation_mask & 0x04 != 0,
        &mut cursor,
    );
    let Ok((server_session, server_finish)) = server.accept_client_finish(&client_finish_message)
    else {
        return;
    };

    let server_finish_message =
        mutate_message(&server_finish, mutation_mask & 0x08 != 0, &mut cursor);
    let Ok(client_session) = client_finish.accept_server_finish(&server_finish_message) else {
        return;
    };

    let session_id = client_session.session_id();
    assert_eq!(session_id, server_session.session_id());
    let (mut client_sender, mut client_receiver) = client_session
        .into_data_plane()
        .expect("established client data plane");
    let (mut server_sender, mut server_receiver) = server_session
        .into_data_plane()
        .expect("established server data plane");

    let path_id = cursor.next_u32();
    let payload = cursor.remaining(MAX_FUZZ_PAYLOAD_LENGTH);
    let client_packet = ipv4_packet(
        fixture.client_context.local_virtual_ip,
        fixture.client_context.peer_virtual_ip,
        payload,
    );
    let server_packet = ipv4_packet(
        fixture.server_context.local_virtual_ip,
        fixture.server_context.peer_virtual_ip,
        payload,
    );

    let client_frame = client_sender
        .seal_ipv4(DataFlags::ACK_ELICITING, path_id, &client_packet)
        .expect("bounded client packet");
    assert!(client_receiver.open(&client_frame).is_err());
    let mut session_confusion = client_frame.clone();
    session_confusion[60] ^= 1;
    assert!(server_receiver.open(&session_confusion).is_err());
    assert!(
        server_receiver
            .open(&tamper(&client_frame, cursor.byte()))
            .is_err()
    );
    assert_eq!(server_receiver.replay_drops_total(), 0);
    let opened = server_receiver
        .open(&client_frame)
        .expect("valid client frame");
    assert_eq!(opened.packet_type, PacketType::Data);
    assert_eq!(opened.plaintext, client_packet);
    assert_eq!(server_receiver.replay_drops_total(), 0);
    assert!(server_receiver.open(&client_frame).is_err());
    assert_eq!(server_receiver.replay_drops_total(), 1);

    let server_frame = server_sender
        .seal_ipv4(DataFlags::ACK_ELICITING, path_id, &server_packet)
        .expect("bounded server packet");
    assert!(server_receiver.open(&server_frame).is_err());
    assert!(
        client_receiver
            .open(&tamper(&server_frame, cursor.byte()))
            .is_err()
    );
    assert_eq!(client_receiver.replay_drops_total(), 0);
    let opened = client_receiver
        .open(&server_frame)
        .expect("valid server frame");
    assert_eq!(opened.packet_type, PacketType::Data);
    assert_eq!(opened.plaintext, server_packet);
    assert_eq!(client_receiver.replay_drops_total(), 0);
    assert!(client_receiver.open(&server_frame).is_err());
    assert_eq!(client_receiver.replay_drops_total(), 1);

    let old_client_before_retire = client_sender
        .seal_ipv4(DataFlags::NONE, path_id, &client_packet)
        .expect("old client epoch packet");
    let old_client_after_retire = client_sender
        .seal_ipv4(DataFlags::NONE, path_id, &client_packet)
        .expect("stale client epoch packet");
    let old_server_before_retire = server_sender
        .seal_ipv4(DataFlags::NONE, path_id, &server_packet)
        .expect("old server epoch packet");
    let old_server_after_retire = server_sender
        .seal_ipv4(DataFlags::NONE, path_id, &server_packet)
        .expect("stale server epoch packet");

    let client_update = key_update_payload(session_id, 0, 1).expect("client key update");
    let server_update = key_update_payload(session_id, 0, 1).expect("server key update");
    assert!(key_update_payload(session_id, 0, 2).is_err());
    let mut invalid_update = client_update;
    let invalid_update_index = usize::from(cursor.byte()) % invalid_update.len();
    invalid_update[invalid_update_index] ^= 1;
    assert!(verify_key_update_payload(&invalid_update, session_id, 0).is_err());

    let client_update_frame = client_sender
        .seal_control(
            PacketType::KeyUpdate,
            DataFlags::CONTROL,
            path_id,
            &client_update,
        )
        .expect("client key update frame");
    let server_update_frame = server_sender
        .seal_control(
            PacketType::KeyUpdate,
            DataFlags::CONTROL,
            path_id,
            &server_update,
        )
        .expect("server key update frame");
    assert!(client_receiver.open(&client_update_frame).is_err());
    assert!(server_receiver.open(&server_update_frame).is_err());
    assert!(
        server_receiver
            .open(&tamper(&client_update_frame, cursor.byte()))
            .is_err()
    );
    assert!(
        client_receiver
            .open(&tamper(&server_update_frame, cursor.byte()))
            .is_err()
    );

    let opened_client_update = server_receiver
        .open(&client_update_frame)
        .expect("authenticated client update");
    let opened_server_update = client_receiver
        .open(&server_update_frame)
        .expect("authenticated server update");
    assert_eq!(opened_client_update.packet_type, PacketType::KeyUpdate);
    assert_eq!(opened_server_update.packet_type, PacketType::KeyUpdate);
    assert_eq!(
        verify_key_update_payload(&opened_client_update.plaintext, session_id, 0),
        Ok(1)
    );
    assert_eq!(
        verify_key_update_payload(&opened_server_update.plaintext, session_id, 0),
        Ok(1)
    );
    let client_update_retry_frame = client_sender
        .seal_control(
            PacketType::KeyUpdate,
            DataFlags::CONTROL,
            path_id,
            &client_update,
        )
        .expect("fresh client key update retry");
    let server_update_retry_frame = server_sender
        .seal_control(
            PacketType::KeyUpdate,
            DataFlags::CONTROL,
            path_id,
            &server_update,
        )
        .expect("fresh server key update retry");
    assert_ne!(client_update_retry_frame, client_update_frame);
    assert_ne!(server_update_retry_frame, server_update_frame);
    let opened_client_retry = server_receiver
        .open(&client_update_retry_frame)
        .expect("fresh client key update retry accepted");
    let opened_server_retry = client_receiver
        .open(&server_update_retry_frame)
        .expect("fresh server key update retry accepted");
    assert_eq!(
        verify_key_update_payload(&opened_client_retry.plaintext, session_id, 0),
        Ok(1)
    );
    assert_eq!(
        verify_key_update_payload(&opened_server_retry.plaintext, session_id, 0),
        Ok(1)
    );
    assert!(server_receiver.open(&client_update_frame).is_err());
    assert!(client_receiver.open(&server_update_frame).is_err());
    assert!(server_receiver.open(&client_update_retry_frame).is_err());
    assert!(client_receiver.open(&server_update_retry_frame).is_err());

    assert!(client_sender.rotate_epoch(0).is_err());
    assert!(client_sender.rotate_epoch(2).is_err());
    assert_eq!(client_sender.current_epoch(), 0);
    assert!(server_sender.rotate_epoch(0).is_err());
    assert!(server_sender.rotate_epoch(2).is_err());
    assert_eq!(server_sender.current_epoch(), 0);
    assert!(server_receiver.install_next_epoch(0).is_err());
    assert!(server_receiver.install_next_epoch(2).is_err());
    assert_eq!(server_receiver.current_epoch(), 0);
    assert!(client_receiver.install_next_epoch(0).is_err());
    assert!(client_receiver.install_next_epoch(2).is_err());
    assert_eq!(client_receiver.current_epoch(), 0);

    server_receiver
        .install_next_epoch(1)
        .expect("install client receive epoch");
    client_receiver
        .install_next_epoch(1)
        .expect("install server receive epoch");
    client_sender.rotate_epoch(1).expect("rotate client epoch");
    server_sender.rotate_epoch(1).expect("rotate server epoch");
    assert_eq!(client_sender.current_epoch(), 1);
    assert_eq!(server_sender.current_epoch(), 1);
    assert_eq!(server_receiver.current_epoch(), 1);
    assert_eq!(client_receiver.current_epoch(), 1);
    assert!(server_receiver.install_next_epoch(3).is_err());
    assert!(client_receiver.install_next_epoch(3).is_err());

    let new_client_frame = client_sender
        .seal_ipv4(DataFlags::NONE, path_id, &client_packet)
        .expect("new client epoch packet");
    assert!(
        server_receiver
            .open(&tamper(&new_client_frame, cursor.byte()))
            .is_err()
    );
    assert!(server_receiver.open(&new_client_frame).is_ok());
    assert!(server_receiver.open(&new_client_frame).is_err());

    let new_server_frame = server_sender
        .seal_ipv4(DataFlags::NONE, path_id, &server_packet)
        .expect("new server epoch packet");
    assert!(
        client_receiver
            .open(&tamper(&new_server_frame, cursor.byte()))
            .is_err()
    );
    assert!(client_receiver.open(&new_server_frame).is_ok());
    assert!(client_receiver.open(&new_server_frame).is_err());

    assert!(server_receiver.open(&old_client_before_retire).is_ok());
    assert!(client_receiver.open(&old_server_before_retire).is_ok());
    server_receiver.retire_previous_epoch();
    client_receiver.retire_previous_epoch();
    assert!(server_receiver.open(&old_client_after_retire).is_err());
    assert!(client_receiver.open(&old_server_after_retire).is_err());
});
