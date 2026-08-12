use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    CONTROL_MESSAGE_LIMIT_BYTES, CONTROL_TRANSFER_CHUNK_BYTES, CONTROL_TRANSFER_THRESHOLD_BYTES,
    ControlServerMessage,
};

#[derive(Debug, Error, Eq, PartialEq)]
/// Reason a bounded control-message transfer was rejected.
pub enum ControlTransferError {
    #[error("invalid control transfer envelope")]
    InvalidEnvelope,
    #[error("control transfer exceeds the configured limit")]
    LimitExceeded,
    #[error("control transfer digest mismatch")]
    DigestMismatch,
    #[error("control transfer payload is invalid")]
    InvalidPayload,
}

#[derive(Debug, Default)]
/// Strictly reassembles one bounded, ordered server-message transfer at a time.
pub struct ControlTransferAssembler {
    active: Option<ActiveTransfer>,
}

#[derive(Debug)]
struct ActiveTransfer {
    transfer_id_base64: String,
    total_bytes: usize,
    chunk_count: usize,
    next_index: usize,
    sha256: [u8; 32],
    bytes: Vec<u8>,
}

impl ControlTransferAssembler {
    /// Returns whether a transfer has started but has not completed.
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.active.is_some()
    }

    /// Accepts one decoded control message and returns a complete logical message when available.
    ///
    /// # Errors
    ///
    /// Returns an error for non-canonical metadata, invalid ordering, oversized data, digest
    /// mismatch, malformed payloads, or nested transfer envelopes. Any error clears partial state.
    pub fn accept(
        &mut self,
        message: ControlServerMessage,
    ) -> Result<Option<ControlServerMessage>, ControlTransferError> {
        let result = self.accept_inner(message);
        if result.is_err() {
            self.active = None;
        }
        result
    }

    fn accept_inner(
        &mut self,
        message: ControlServerMessage,
    ) -> Result<Option<ControlServerMessage>, ControlTransferError> {
        match message {
            ControlServerMessage::TransferStart {
                transfer_id_base64,
                total_bytes,
                chunk_count,
                sha256_base64,
            } if self.active.is_none() => {
                validate_fixed::<16>(&transfer_id_base64)?;
                let sha256 = validate_fixed::<32>(&sha256_base64)?;
                let total_bytes = usize::try_from(total_bytes)
                    .map_err(|_| ControlTransferError::LimitExceeded)?;
                let chunk_count = usize::from(chunk_count);
                if !(CONTROL_TRANSFER_THRESHOLD_BYTES + 1..=CONTROL_MESSAGE_LIMIT_BYTES)
                    .contains(&total_bytes)
                    || chunk_count == 0
                    || chunk_count != total_bytes.div_ceil(CONTROL_TRANSFER_CHUNK_BYTES)
                {
                    return Err(ControlTransferError::InvalidEnvelope);
                }
                self.active = Some(ActiveTransfer {
                    transfer_id_base64,
                    total_bytes,
                    chunk_count,
                    next_index: 0,
                    sha256,
                    bytes: Vec::with_capacity(total_bytes),
                });
                Ok(None)
            }
            ControlServerMessage::TransferChunk {
                transfer_id_base64,
                index,
                data_base64,
            } => {
                let active = self
                    .active
                    .as_mut()
                    .ok_or(ControlTransferError::InvalidEnvelope)?;
                let index = usize::from(index);
                if transfer_id_base64 != active.transfer_id_base64
                    || index != active.next_index
                    || index >= active.chunk_count
                {
                    return Err(ControlTransferError::InvalidEnvelope);
                }
                let chunk = URL_SAFE_NO_PAD
                    .decode(&data_base64)
                    .map_err(|_| ControlTransferError::InvalidEnvelope)?;
                if URL_SAFE_NO_PAD.encode(&chunk) != data_base64 {
                    return Err(ControlTransferError::InvalidEnvelope);
                }
                let remaining = active.total_bytes.saturating_sub(active.bytes.len());
                let expected = remaining.min(CONTROL_TRANSFER_CHUNK_BYTES);
                if chunk.len() != expected {
                    return Err(ControlTransferError::InvalidEnvelope);
                }
                active.bytes.extend_from_slice(&chunk);
                active.next_index += 1;
                Ok(None)
            }
            ControlServerMessage::TransferEnd { transfer_id_base64 } => {
                let active = self
                    .active
                    .take()
                    .ok_or(ControlTransferError::InvalidEnvelope)?;
                if transfer_id_base64 != active.transfer_id_base64
                    || active.next_index != active.chunk_count
                    || active.bytes.len() != active.total_bytes
                {
                    return Err(ControlTransferError::InvalidEnvelope);
                }
                let digest: [u8; 32] = Sha256::digest(&active.bytes).into();
                if digest != active.sha256 {
                    return Err(ControlTransferError::DigestMismatch);
                }
                let message = serde_json::from_slice::<ControlServerMessage>(&active.bytes)
                    .map_err(|_| ControlTransferError::InvalidPayload)?;
                if is_transfer_envelope(&message) {
                    return Err(ControlTransferError::InvalidPayload);
                }
                Ok(Some(message))
            }
            ControlServerMessage::TransferStart { .. } => {
                Err(ControlTransferError::InvalidEnvelope)
            }
            message if self.active.is_none() => Ok(Some(message)),
            _ => Err(ControlTransferError::InvalidEnvelope),
        }
    }
}

fn validate_fixed<const LENGTH: usize>(value: &str) -> Result<[u8; LENGTH], ControlTransferError> {
    let decoded: [u8; LENGTH] = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| ControlTransferError::InvalidEnvelope)?
        .try_into()
        .map_err(|_| ControlTransferError::InvalidEnvelope)?;
    if URL_SAFE_NO_PAD.encode(decoded) == value {
        Ok(decoded)
    } else {
        Err(ControlTransferError::InvalidEnvelope)
    }
}

const fn is_transfer_envelope(message: &ControlServerMessage) -> bool {
    matches!(
        message,
        ControlServerMessage::TransferStart { .. }
            | ControlServerMessage::TransferChunk { .. }
            | ControlServerMessage::TransferEnd { .. }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reassembles_large_message() {
        let original = ControlServerMessage::Error {
            code: "x".repeat(CONTROL_TRANSFER_THRESHOLD_BYTES + 1),
        };
        let encoded = serde_json::to_vec(&original).expect("serialize fixture");
        let transfer_id_base64 = URL_SAFE_NO_PAD.encode([7_u8; 16]);
        let digest = Sha256::digest(&encoded);
        let chunk_count = encoded.len().div_ceil(CONTROL_TRANSFER_CHUNK_BYTES);
        let mut assembler = ControlTransferAssembler::default();
        assert!(
            assembler
                .accept(ControlServerMessage::TransferStart {
                    transfer_id_base64: transfer_id_base64.clone(),
                    total_bytes: u32::try_from(encoded.len()).expect("fixture length"),
                    chunk_count: u16::try_from(chunk_count).expect("fixture chunks"),
                    sha256_base64: URL_SAFE_NO_PAD.encode(digest),
                })
                .expect("valid start")
                .is_none()
        );
        for (index, chunk) in encoded.chunks(CONTROL_TRANSFER_CHUNK_BYTES).enumerate() {
            assert!(
                assembler
                    .accept(ControlServerMessage::TransferChunk {
                        transfer_id_base64: transfer_id_base64.clone(),
                        index: u16::try_from(index).expect("fixture index"),
                        data_base64: URL_SAFE_NO_PAD.encode(chunk),
                    })
                    .expect("valid chunk")
                    .is_none()
            );
        }
        let decoded = assembler
            .accept(ControlServerMessage::TransferEnd { transfer_id_base64 })
            .expect("valid end")
            .expect("complete message");
        assert!(
            matches!(decoded, ControlServerMessage::Error { code } if code.len() == CONTROL_TRANSFER_THRESHOLD_BYTES + 1)
        );
    }

    #[test]
    fn rejects_out_of_order_chunk_and_clears_state() {
        let mut assembler = ControlTransferAssembler::default();
        let total_bytes = CONTROL_TRANSFER_THRESHOLD_BYTES + 1;
        let transfer_id_base64 = URL_SAFE_NO_PAD.encode([9_u8; 16]);
        assembler
            .accept(ControlServerMessage::TransferStart {
                transfer_id_base64: transfer_id_base64.clone(),
                total_bytes: u32::try_from(total_bytes).expect("fixture length"),
                chunk_count: u16::try_from(total_bytes.div_ceil(CONTROL_TRANSFER_CHUNK_BYTES))
                    .expect("fixture chunks"),
                sha256_base64: URL_SAFE_NO_PAD.encode([0_u8; 32]),
            })
            .expect("valid start");
        assert!(matches!(
            assembler.accept(ControlServerMessage::TransferChunk {
                transfer_id_base64,
                index: 1,
                data_base64: URL_SAFE_NO_PAD.encode([0_u8; CONTROL_TRANSFER_CHUNK_BYTES]),
            }),
            Err(ControlTransferError::InvalidEnvelope)
        ));
        assert!(!assembler.is_active());
    }

    #[test]
    fn rejects_digest_mismatch() {
        let encoded = vec![b'x'; CONTROL_TRANSFER_THRESHOLD_BYTES + 1];
        let transfer_id_base64 = URL_SAFE_NO_PAD.encode([11_u8; 16]);
        let mut assembler = ControlTransferAssembler::default();
        assembler
            .accept(ControlServerMessage::TransferStart {
                transfer_id_base64: transfer_id_base64.clone(),
                total_bytes: u32::try_from(encoded.len()).expect("fixture length"),
                chunk_count: u16::try_from(encoded.len().div_ceil(CONTROL_TRANSFER_CHUNK_BYTES))
                    .expect("fixture chunks"),
                sha256_base64: URL_SAFE_NO_PAD.encode([0_u8; 32]),
            })
            .expect("valid start");
        for (index, chunk) in encoded.chunks(CONTROL_TRANSFER_CHUNK_BYTES).enumerate() {
            assembler
                .accept(ControlServerMessage::TransferChunk {
                    transfer_id_base64: transfer_id_base64.clone(),
                    index: u16::try_from(index).expect("fixture index"),
                    data_base64: URL_SAFE_NO_PAD.encode(chunk),
                })
                .expect("valid chunk");
        }
        assert!(matches!(
            assembler.accept(ControlServerMessage::TransferEnd { transfer_id_base64 }),
            Err(ControlTransferError::DigestMismatch)
        ));
        assert!(!assembler.is_active());
    }

    #[test]
    fn maximum_chunk_envelope_stays_below_single_frame_threshold() {
        let message = ControlServerMessage::TransferChunk {
            transfer_id_base64: URL_SAFE_NO_PAD.encode([u8::MAX; 16]),
            index: u16::MAX,
            data_base64: URL_SAFE_NO_PAD.encode([u8::MAX; CONTROL_TRANSFER_CHUNK_BYTES]),
        };
        let encoded = serde_json::to_vec(&message).expect("serialize maximum chunk envelope");
        assert!(encoded.len() <= CONTROL_TRANSFER_THRESHOLD_BYTES);
    }
}
