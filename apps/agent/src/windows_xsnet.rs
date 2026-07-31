use thiserror::Error;

const ABI_MAGIC: u32 = 0x314e_5358;
const ABI_VERSION: u16 = 1;
const HEADER_SIZE: usize = 32;
const HEADER_SIZE_U16: u16 = 32;
const MAX_PAYLOAD: usize = 1_048_576;
const MAX_PACKETS: usize = 64;
const MAX_PACKETS_U16: u16 = 64;
const MIN_PACKET_SIZE: usize = 20;
const MAX_PACKET_SIZE: usize = 9000;
const MAX_PACKET_SIZE_U32: u32 = 9000;
const CAPABILITY_IPV4: u32 = 1;
const DEVICE_TYPE: u32 = 0x8337;
const IOCTL_ACCESS: u32 = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum MessageType {
    Hello = 1,
    Attach = 2,
    SetLink = 3,
    TransmitBatch = 4,
    ReceiveBatch = 5,
    Detach = 6,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientState {
    Opened,
    Negotiated,
    Attached,
    LinkUp,
    ReconnectRequired,
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ClientError {
    #[error("xsnet client state does not allow this operation")]
    BadState,
    #[error("another xsnet request is already active")]
    RequestActive,
    #[error("xsnet request completion does not match the active request")]
    CompletionMismatch,
    #[error("xsnet sequence is exhausted and the handle must be reopened")]
    SequenceExhausted,
    #[error("xsnet packet or negotiated limit is invalid")]
    InvalidPacket,
    #[error("xsnet driver response is malformed")]
    InvalidResponse,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedRequest {
    ioctl: u32,
    sequence: u64,
    buffered_input: Vec<u8>,
    direct_input: Vec<u8>,
    direct_output_capacity: usize,
    message_type: MessageType,
    transition: ClientState,
}

impl PreparedRequest {
    #[must_use]
    pub const fn ioctl(&self) -> u32 {
        self.ioctl
    }

    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    #[must_use]
    pub fn buffered_input(&self) -> &[u8] {
        &self.buffered_input
    }

    #[must_use]
    pub fn direct_input(&self) -> &[u8] {
        &self.direct_input
    }

    #[must_use]
    pub const fn direct_output_capacity(&self) -> usize {
        self.direct_output_capacity
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransportOutcome {
    Success(Vec<u8>),
    Rejected(u32),
    Indeterminate,
}

pub trait XsnetTransport {
    fn execute(&mut self, request: &PreparedRequest) -> TransportOutcome;
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ExecutionError {
    #[error(transparent)]
    Client(#[from] ClientError),
    #[error("xsnet driver rejected the request with status {0:#010x}")]
    Rejected(u32),
    #[error("xsnet request outcome is indeterminate and the handle must be reopened")]
    ReconnectRequired,
}

#[derive(Debug)]
pub struct XsnetClient {
    state: ClientState,
    next_sequence: u64,
    pending: Option<(u64, MessageType, ClientState)>,
    mtu: u32,
    transmit_depth: u16,
    receive_depth: u16,
}

impl XsnetClient {
    #[must_use]
    pub const fn opened() -> Self {
        Self {
            state: ClientState::Opened,
            next_sequence: 1,
            pending: None,
            mtu: 0,
            transmit_depth: 0,
            receive_depth: 0,
        }
    }

    #[must_use]
    pub const fn state(&self) -> ClientState {
        self.state
    }

    #[must_use]
    pub const fn next_sequence(&self) -> u64 {
        self.next_sequence
    }

    /// Builds the first version and capability negotiation request.
    ///
    /// # Errors
    ///
    /// Returns an error when the client is not opened, another request is active, or sequence space is exhausted.
    pub fn prepare_hello(&mut self) -> Result<PreparedRequest, ClientError> {
        self.require_state(&[ClientState::Opened])?;
        let mut payload = vec![0_u8; 16];
        write_u16(&mut payload, 0, ABI_VERSION);
        write_u16(&mut payload, 2, ABI_VERSION);
        write_u32(&mut payload, 4, CAPABILITY_IPV4);
        self.prepare(
            MessageType::Hello,
            &payload,
            &[],
            0,
            ClientState::Negotiated,
        )
    }

    /// Builds an attach request without committing negotiated limits.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid limits, an invalid state, an active request, or exhausted sequence space.
    pub fn prepare_attach(
        &mut self,
        mtu: u32,
        transmit_depth: u16,
        receive_depth: u16,
    ) -> Result<PreparedRequest, ClientError> {
        self.require_state(&[ClientState::Negotiated])?;
        if !(1280..=MAX_PACKET_SIZE_U32).contains(&mtu)
            || !(1..=MAX_PACKETS_U16).contains(&transmit_depth)
            || !(1..=MAX_PACKETS_U16).contains(&receive_depth)
        {
            return Err(ClientError::InvalidPacket);
        }
        let mut payload = vec![0_u8; 16];
        write_u32(&mut payload, 0, mtu);
        write_u16(&mut payload, 4, transmit_depth);
        write_u16(&mut payload, 6, receive_depth);
        write_u32(&mut payload, 8, CAPABILITY_IPV4);
        self.prepare(MessageType::Attach, &payload, &[], 0, ClientState::Attached)
    }

    /// Builds a virtual-link state request.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid state, an active request, or exhausted sequence space.
    pub fn prepare_set_link(&mut self, link_up: bool) -> Result<PreparedRequest, ClientError> {
        self.require_state(&[ClientState::Attached, ClientState::LinkUp])?;
        let mut payload = vec![0_u8; 8];
        write_u32(&mut payload, 0, u32::from(link_up));
        self.prepare(
            MessageType::SetLink,
            &payload,
            &[],
            0,
            if link_up {
                ClientState::LinkUp
            } else {
                ClientState::Attached
            },
        )
    }

    /// Builds a bounded TX dequeue request.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid output capacity or client state.
    pub fn prepare_transmit(
        &mut self,
        output_capacity: usize,
    ) -> Result<PreparedRequest, ClientError> {
        self.require_state(&[ClientState::LinkUp])?;
        if !(HEADER_SIZE..=HEADER_SIZE + MAX_PAYLOAD).contains(&output_capacity) {
            return Err(ClientError::InvalidPacket);
        }
        self.prepare(
            MessageType::TransmitBatch,
            &[],
            &[],
            output_capacity,
            ClientState::LinkUp,
        )
    }

    /// Builds a canonical RX packet batch.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid IPv4 packets, queue limits, or client state.
    pub fn prepare_receive(&mut self, packets: &[&[u8]]) -> Result<PreparedRequest, ClientError> {
        self.require_state(&[ClientState::LinkUp])?;
        if packets.len() > usize::from(self.receive_depth) {
            return Err(ClientError::InvalidPacket);
        }
        let payload = encode_packet_batch(packets, self.mtu)?;
        self.prepare(
            MessageType::ReceiveBatch,
            &[],
            &payload,
            0,
            ClientState::LinkUp,
        )
    }

    /// Builds an idempotent virtual-link detach request.
    ///
    /// # Errors
    ///
    /// Returns an error for an invalid state, an active request, or exhausted sequence space.
    pub fn prepare_detach(&mut self) -> Result<PreparedRequest, ClientError> {
        self.require_state(&[
            ClientState::Negotiated,
            ClientState::Attached,
            ClientState::LinkUp,
        ])?;
        self.prepare(MessageType::Detach, &[], &[], 0, ClientState::Negotiated)
    }

    /// Commits a request only after validating its successful response.
    ///
    /// # Errors
    ///
    /// Returns an error when the completion is mismatched or the driver response is malformed.
    pub fn complete_success(
        &mut self,
        request: &PreparedRequest,
        response: &[u8],
    ) -> Result<Vec<Vec<u8>>, ClientError> {
        self.require_pending(request)?;
        let packets = if request.message_type == MessageType::TransmitBatch {
            decode_transmit_response(response, request.sequence, self.mtu, self.transmit_depth)?
        } else {
            if !response.is_empty() {
                return Err(ClientError::InvalidResponse);
            }
            Vec::new()
        };
        let attach_configuration = if request.message_type == MessageType::Attach {
            Some((
                read_u32(&request.buffered_input, HEADER_SIZE)
                    .ok_or(ClientError::CompletionMismatch)?,
                read_u16(&request.buffered_input, HEADER_SIZE + 4)
                    .ok_or(ClientError::CompletionMismatch)?,
                read_u16(&request.buffered_input, HEADER_SIZE + 6)
                    .ok_or(ClientError::CompletionMismatch)?,
            ))
        } else {
            None
        };
        self.pending = None;
        self.state = request.transition;
        if let Some((mtu, transmit_depth, receive_depth)) = attach_configuration {
            self.mtu = mtu;
            self.transmit_depth = transmit_depth;
            self.receive_depth = receive_depth;
        } else if request.message_type == MessageType::Detach {
            self.mtu = 0;
            self.transmit_depth = 0;
            self.receive_depth = 0;
        }
        self.next_sequence = request
            .sequence
            .checked_add(1)
            .ok_or(ClientError::SequenceExhausted)?;
        Ok(packets)
    }

    /// Clears a request after a definitive driver rejection without advancing sequence or state.
    ///
    /// # Errors
    ///
    /// Returns an error when the completion does not match the active request.
    pub fn complete_rejected(&mut self, request: &PreparedRequest) -> Result<(), ClientError> {
        self.require_pending(request)?;
        self.pending = None;
        Ok(())
    }

    /// Poisons the handle after an outcome whose sequence commit is unknown.
    ///
    /// # Errors
    ///
    /// Returns an error when the completion does not match the active request.
    pub fn complete_indeterminate(&mut self, request: &PreparedRequest) -> Result<(), ClientError> {
        self.require_pending(request)?;
        self.pending = None;
        self.state = ClientState::ReconnectRequired;
        Ok(())
    }

    /// Executes one prepared request and applies its failure classification atomically.
    ///
    /// # Errors
    ///
    /// Returns a client validation error, an explicit driver rejection, or a reconnect requirement.
    pub fn execute_prepared<T: XsnetTransport>(
        &mut self,
        transport: &mut T,
        request: &PreparedRequest,
    ) -> Result<Vec<Vec<u8>>, ExecutionError> {
        self.require_pending(request)?;
        match transport.execute(request) {
            TransportOutcome::Success(response) => {
                match self.complete_success(request, &response) {
                    Ok(packets) => Ok(packets),
                    Err(error) => {
                        self.complete_indeterminate(request)?;
                        Err(ExecutionError::Client(error))
                    }
                }
            }
            TransportOutcome::Rejected(status) => {
                self.complete_rejected(request)?;
                Err(ExecutionError::Rejected(status))
            }
            TransportOutcome::Indeterminate => {
                self.complete_indeterminate(request)?;
                Err(ExecutionError::ReconnectRequired)
            }
        }
    }

    fn require_state(&self, allowed: &[ClientState]) -> Result<(), ClientError> {
        if self.pending.is_some() {
            return Err(ClientError::RequestActive);
        }
        if !allowed.contains(&self.state) {
            return Err(ClientError::BadState);
        }
        Ok(())
    }

    fn prepare(
        &mut self,
        message_type: MessageType,
        payload: &[u8],
        direct_payload: &[u8],
        direct_output_capacity: usize,
        transition: ClientState,
    ) -> Result<PreparedRequest, ClientError> {
        if self.next_sequence == u64::MAX {
            return Err(ClientError::SequenceExhausted);
        }
        let sequence = self.next_sequence;
        let buffered_input = if message_type == MessageType::ReceiveBatch {
            Vec::new()
        } else {
            encode_message(message_type, sequence, payload)?
        };
        let direct_input = if message_type == MessageType::ReceiveBatch {
            encode_message(message_type, sequence, direct_payload)?
        } else {
            Vec::new()
        };
        self.pending = Some((sequence, message_type, transition));
        Ok(PreparedRequest {
            ioctl: ioctl_for(message_type),
            sequence,
            buffered_input,
            direct_input,
            direct_output_capacity,
            message_type,
            transition,
        })
    }

    fn require_pending(&self, request: &PreparedRequest) -> Result<(), ClientError> {
        if self.pending == Some((request.sequence, request.message_type, request.transition)) {
            Ok(())
        } else {
            Err(ClientError::CompletionMismatch)
        }
    }
}

const fn ctl_code(function: u32, method: u32) -> u32 {
    (DEVICE_TYPE << 16) | (IOCTL_ACCESS << 14) | (function << 2) | method
}

const fn ioctl_for(message_type: MessageType) -> u32 {
    match message_type {
        MessageType::Hello => ctl_code(0x800, 0),
        MessageType::Attach => ctl_code(0x801, 0),
        MessageType::SetLink => ctl_code(0x802, 0),
        MessageType::TransmitBatch => ctl_code(0x803, 2),
        MessageType::ReceiveBatch => ctl_code(0x804, 1),
        MessageType::Detach => ctl_code(0x805, 0),
    }
}

fn encode_message(
    message_type: MessageType,
    sequence: u64,
    payload: &[u8],
) -> Result<Vec<u8>, ClientError> {
    let payload_length = u32::try_from(payload.len()).map_err(|_| ClientError::InvalidPacket)?;
    if payload.len() > MAX_PAYLOAD {
        return Err(ClientError::InvalidPacket);
    }
    let mut message = vec![0_u8; HEADER_SIZE + payload.len()];
    write_u32(&mut message, 0, ABI_MAGIC);
    write_u16(&mut message, 4, ABI_VERSION);
    write_u16(&mut message, 6, HEADER_SIZE_U16);
    write_u32(&mut message, 8, message_type as u32);
    write_u32(&mut message, 16, payload_length);
    write_u64(&mut message, 24, sequence);
    message[HEADER_SIZE..].copy_from_slice(payload);
    Ok(message)
}

fn encode_packet_batch(packets: &[&[u8]], mtu: u32) -> Result<Vec<u8>, ClientError> {
    if packets.is_empty() || packets.len() > MAX_PACKETS {
        return Err(ClientError::InvalidPacket);
    }
    let descriptor_bytes = packets.len() * 8;
    let packet_bytes = packets.iter().try_fold(0_usize, |total, packet| {
        validate_ipv4(packet, mtu)?;
        total
            .checked_add(packet.len())
            .ok_or(ClientError::InvalidPacket)
    })?;
    let total = 8_usize
        .checked_add(descriptor_bytes)
        .and_then(|value| value.checked_add(packet_bytes))
        .ok_or(ClientError::InvalidPacket)?;
    if total > MAX_PAYLOAD {
        return Err(ClientError::InvalidPacket);
    }
    let mut payload = vec![0_u8; total];
    let packet_count = u16::try_from(packets.len()).map_err(|_| ClientError::InvalidPacket)?;
    let descriptor_length =
        u32::try_from(descriptor_bytes).map_err(|_| ClientError::InvalidPacket)?;
    write_u16(&mut payload, 0, packet_count);
    write_u32(&mut payload, 4, descriptor_length);
    let mut packet_offset = 8 + descriptor_bytes;
    for (packet_index, packet) in packets.iter().enumerate() {
        let descriptor_offset = 8 + packet_index * 8;
        write_u32(
            &mut payload,
            descriptor_offset,
            u32::try_from(packet_offset).map_err(|_| ClientError::InvalidPacket)?,
        );
        write_u32(
            &mut payload,
            descriptor_offset + 4,
            u32::try_from(packet.len()).map_err(|_| ClientError::InvalidPacket)?,
        );
        payload[packet_offset..packet_offset + packet.len()].copy_from_slice(packet);
        packet_offset += packet.len();
    }
    Ok(payload)
}

fn decode_transmit_response(
    response: &[u8],
    sequence: u64,
    mtu: u32,
    transmit_depth: u16,
) -> Result<Vec<Vec<u8>>, ClientError> {
    if response.len() < HEADER_SIZE
        || read_u32(response, 0) != Some(ABI_MAGIC)
        || read_u16(response, 4) != Some(ABI_VERSION)
        || read_u16(response, 6) != Some(HEADER_SIZE_U16)
        || read_u32(response, 8) != Some(MessageType::TransmitBatch as u32)
        || read_u32(response, 12) != Some(0)
        || read_u32(response, 20) != Some(0)
        || read_u64(response, 24) != Some(sequence)
    {
        return Err(ClientError::InvalidResponse);
    }
    let payload_length =
        usize::try_from(read_u32(response, 16).ok_or(ClientError::InvalidResponse)?)
            .map_err(|_| ClientError::InvalidResponse)?;
    if payload_length > MAX_PAYLOAD || response.len() != HEADER_SIZE + payload_length {
        return Err(ClientError::InvalidResponse);
    }
    decode_packet_batch(&response[HEADER_SIZE..], mtu, transmit_depth)
}

fn decode_packet_batch(
    payload: &[u8],
    mtu: u32,
    maximum_packets: u16,
) -> Result<Vec<Vec<u8>>, ClientError> {
    let packet_count = usize::from(read_u16(payload, 0).ok_or(ClientError::InvalidResponse)?);
    let descriptor_bytes =
        usize::try_from(read_u32(payload, 4).ok_or(ClientError::InvalidResponse)?)
            .map_err(|_| ClientError::InvalidResponse)?;
    if packet_count == 0
        || packet_count > usize::from(maximum_packets)
        || packet_count > MAX_PACKETS
        || read_u16(payload, 2) != Some(0)
        || descriptor_bytes != packet_count * 8
        || 8 + descriptor_bytes > payload.len()
    {
        return Err(ClientError::InvalidResponse);
    }
    let mut expected_offset = 8 + descriptor_bytes;
    let mut packets = Vec::with_capacity(packet_count);
    for packet_index in 0..packet_count {
        let descriptor_offset = 8 + packet_index * 8;
        let packet_offset = usize::try_from(
            read_u32(payload, descriptor_offset).ok_or(ClientError::InvalidResponse)?,
        )
        .map_err(|_| ClientError::InvalidResponse)?;
        let packet_length = usize::try_from(
            read_u32(payload, descriptor_offset + 4).ok_or(ClientError::InvalidResponse)?,
        )
        .map_err(|_| ClientError::InvalidResponse)?;
        let packet_end = packet_offset
            .checked_add(packet_length)
            .ok_or(ClientError::InvalidResponse)?;
        if packet_offset != expected_offset || packet_end > payload.len() {
            return Err(ClientError::InvalidResponse);
        }
        validate_ipv4(&payload[packet_offset..packet_end], mtu)
            .map_err(|_| ClientError::InvalidResponse)?;
        packets.push(payload[packet_offset..packet_end].to_vec());
        expected_offset = packet_end;
    }
    if expected_offset != payload.len() {
        return Err(ClientError::InvalidResponse);
    }
    Ok(packets)
}

fn validate_ipv4(packet: &[u8], mtu: u32) -> Result<(), ClientError> {
    if packet.len() < MIN_PACKET_SIZE
        || packet.len() > mtu as usize
        || packet.len() > MAX_PACKET_SIZE
        || packet[0] >> 4 != 4
    {
        return Err(ClientError::InvalidPacket);
    }
    let header_length = usize::from(packet[0] & 0x0f) * 4;
    let total_length = usize::from(u16::from_be_bytes([packet[2], packet[3]]));
    if header_length < MIN_PACKET_SIZE
        || header_length > packet.len()
        || total_length != packet.len()
    {
        return Err(ClientError::InvalidPacket);
    }
    Ok(())
}

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn read_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        bytes.get(offset..offset + 8)?.try_into().ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet(length: usize) -> Vec<u8> {
        let mut packet = vec![0_u8; length];
        packet[0] = 0x45;
        packet[2..4].copy_from_slice(
            &u16::try_from(length)
                .expect("test packet length fits u16")
                .to_be_bytes(),
        );
        packet
    }

    struct FixedTransport(TransportOutcome);

    impl XsnetTransport for FixedTransport {
        fn execute(&mut self, _request: &PreparedRequest) -> TransportOutcome {
            self.0.clone()
        }
    }

    fn complete_handshake(client: &mut XsnetClient) {
        let hello = client.prepare_hello().expect("hello");
        client.complete_success(&hello, &[]).expect("hello success");
        let attach = client.prepare_attach(1400, 8, 8).expect("attach");
        client
            .complete_success(&attach, &[])
            .expect("attach success");
        let link = client.prepare_set_link(true).expect("link");
        client.complete_success(&link, &[]).expect("link success");
    }

    #[test]
    fn successful_flow_commits_sequence_and_packets() {
        let mut client = XsnetClient::opened();
        complete_handshake(&mut client);
        let outbound = packet(32);
        let request = client.prepare_transmit(4096).expect("transmit");
        let payload = encode_packet_batch(&[outbound.as_slice()], 1400).expect("batch");
        let response = encode_message(MessageType::TransmitBatch, request.sequence, &payload)
            .expect("response");
        let packets = client
            .complete_success(&request, &response)
            .expect("transmit success");
        assert_eq!(packets, vec![outbound]);
        assert_eq!(client.state(), ClientState::LinkUp);
        assert_eq!(client.next_sequence(), 5);
    }

    #[test]
    fn known_rejection_reuses_sequence_without_state_commit() {
        let mut client = XsnetClient::opened();
        let first = client.prepare_hello().expect("first hello");
        client.complete_rejected(&first).expect("known rejection");
        let retry = client.prepare_hello().expect("retry hello");
        assert_eq!(retry.sequence, first.sequence);
        assert_eq!(retry.buffered_input, first.buffered_input);
        assert_eq!(client.state(), ClientState::Opened);
        assert_eq!(client.mtu, 0);
        assert_eq!(client.transmit_depth, 0);
        assert_eq!(client.receive_depth, 0);
    }

    #[test]
    fn indeterminate_result_requires_fresh_handle() {
        let mut client = XsnetClient::opened();
        let hello = client.prepare_hello().expect("hello");
        client
            .complete_indeterminate(&hello)
            .expect("indeterminate");
        assert_eq!(client.state(), ClientState::ReconnectRequired);
        assert_eq!(client.prepare_hello(), Err(ClientError::BadState));
        assert_eq!(XsnetClient::opened().next_sequence(), 1);
    }

    #[test]
    fn invalid_packets_and_responses_fail_closed() {
        let mut client = XsnetClient::opened();
        complete_handshake(&mut client);
        let malformed = vec![0_u8; 20];
        assert_eq!(
            client.prepare_receive(&[malformed.as_slice()]),
            Err(ClientError::InvalidPacket)
        );
        let request = client.prepare_transmit(4096).expect("transmit");
        assert_eq!(
            client.complete_success(&request, &[0_u8; HEADER_SIZE]),
            Err(ClientError::InvalidResponse)
        );
        assert_eq!(client.state(), ClientState::LinkUp);
        assert_eq!(client.next_sequence(), request.sequence);
        client
            .complete_indeterminate(&request)
            .expect("abandon malformed response");
        assert_eq!(client.state(), ClientState::ReconnectRequired);
    }

    #[test]
    fn request_is_single_flight_and_completion_is_bound() {
        let mut client = XsnetClient::opened();
        let hello = client.prepare_hello().expect("hello");
        assert_eq!(client.prepare_hello(), Err(ClientError::RequestActive));
        let mut wrong = hello.clone();
        wrong.sequence += 1;
        assert_eq!(
            client.complete_rejected(&wrong),
            Err(ClientError::CompletionMismatch)
        );
        client
            .complete_rejected(&hello)
            .expect("matching rejection");
    }

    #[test]
    fn constants_and_header_match_driver_contract() {
        assert_eq!(ioctl_for(MessageType::Hello), 0x8337_e000);
        assert_eq!(ioctl_for(MessageType::Attach), 0x8337_e004);
        assert_eq!(ioctl_for(MessageType::SetLink), 0x8337_e008);
        assert_eq!(ioctl_for(MessageType::TransmitBatch), 0x8337_e00e);
        assert_eq!(ioctl_for(MessageType::ReceiveBatch), 0x8337_e011);
        assert_eq!(ioctl_for(MessageType::Detach), 0x8337_e014);

        let mut client = XsnetClient::opened();
        let request = client.prepare_hello().expect("hello");
        assert_eq!(&request.buffered_input[0..4], &ABI_MAGIC.to_le_bytes());
        assert_eq!(&request.buffered_input[4..6], &ABI_VERSION.to_le_bytes());
        assert_eq!(
            &request.buffered_input[6..8],
            &HEADER_SIZE_U16.to_le_bytes()
        );
        assert_eq!(read_u32(&request.buffered_input, 8), Some(1));
        assert_eq!(read_u32(&request.buffered_input, 16), Some(16));
        assert_eq!(read_u64(&request.buffered_input, 24), Some(1));
    }

    #[test]
    fn transport_classification_controls_recovery() {
        let mut rejected_client = XsnetClient::opened();
        let rejected_request = rejected_client.prepare_hello().expect("hello");
        let mut rejected_transport = FixedTransport(TransportOutcome::Rejected(0xc000_0184));
        assert_eq!(
            rejected_client.execute_prepared(&mut rejected_transport, &rejected_request),
            Err(ExecutionError::Rejected(0xc000_0184))
        );
        assert_eq!(rejected_client.state(), ClientState::Opened);
        assert_eq!(rejected_client.next_sequence(), 1);

        let mut unknown_client = XsnetClient::opened();
        let unknown_request = unknown_client.prepare_hello().expect("hello");
        let mut unknown_transport = FixedTransport(TransportOutcome::Indeterminate);
        assert_eq!(
            unknown_client.execute_prepared(&mut unknown_transport, &unknown_request),
            Err(ExecutionError::ReconnectRequired)
        );
        assert_eq!(unknown_client.state(), ClientState::ReconnectRequired);

        let mut malformed_client = XsnetClient::opened();
        complete_handshake(&mut malformed_client);
        let malformed_request = malformed_client.prepare_transmit(4096).expect("transmit");
        let mut malformed_transport =
            FixedTransport(TransportOutcome::Success(vec![0_u8; HEADER_SIZE]));
        assert_eq!(
            malformed_client.execute_prepared(&mut malformed_transport, &malformed_request),
            Err(ExecutionError::Client(ClientError::InvalidResponse))
        );
        assert_eq!(malformed_client.state(), ClientState::ReconnectRequired);
    }
}
