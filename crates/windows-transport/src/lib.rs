#![no_std]
#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

extern crate alloc;

use alloc::vec::Vec;
use core::error::Error;
use core::fmt::{Display, Formatter};

pub const IOCTL_HELLO: u32 = 0x8337_e000;
pub const IOCTL_ATTACH: u32 = 0x8337_e004;
pub const IOCTL_SET_LINK: u32 = 0x8337_e008;
pub const IOCTL_DEQUEUE_TX: u32 = 0x8337_e00e;
pub const IOCTL_ENQUEUE_RX: u32 = 0x8337_e011;
pub const IOCTL_DETACH: u32 = 0x8337_e014;
pub const IOCTL_QUERY_IDENTITY: u32 = 0x8337_e018;
const HEADER_SIZE: usize = 32;
const MAX_PAYLOAD: usize = 1_048_576;
const MIN_RX_MESSAGE: usize = HEADER_SIZE + 16 + 20;
#[cfg(any(windows, test))]
const IDENTITY_VERSION: u16 = 1;
#[cfg(any(windows, test))]
const IDENTITY_RESPONSE_SIZE: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestError {
    UnsupportedIoctl,
    InvalidBuffers,
}

impl Display for RequestError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnsupportedIoctl => formatter.write_str("unsupported xsnet IOCTL"),
            Self::InvalidBuffers => formatter.write_str("invalid xsnet transport buffers"),
        }
    }
}

impl Error for RequestError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RequestKind {
    Buffered,
    Dequeue,
    Enqueue,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Request<'a> {
    ioctl: u32,
    buffered_input: &'a [u8],
    direct_input: &'a [u8],
    direct_output_capacity: usize,
    kind: RequestKind,
}

impl<'a> Request<'a> {
    /// Validates the exact xsnet buffer mapping before entering Win32.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown IOCTL, oversized length, or a buffer in the wrong slot.
    pub fn new(
        ioctl: u32,
        buffered_input: &'a [u8],
        direct_input: &'a [u8],
        direct_output_capacity: usize,
    ) -> Result<Self, RequestError> {
        let maximum_message = HEADER_SIZE + MAX_PAYLOAD;
        let kind = match ioctl {
            IOCTL_HELLO | IOCTL_ATTACH | IOCTL_SET_LINK | IOCTL_DETACH
                if (HEADER_SIZE..=maximum_message).contains(&buffered_input.len())
                    && direct_input.is_empty()
                    && direct_output_capacity == 0 =>
            {
                RequestKind::Buffered
            }
            IOCTL_DEQUEUE_TX
                if buffered_input.len() == HEADER_SIZE
                    && direct_input.is_empty()
                    && (HEADER_SIZE..=maximum_message).contains(&direct_output_capacity) =>
            {
                RequestKind::Dequeue
            }
            IOCTL_ENQUEUE_RX
                if buffered_input.is_empty()
                    && (MIN_RX_MESSAGE..=maximum_message).contains(&direct_input.len())
                    && direct_output_capacity == 0 =>
            {
                RequestKind::Enqueue
            }
            IOCTL_HELLO | IOCTL_ATTACH | IOCTL_SET_LINK | IOCTL_DEQUEUE_TX | IOCTL_ENQUEUE_RX
            | IOCTL_DETACH => return Err(RequestError::InvalidBuffers),
            _ => return Err(RequestError::UnsupportedIoctl),
        };
        u32::try_from(buffered_input.len()).map_err(|_| RequestError::InvalidBuffers)?;
        u32::try_from(direct_input.len()).map_err(|_| RequestError::InvalidBuffers)?;
        u32::try_from(direct_output_capacity).map_err(|_| RequestError::InvalidBuffers)?;
        Ok(Self {
            ioctl,
            buffered_input,
            direct_input,
            direct_output_capacity,
            kind,
        })
    }

    #[cfg(windows)]
    pub(crate) const fn ioctl(self) -> u32 {
        self.ioctl
    }

    #[cfg(windows)]
    pub(crate) const fn buffered_input(self) -> &'a [u8] {
        self.buffered_input
    }

    #[cfg(windows)]
    pub(crate) const fn direct_input(self) -> &'a [u8] {
        self.direct_input
    }

    #[cfg(windows)]
    pub(crate) const fn direct_output_capacity(self) -> usize {
        self.direct_output_capacity
    }

    #[cfg(windows)]
    pub(crate) const fn kind(self) -> RequestKind {
        self.kind
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Outcome {
    Success(Vec<u8>),
    Indeterminate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpenError {
    UnsupportedPlatform,
    EnumerationFailed,
    InvalidInterfaceList,
    AmbiguousInterface,
    OpenFailed,
    IdentityQueryFailed,
    InvalidIdentity,
}

impl Display for OpenError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnsupportedPlatform => formatter.write_str("xsnet is only available on Windows"),
            Self::EnumerationFailed => formatter.write_str("xsnet interface enumeration failed"),
            Self::InvalidInterfaceList => formatter.write_str("xsnet interface list is invalid"),
            Self::AmbiguousInterface => formatter.write_str("xsnet interface list is ambiguous"),
            Self::OpenFailed => formatter.write_str("xsnet interface open failed"),
            Self::IdentityQueryFailed => formatter.write_str("xsnet identity query failed"),
            Self::InvalidIdentity => formatter.write_str("xsnet identity response is invalid"),
        }
    }
}

#[cfg(any(windows, test))]
fn parse_identity_response(buffer: &[u8]) -> Result<u64, OpenError> {
    if buffer.len() != IDENTITY_RESPONSE_SIZE
        || u16::from_le_bytes([buffer[0], buffer[1]]) != IDENTITY_VERSION
        || usize::from(u16::from_le_bytes([buffer[2], buffer[3]])) != IDENTITY_RESPONSE_SIZE
        || buffer[4..8] != [0, 0, 0, 0]
    {
        return Err(OpenError::InvalidIdentity);
    }
    let luid = u64::from_le_bytes(
        buffer[8..16]
            .try_into()
            .map_err(|_| OpenError::InvalidIdentity)?,
    );
    if luid == 0 {
        return Err(OpenError::InvalidIdentity);
    }
    Ok(luid)
}

impl Error for OpenError {}

#[cfg(any(windows, test))]
fn parse_single_interface(buffer: &[u16]) -> Result<Vec<u16>, OpenError> {
    if buffer.len() < 2 || buffer[buffer.len() - 2..] != [0, 0] {
        return Err(OpenError::InvalidInterfaceList);
    }
    let terminator = buffer
        .iter()
        .position(|value| *value == 0)
        .ok_or(OpenError::InvalidInterfaceList)?;
    if terminator == 0 {
        return Err(OpenError::InvalidInterfaceList);
    }
    if terminator != buffer.len() - 2 {
        return Err(OpenError::AmbiguousInterface);
    }
    let prefix = [
        u16::from(b'\\'),
        u16::from(b'\\'),
        u16::from(b'?'),
        u16::from(b'\\'),
    ];
    if terminator <= prefix.len()
        || !buffer.starts_with(&prefix)
        || core::char::decode_utf16(buffer[..terminator].iter().copied())
            .any(|value| value.is_err())
    {
        return Err(OpenError::InvalidInterfaceList);
    }
    Ok(buffer[..=terminator].to_vec())
}

#[cfg(windows)]
#[allow(unsafe_code)]
mod platform;

#[cfg(windows)]
pub use platform::DeviceTransport;

#[cfg(not(windows))]
#[derive(Debug)]
pub struct DeviceTransport;

#[cfg(not(windows))]
impl DeviceTransport {
    /// Rejects opening xsnet on non-Windows platforms.
    ///
    /// # Errors
    ///
    /// Always returns `UnsupportedPlatform`.
    pub const fn open() -> Result<Self, OpenError> {
        Err(OpenError::UnsupportedPlatform)
    }

    #[must_use]
    pub const fn interface_luid(&self) -> u64 {
        0
    }

    pub fn execute(&mut self, _request: Request<'_>) -> Outcome {
        Outcome::Indeterminate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_mapping_is_exact_and_bounded() {
        let control = [0_u8; 48];
        assert!(Request::new(IOCTL_HELLO, &control, &[], 0).is_ok());
        assert!(Request::new(IOCTL_DEQUEUE_TX, &[0_u8; HEADER_SIZE], &[], 4096).is_ok());
        assert!(Request::new(IOCTL_ENQUEUE_RX, &[], &[0_u8; MIN_RX_MESSAGE], 0).is_ok());
        assert_eq!(
            Request::new(IOCTL_ENQUEUE_RX, &[0_u8; 1], &[0_u8; MIN_RX_MESSAGE], 0),
            Err(RequestError::InvalidBuffers)
        );
        assert_eq!(
            Request::new(0xdead_beef, &control, &[], 0),
            Err(RequestError::UnsupportedIoctl)
        );
    }

    #[test]
    fn interface_list_requires_one_canonical_path() {
        let path: Vec<u16> = r"\\?\xsnet#one".encode_utf16().chain([0, 0]).collect();
        let parsed = parse_single_interface(&path).expect("single path");
        assert_eq!(parsed.last(), Some(&0));

        let multiple: Vec<u16> = r"\\?\xsnet#one"
            .encode_utf16()
            .chain([0])
            .chain(r"\\?\xsnet#two".encode_utf16())
            .chain([0, 0])
            .collect();
        assert_eq!(
            parse_single_interface(&multiple),
            Err(OpenError::AmbiguousInterface)
        );
        assert_eq!(
            parse_single_interface(&[0, 0]),
            Err(OpenError::InvalidInterfaceList)
        );
        let prefix_only: Vec<u16> = r"\\?\".encode_utf16().chain([0, 0]).collect();
        assert_eq!(
            parse_single_interface(&prefix_only),
            Err(OpenError::InvalidInterfaceList)
        );
        let invalid_utf16 = [
            u16::from(b'\\'),
            u16::from(b'\\'),
            u16::from(b'?'),
            u16::from(b'\\'),
            0xd800,
            0,
            0,
        ];
        assert_eq!(
            parse_single_interface(&invalid_utf16),
            Err(OpenError::InvalidInterfaceList)
        );
    }

    #[test]
    fn identity_response_requires_exact_schema_and_nonzero_luid() {
        let mut response = [0_u8; IDENTITY_RESPONSE_SIZE];
        response[0..2].copy_from_slice(&IDENTITY_VERSION.to_le_bytes());
        response[2..4].copy_from_slice(&16_u16.to_le_bytes());
        response[8..16].copy_from_slice(&42_u64.to_le_bytes());
        assert_eq!(parse_identity_response(&response), Ok(42));

        let mut invalid = response;
        invalid[4] = 1;
        assert_eq!(
            parse_identity_response(&invalid),
            Err(OpenError::InvalidIdentity)
        );
        assert_eq!(
            parse_identity_response(&response[..IDENTITY_RESPONSE_SIZE - 1]),
            Err(OpenError::InvalidIdentity)
        );
        response[8..16].fill(0);
        assert_eq!(
            parse_identity_response(&response),
            Err(OpenError::InvalidIdentity)
        );
    }
}
