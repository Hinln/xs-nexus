use std::{fmt, net::Ipv4Addr};

use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce, Tag, aead::AeadInPlace};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::{InvalidProtocolMessage, handshake::hkdf_expand};

pub const DATA_HEADER_LENGTH: usize = 96;
pub const DATA_TAG_LENGTH: usize = 16;
pub const MAX_DATAGRAM_LENGTH: usize = 1500;
pub const MAX_ENCRYPTED_PAYLOAD_LENGTH: usize =
    MAX_DATAGRAM_LENGTH - DATA_HEADER_LENGTH - DATA_TAG_LENGTH;
const MAGIC: &[u8; 4] = b"XSP1";
const VERSION: u8 = 1;
const DATA_HEADER_LENGTH_U16: u16 = 96;
const TRAFFIC_KEY_DOMAIN: &[u8] = b"XSP/1 traffic key v1";
const NONCE_SALT_DOMAIN: &[u8] = b"XSP/1 nonce salt v1";
const KEY_UPDATE_DOMAIN: &[u8] = b"XSP/1 key update v1";
const REPLAY_WINDOW_BITS: usize = 1024;
const REPLAY_WINDOW_WORDS: usize = REPLAY_WINDOW_BITS / 64;
const MAX_SEQUENCE_ADVANCE: u64 = 1 << 20;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum PacketType {
    Data = 0x01,
    Keepalive = 0x02,
    PathChallenge = 0x03,
    PathResponse = 0x04,
    KeyUpdate = 0x05,
    KeyUpdateAck = 0x06,
    Close = 0x07,
}

impl TryFrom<u8> for PacketType {
    type Error = InvalidProtocolMessage;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x01 => Ok(Self::Data),
            0x02 => Ok(Self::Keepalive),
            0x03 => Ok(Self::PathChallenge),
            0x04 => Ok(Self::PathResponse),
            0x05 => Ok(Self::KeyUpdate),
            0x06 => Ok(Self::KeyUpdateAck),
            0x07 => Ok(Self::Close),
            _ => Err(InvalidProtocolMessage),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DataFlags(u16);

impl DataFlags {
    pub const NONE: Self = Self(0);
    pub const ACK_ELICITING: Self = Self(0x0001);
    pub const CONTROL: Self = Self(0x0002);
    pub const PATH_PROBE: Self = Self(0x0004);

    #[must_use]
    pub const fn bits(self) -> u16 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DataHeader {
    pub packet_type: PacketType,
    pub flags: DataFlags,
    pub payload_length: u16,
    pub network_id: [u8; 16],
    pub source_node_id: [u8; 16],
    pub destination_node_id: [u8; 16],
    pub session_id: [u8; 16],
    pub key_epoch: u32,
    pub sequence: u64,
    pub path_id: u32,
}

impl DataHeader {
    #[must_use]
    pub fn encode(self) -> [u8; DATA_HEADER_LENGTH] {
        let mut encoded = [0_u8; DATA_HEADER_LENGTH];
        encoded[0..4].copy_from_slice(MAGIC);
        encoded[4] = VERSION;
        encoded[5] = self.packet_type as u8;
        encoded[6..8].copy_from_slice(&self.flags.bits().to_be_bytes());
        encoded[8..10].copy_from_slice(&DATA_HEADER_LENGTH_U16.to_be_bytes());
        encoded[10..12].copy_from_slice(&self.payload_length.to_be_bytes());
        encoded[12..28].copy_from_slice(&self.network_id);
        encoded[28..44].copy_from_slice(&self.source_node_id);
        encoded[44..60].copy_from_slice(&self.destination_node_id);
        encoded[60..76].copy_from_slice(&self.session_id);
        encoded[76..80].copy_from_slice(&self.key_epoch.to_be_bytes());
        encoded[80..88].copy_from_slice(&self.sequence.to_be_bytes());
        encoded[88..92].copy_from_slice(&self.path_id.to_be_bytes());
        encoded
    }

    /// Parses a strict canonical XSP/1 data header.
    ///
    /// # Errors
    ///
    /// Returns `InvalidProtocolMessage` when framing, version, flags, lengths, or reserved fields are invalid.
    pub fn parse(encoded: &[u8]) -> Result<Self, InvalidProtocolMessage> {
        if encoded.len() < DATA_HEADER_LENGTH
            || &encoded[0..4] != MAGIC
            || encoded[4] != VERSION
            || u16::from_be_bytes(array::<2>(&encoded[8..10])?)
                != u16::try_from(DATA_HEADER_LENGTH).map_err(|_| InvalidProtocolMessage)?
            || encoded[92..96] != [0_u8; 4]
        {
            return Err(InvalidProtocolMessage);
        }
        let packet_type = PacketType::try_from(encoded[5])?;
        let flags = DataFlags(u16::from_be_bytes(array::<2>(&encoded[6..8])?));
        if !valid_flags(packet_type, flags) {
            return Err(InvalidProtocolMessage);
        }
        Ok(Self {
            packet_type,
            flags,
            payload_length: u16::from_be_bytes(array::<2>(&encoded[10..12])?),
            network_id: array::<16>(&encoded[12..28])?,
            source_node_id: array::<16>(&encoded[28..44])?,
            destination_node_id: array::<16>(&encoded[44..60])?,
            session_id: array::<16>(&encoded[60..76])?,
            key_epoch: u32::from_be_bytes(array::<4>(&encoded[76..80])?),
            sequence: u64::from_be_bytes(array::<8>(&encoded[80..88])?),
            path_id: u32::from_be_bytes(array::<4>(&encoded[88..92])?),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenedPacket {
    pub packet_type: PacketType,
    pub flags: DataFlags,
    pub key_epoch: u32,
    pub sequence: u64,
    pub path_id: u32,
    pub plaintext: Vec<u8>,
}

pub struct DataSender {
    direction_secret: Zeroizing<[u8; 32]>,
    epoch: EpochCipher,
    next_sequence: u64,
    network_id: [u8; 16],
    source_node_id: [u8; 16],
    destination_node_id: [u8; 16],
    session_id: [u8; 16],
    source_virtual_ip: Ipv4Addr,
    destination_virtual_ip: Ipv4Addr,
}

pub struct DataReceiver {
    direction_secret: Zeroizing<[u8; 32]>,
    current: EpochCipher,
    previous: Option<EpochCipher>,
    network_id: [u8; 16],
    source_node_id: [u8; 16],
    destination_node_id: [u8; 16],
    session_id: [u8; 16],
    source_virtual_ip: Ipv4Addr,
    destination_virtual_ip: Ipv4Addr,
}

struct EpochCipher {
    epoch: u32,
    key: Zeroizing<[u8; 32]>,
    nonce_salt: [u8; 4],
    replay: ReplayWindow,
}

#[derive(Clone)]
struct ReplayWindow {
    highest: Option<u64>,
    words: [u64; REPLAY_WINDOW_WORDS],
}

impl DataSender {
    pub(crate) fn new(
        direction_secret: Zeroizing<[u8; 32]>,
        network_id: [u8; 16],
        source_node_id: [u8; 16],
        destination_node_id: [u8; 16],
        session_id: [u8; 16],
        source_virtual_ip: Ipv4Addr,
        destination_virtual_ip: Ipv4Addr,
    ) -> Result<Self, InvalidProtocolMessage> {
        let epoch = derive_epoch(&direction_secret, 0)?;
        Ok(Self {
            direction_secret,
            epoch,
            next_sequence: 0,
            network_id,
            source_node_id,
            destination_node_id,
            session_id,
            source_virtual_ip,
            destination_virtual_ip,
        })
    }

    /// Encrypts a complete IPv4 packet after enforcing the session source and destination binding.
    ///
    /// # Errors
    ///
    /// Returns `InvalidProtocolMessage` for malformed IPv4, identity mismatch, size, flags, or sequence exhaustion.
    pub fn seal_ipv4(
        &mut self,
        flags: DataFlags,
        path_id: u32,
        packet: &[u8],
    ) -> Result<Vec<u8>, InvalidProtocolMessage> {
        validate_ipv4(packet, self.source_virtual_ip, self.destination_virtual_ip)?;
        self.seal(PacketType::Data, flags, path_id, packet)
    }

    /// Encrypts a routed IPv4 packet after validating its canonical framing.
    ///
    /// The caller must authorize and bind the inner source and destination to
    /// the authenticated subnet-routing policy before calling this method.
    ///
    /// # Errors
    ///
    /// Returns `InvalidProtocolMessage` for malformed, fragmented, oversized,
    /// invalid-flag, or sequence-exhausted packets.
    pub fn seal_routed_ipv4(
        &mut self,
        flags: DataFlags,
        path_id: u32,
        packet: &[u8],
    ) -> Result<Vec<u8>, InvalidProtocolMessage> {
        validate_ipv4_packet(packet)?;
        self.seal(PacketType::Data, flags, path_id, packet)
    }

    /// Encrypts a bounded authenticated control payload.
    ///
    /// # Errors
    ///
    /// Returns `InvalidProtocolMessage` for data packets, invalid type lengths, flags, size, or sequence exhaustion.
    pub fn seal_control(
        &mut self,
        packet_type: PacketType,
        flags: DataFlags,
        path_id: u32,
        payload: &[u8],
    ) -> Result<Vec<u8>, InvalidProtocolMessage> {
        if packet_type == PacketType::Data {
            return Err(InvalidProtocolMessage);
        }
        validate_payload(packet_type, payload)?;
        self.seal(packet_type, flags, path_id, payload)
    }

    /// Advances the sending direction to exactly the next key epoch.
    ///
    /// # Errors
    ///
    /// Returns `InvalidProtocolMessage` for an epoch jump, wrap, or derivation failure.
    pub fn rotate_epoch(&mut self, next_epoch: u32) -> Result<(), InvalidProtocolMessage> {
        if self.epoch.epoch.checked_add(1) != Some(next_epoch) {
            return Err(InvalidProtocolMessage);
        }
        self.epoch = derive_epoch(&self.direction_secret, next_epoch)?;
        self.next_sequence = 0;
        Ok(())
    }

    #[must_use]
    pub const fn current_epoch(&self) -> u32 {
        self.epoch.epoch
    }

    fn seal(
        &mut self,
        packet_type: PacketType,
        flags: DataFlags,
        path_id: u32,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, InvalidProtocolMessage> {
        if !valid_flags(packet_type, flags)
            || plaintext.len() > MAX_ENCRYPTED_PAYLOAD_LENGTH
            || self.next_sequence == u64::MAX
        {
            return Err(InvalidProtocolMessage);
        }
        validate_payload(packet_type, plaintext)?;
        let payload_length = u16::try_from(plaintext.len()).map_err(|_| InvalidProtocolMessage)?;
        let header = DataHeader {
            packet_type,
            flags,
            payload_length,
            network_id: self.network_id,
            source_node_id: self.source_node_id,
            destination_node_id: self.destination_node_id,
            session_id: self.session_id,
            key_epoch: self.epoch.epoch,
            sequence: self.next_sequence,
            path_id,
        };
        let encoded_header = header.encode();
        let nonce = packet_nonce(self.epoch.nonce_salt, self.next_sequence);
        let cipher = ChaCha20Poly1305::new_from_slice(&self.epoch.key[..])
            .map_err(|_| InvalidProtocolMessage)?;
        let mut ciphertext = plaintext.to_vec();
        let tag = cipher
            .encrypt_in_place_detached(Nonce::from_slice(&nonce), &encoded_header, &mut ciphertext)
            .map_err(|_| InvalidProtocolMessage)?;
        let mut encoded =
            Vec::with_capacity(DATA_HEADER_LENGTH + ciphertext.len() + DATA_TAG_LENGTH);
        encoded.extend_from_slice(&encoded_header);
        encoded.extend_from_slice(&ciphertext);
        encoded.extend_from_slice(&tag);
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or(InvalidProtocolMessage)?;
        Ok(encoded)
    }
}

impl DataReceiver {
    pub(crate) fn new(
        direction_secret: Zeroizing<[u8; 32]>,
        network_id: [u8; 16],
        source_node_id: [u8; 16],
        destination_node_id: [u8; 16],
        session_id: [u8; 16],
        source_virtual_ip: Ipv4Addr,
        destination_virtual_ip: Ipv4Addr,
    ) -> Result<Self, InvalidProtocolMessage> {
        let current = derive_epoch(&direction_secret, 0)?;
        Ok(Self {
            direction_secret,
            current,
            previous: None,
            network_id,
            source_node_id,
            destination_node_id,
            session_id,
            source_virtual_ip,
            destination_virtual_ip,
        })
    }

    /// Authenticates, decrypts, replay-checks, and validates one XSP/1 packet.
    ///
    /// # Errors
    ///
    /// Returns `InvalidProtocolMessage` for any framing, identity, epoch, replay, tag, or payload failure.
    pub fn open(&mut self, encoded: &[u8]) -> Result<OpenedPacket, InvalidProtocolMessage> {
        self.open_with_routed_addresses(encoded, false)
    }

    /// Authenticates and decrypts a routed packet without requiring the inner
    /// addresses to equal the two peers' virtual addresses.
    ///
    /// The caller must authorize and bind decrypted routed addresses to the
    /// authenticated peer and current subnet-routing policy before forwarding.
    ///
    /// # Errors
    ///
    /// Returns `InvalidProtocolMessage` for any framing, identity, epoch,
    /// replay, tag, payload, or IPv4 structure failure.
    pub fn open_routed(&mut self, encoded: &[u8]) -> Result<OpenedPacket, InvalidProtocolMessage> {
        self.open_with_routed_addresses(encoded, true)
    }

    fn open_with_routed_addresses(
        &mut self,
        encoded: &[u8],
        allow_routed_addresses: bool,
    ) -> Result<OpenedPacket, InvalidProtocolMessage> {
        if encoded.len() < DATA_HEADER_LENGTH + DATA_TAG_LENGTH
            || encoded.len() > MAX_DATAGRAM_LENGTH
        {
            return Err(InvalidProtocolMessage);
        }
        let header = DataHeader::parse(encoded)?;
        let payload_length = usize::from(header.payload_length);
        let expected_length = DATA_HEADER_LENGTH
            .checked_add(payload_length)
            .and_then(|length| length.checked_add(DATA_TAG_LENGTH))
            .ok_or(InvalidProtocolMessage)?;
        if expected_length != encoded.len()
            || header.network_id != self.network_id
            || header.source_node_id != self.source_node_id
            || header.destination_node_id != self.destination_node_id
            || header.session_id != self.session_id
        {
            return Err(InvalidProtocolMessage);
        }
        let state = if header.key_epoch == self.current.epoch {
            &mut self.current
        } else if self
            .previous
            .as_ref()
            .is_some_and(|previous| previous.epoch == header.key_epoch)
        {
            self.previous.as_mut().ok_or(InvalidProtocolMessage)?
        } else {
            return Err(InvalidProtocolMessage);
        };
        state.replay.precheck(header.sequence)?;
        let ciphertext_end = DATA_HEADER_LENGTH + payload_length;
        let mut plaintext = encoded[DATA_HEADER_LENGTH..ciphertext_end].to_vec();
        let tag = Tag::from_slice(&encoded[ciphertext_end..]);
        let nonce = packet_nonce(state.nonce_salt, header.sequence);
        let cipher =
            ChaCha20Poly1305::new_from_slice(&state.key[..]).map_err(|_| InvalidProtocolMessage)?;
        cipher
            .decrypt_in_place_detached(
                Nonce::from_slice(&nonce),
                &encoded[..DATA_HEADER_LENGTH],
                &mut plaintext,
                tag,
            )
            .map_err(|_| InvalidProtocolMessage)?;
        validate_payload(header.packet_type, &plaintext)?;
        if header.packet_type == PacketType::Data {
            if allow_routed_addresses {
                validate_ipv4_packet(&plaintext)?;
            } else {
                validate_ipv4(
                    &plaintext,
                    self.source_virtual_ip,
                    self.destination_virtual_ip,
                )?;
            }
        }
        state.replay.commit(header.sequence)?;
        Ok(OpenedPacket {
            packet_type: header.packet_type,
            flags: header.flags,
            key_epoch: header.key_epoch,
            sequence: header.sequence,
            path_id: header.path_id,
            plaintext,
        })
    }

    /// Installs exactly the next receive epoch while retaining the old window temporarily.
    ///
    /// # Errors
    ///
    /// Returns `InvalidProtocolMessage` for an epoch jump, wrap, or derivation failure.
    pub fn install_next_epoch(&mut self, next_epoch: u32) -> Result<(), InvalidProtocolMessage> {
        if self.current.epoch.checked_add(1) != Some(next_epoch) {
            return Err(InvalidProtocolMessage);
        }
        let next = derive_epoch(&self.direction_secret, next_epoch)?;
        let previous = std::mem::replace(&mut self.current, next);
        self.previous = Some(previous);
        Ok(())
    }

    pub fn retire_previous_epoch(&mut self) {
        self.previous = None;
    }

    #[must_use]
    pub const fn current_epoch(&self) -> u32 {
        self.current.epoch
    }
}

impl fmt::Debug for DataSender {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DataSender")
            .field("epoch", &self.epoch.epoch)
            .field("next_sequence", &self.next_sequence)
            .field("secret", &"[redacted]")
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for DataReceiver {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DataReceiver")
            .field("current_epoch", &self.current.epoch)
            .field(
                "previous_epoch",
                &self.previous.as_ref().map(|state| state.epoch),
            )
            .field("secret", &"[redacted]")
            .finish_non_exhaustive()
    }
}

/// Creates the fixed 36-byte authenticated `KeyUpdate` payload.
///
/// # Errors
///
/// Returns `InvalidProtocolMessage` unless `next_epoch` is exactly `current_epoch + 1`.
pub fn key_update_payload(
    session_id: [u8; 16],
    current_epoch: u32,
    next_epoch: u32,
) -> Result<[u8; 36], InvalidProtocolMessage> {
    if current_epoch.checked_add(1) != Some(next_epoch) {
        return Err(InvalidProtocolMessage);
    }
    let mut hash = Sha256::new();
    hash.update(KEY_UPDATE_DOMAIN);
    hash.update(session_id);
    hash.update(current_epoch.to_be_bytes());
    hash.update(next_epoch.to_be_bytes());
    let mut payload = [0_u8; 36];
    payload[..4].copy_from_slice(&next_epoch.to_be_bytes());
    payload[4..].copy_from_slice(&hash.finalize());
    Ok(payload)
}

/// Verifies one fixed `KeyUpdate` payload and returns the authenticated next epoch.
///
/// # Errors
///
/// Returns `InvalidProtocolMessage` for length, epoch, or digest failures.
pub fn verify_key_update_payload(
    payload: &[u8],
    session_id: [u8; 16],
    current_epoch: u32,
) -> Result<u32, InvalidProtocolMessage> {
    let bytes: &[u8; 36] = payload.try_into().map_err(|_| InvalidProtocolMessage)?;
    let next_epoch = u32::from_be_bytes(array::<4>(&bytes[..4])?);
    let expected = key_update_payload(session_id, current_epoch, next_epoch)?;
    if bytes != &expected {
        return Err(InvalidProtocolMessage);
    }
    Ok(next_epoch)
}

impl ReplayWindow {
    const fn new() -> Self {
        Self {
            highest: None,
            words: [0_u64; REPLAY_WINDOW_WORDS],
        }
    }

    fn precheck(&self, sequence: u64) -> Result<(), InvalidProtocolMessage> {
        let Some(highest) = self.highest else {
            if sequence > MAX_SEQUENCE_ADVANCE {
                return Err(InvalidProtocolMessage);
            }
            return Ok(());
        };
        if sequence > highest {
            if sequence - highest > MAX_SEQUENCE_ADVANCE {
                return Err(InvalidProtocolMessage);
            }
            return Ok(());
        }
        let offset = highest - sequence;
        if offset >= 1024 {
            return Err(InvalidProtocolMessage);
        }
        let offset = usize::try_from(offset).map_err(|_| InvalidProtocolMessage)?;
        if self.bit_is_set(offset) {
            return Err(InvalidProtocolMessage);
        }
        Ok(())
    }

    fn commit(&mut self, sequence: u64) -> Result<(), InvalidProtocolMessage> {
        self.precheck(sequence)?;
        match self.highest {
            None => {
                self.highest = Some(sequence);
                self.words[0] = 1;
            }
            Some(highest) if sequence > highest => {
                let distance =
                    usize::try_from(sequence - highest).map_err(|_| InvalidProtocolMessage)?;
                self.shift(distance);
                self.highest = Some(sequence);
                self.words[0] |= 1;
            }
            Some(highest) => {
                let offset =
                    usize::try_from(highest - sequence).map_err(|_| InvalidProtocolMessage)?;
                self.words[offset / 64] |= 1_u64 << (offset % 64);
            }
        }
        Ok(())
    }

    fn bit_is_set(&self, offset: usize) -> bool {
        self.words[offset / 64] & (1_u64 << (offset % 64)) != 0
    }

    fn shift(&mut self, distance: usize) {
        if distance >= REPLAY_WINDOW_BITS {
            self.words.fill(0);
            return;
        }
        let old = self.words;
        self.words.fill(0);
        let word_shift = distance / 64;
        let bit_shift = distance % 64;
        for (source, word) in old.into_iter().enumerate() {
            let target = source + word_shift;
            if target >= REPLAY_WINDOW_WORDS {
                break;
            }
            self.words[target] |= word << bit_shift;
            if bit_shift != 0 && target + 1 < REPLAY_WINDOW_WORDS {
                self.words[target + 1] |= word >> (64 - bit_shift);
            }
        }
    }
}

fn derive_epoch(
    direction_secret: &[u8; 32],
    epoch: u32,
) -> Result<EpochCipher, InvalidProtocolMessage> {
    let encoded_epoch = epoch.to_be_bytes();
    Ok(EpochCipher {
        epoch,
        key: Zeroizing::new(hkdf_expand::<32>(
            direction_secret,
            TRAFFIC_KEY_DOMAIN,
            &encoded_epoch,
        )?),
        nonce_salt: hkdf_expand::<4>(direction_secret, NONCE_SALT_DOMAIN, &encoded_epoch)?,
        replay: ReplayWindow::new(),
    })
}

fn packet_nonce(nonce_salt: [u8; 4], sequence: u64) -> [u8; 12] {
    let mut nonce = [0_u8; 12];
    nonce[..4].copy_from_slice(&nonce_salt);
    nonce[4..].copy_from_slice(&sequence.to_be_bytes());
    nonce
}

fn valid_flags(packet_type: PacketType, flags: DataFlags) -> bool {
    let bits = flags.bits();
    if bits & !0x0007 != 0 {
        return false;
    }
    match packet_type {
        PacketType::Data | PacketType::Keepalive => bits & !DataFlags::ACK_ELICITING.bits() == 0,
        PacketType::PathChallenge | PacketType::PathResponse => {
            bits & DataFlags::PATH_PROBE.bits() != 0 && bits & DataFlags::CONTROL.bits() == 0
        }
        PacketType::KeyUpdate | PacketType::KeyUpdateAck | PacketType::Close => {
            bits & DataFlags::CONTROL.bits() != 0 && bits & DataFlags::PATH_PROBE.bits() == 0
        }
    }
}

fn validate_payload(packet_type: PacketType, payload: &[u8]) -> Result<(), InvalidProtocolMessage> {
    let valid = match packet_type {
        PacketType::Data => !payload.is_empty(),
        PacketType::Keepalive => payload.is_empty(),
        PacketType::PathChallenge | PacketType::PathResponse => payload.len() == 8,
        PacketType::KeyUpdate | PacketType::KeyUpdateAck => payload.len() == 36,
        PacketType::Close => (2..=258).contains(&payload.len()),
    };
    if !valid {
        return Err(InvalidProtocolMessage);
    }
    Ok(())
}

fn validate_ipv4(
    packet: &[u8],
    expected_source: Ipv4Addr,
    expected_destination: Ipv4Addr,
) -> Result<(), InvalidProtocolMessage> {
    let (source, destination) = validate_ipv4_packet(packet)?;
    if source != expected_source || destination != expected_destination {
        return Err(InvalidProtocolMessage);
    }
    Ok(())
}

fn validate_ipv4_packet(packet: &[u8]) -> Result<(Ipv4Addr, Ipv4Addr), InvalidProtocolMessage> {
    if packet.len() < 20 || packet[0] >> 4 != 4 {
        return Err(InvalidProtocolMessage);
    }
    let header_length = usize::from(packet[0] & 0x0f) * 4;
    let total_length = usize::from(u16::from_be_bytes(array::<2>(&packet[2..4])?));
    let fragment = u16::from_be_bytes(array::<2>(&packet[6..8])?);
    let source = Ipv4Addr::from(array::<4>(&packet[12..16])?);
    let destination = Ipv4Addr::from(array::<4>(&packet[16..20])?);
    if header_length < 20
        || header_length > packet.len()
        || total_length != packet.len()
        || fragment & 0x3fff != 0
    {
        return Err(InvalidProtocolMessage);
    }
    Ok((source, destination))
}

fn array<const LENGTH: usize>(bytes: &[u8]) -> Result<[u8; LENGTH], InvalidProtocolMessage> {
    bytes.try_into().map_err(|_| InvalidProtocolMessage)
}

#[cfg(test)]
mod tests {
    use std::{fs, time::Instant};

    use super::*;

    #[test]
    fn replay_window_enforces_exact_1024_packet_boundary() {
        let mut window = ReplayWindow::new();
        window.commit(1023).expect("first high sequence");
        assert!(window.precheck(0).is_ok());
        window.commit(1024).expect("advance one");
        assert!(window.precheck(0).is_err());
        assert!(window.precheck(1).is_ok());
    }

    #[test]
    fn replay_window_shift_preserves_marked_sequences() {
        let mut window = ReplayWindow::new();
        window.commit(0).expect("sequence zero");
        window.commit(65).expect("advance across a word");
        assert!(window.precheck(0).is_err());
        assert!(window.precheck(64).is_ok());
        window.commit(64).expect("out of order");
        assert!(window.precheck(64).is_err());
    }

    #[test]
    fn key_update_binds_session_and_adjacent_epoch() {
        let payload = key_update_payload([7_u8; 16], 9, 10).expect("key update");
        assert_eq!(verify_key_update_payload(&payload, [7_u8; 16], 9), Ok(10));
        assert!(verify_key_update_payload(&payload, [8_u8; 16], 9).is_err());
        assert!(key_update_payload([7_u8; 16], 9, 11).is_err());
        let mut tampered = payload;
        tampered[35] ^= 1;
        assert!(verify_key_update_payload(&tampered, [7_u8; 16], 9).is_err());
    }

    #[test]
    fn packet_nonce_separates_salt_and_sequence() {
        let first = packet_nonce([1, 2, 3, 4], 0);
        assert_eq!(first, [1, 2, 3, 4, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert_ne!(first, packet_nonce([1, 2, 3, 4], 1));
        assert_ne!(first, packet_nonce([1, 2, 3, 5], 0));
    }

    #[test]
    fn sequence_and_epoch_exhaustion_fail_without_state_change() {
        let source = Ipv4Addr::new(100, 96, 0, 16);
        let destination = Ipv4Addr::new(100, 96, 0, 17);
        let ids = ([41_u8; 16], [42_u8; 16], [43_u8; 16], [44_u8; 16]);
        let secret = Zeroizing::new([45_u8; 32]);
        let mut sender = DataSender::new(
            secret.clone(),
            ids.0,
            ids.1,
            ids.2,
            ids.3,
            source,
            destination,
        )
        .expect("sender");
        let mut packet = vec![0_u8; 20];
        packet[0] = 0x45;
        packet[2..4].copy_from_slice(&20_u16.to_be_bytes());
        packet[12..16].copy_from_slice(&source.octets());
        packet[16..20].copy_from_slice(&destination.octets());

        sender.next_sequence = u64::MAX;
        assert!(sender.seal_ipv4(DataFlags::NONE, 0, &packet).is_err());
        assert_eq!(sender.next_sequence, u64::MAX);
        assert_eq!(sender.current_epoch(), 0);

        sender.epoch.epoch = u32::MAX;
        assert!(sender.rotate_epoch(0).is_err());
        assert_eq!(sender.current_epoch(), u32::MAX);

        let mut receiver =
            DataReceiver::new(secret, ids.0, ids.1, ids.2, ids.3, source, destination)
                .expect("receiver");
        receiver.current.epoch = u32::MAX;
        assert!(receiver.install_next_epoch(0).is_err());
        assert_eq!(receiver.current_epoch(), u32::MAX);
        assert!(receiver.previous.is_none());
    }

    #[test]
    fn encryption_throughput_baseline() {
        const ITERATIONS: u32 = 50_000;
        let source = Ipv4Addr::new(100, 96, 0, 16);
        let destination = Ipv4Addr::new(100, 96, 0, 17);
        let mut packet = vec![0_u8; 1200];
        packet[0] = 0x45;
        let packet_length = u16::try_from(packet.len()).expect("packet length fits u16");
        packet[2..4].copy_from_slice(&packet_length.to_be_bytes());
        packet[12..16].copy_from_slice(&source.octets());
        packet[16..20].copy_from_slice(&destination.octets());
        for (index, byte) in packet[20..].iter_mut().enumerate() {
            *byte = u8::try_from(index % 256)
                .expect("modulo byte index")
                .wrapping_mul(31);
        }
        let ids = ([41_u8; 16], [42_u8; 16], [43_u8; 16], [44_u8; 16]);
        let secret = Zeroizing::new([45_u8; 32]);
        let mut sender = DataSender::new(
            secret.clone(),
            ids.0,
            ids.1,
            ids.2,
            ids.3,
            source,
            destination,
        )
        .expect("sender");
        let mut receiver =
            DataReceiver::new(secret, ids.0, ids.1, ids.2, ids.3, source, destination)
                .expect("receiver");
        let started = Instant::now();
        let mut bytes = 0_u32;
        for _ in 0..ITERATIONS {
            let encrypted = sender
                .seal_ipv4(DataFlags::ACK_ELICITING, 7, &packet)
                .expect("encrypt");
            let opened = receiver.open(&encrypted).expect("decrypt");
            assert_eq!(opened.plaintext, packet);
            bytes += u32::try_from(packet.len()).expect("packet length fits u32");
        }
        let elapsed = started.elapsed();
        let report = serde_json::json!({
            "iterations": ITERATIONS,
            "plaintext_bytes": bytes,
            "elapsed_ms": elapsed.as_secs_f64() * 1000.0,
            "encrypt_decrypt_ops_per_second": f64::from(ITERATIONS) / elapsed.as_secs_f64(),
            "plaintext_mib_per_second": f64::from(bytes) / elapsed.as_secs_f64() / 1_048_576.0,
            "packet_bytes": packet.len(),
        });
        if let Some(path) = std::env::var_os("XS_PROTOCOL_THROUGHPUT_REPORT") {
            fs::write(
                path,
                serde_json::to_vec_pretty(&report).expect("serialize throughput report"),
            )
            .expect("write throughput report");
        }
        println!(
            "{}",
            serde_json::to_string(&report).expect("throughput JSON")
        );
    }
}
