//! Protocol error types

use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    #[error("Invalid message type: {0}")]
    InvalidMessageType(u8),

    #[error("Message too short: expected {expected}, got {actual}")]
    MessageTooShort { expected: usize, actual: usize },

    #[error("Invalid nonce length: expected {expected}, got {actual}")]
    InvalidNonceLength { expected: usize, actual: usize },

    #[error("Invalid solution length: expected 1344, got {0}")]
    InvalidSolutionLength(usize),

    #[error("Unknown channel: {0}")]
    UnknownChannel(u32),

    #[error("Unknown job: {0}")]
    UnknownJob(u32),

    #[error("Stale share: job {job_id} superseded")]
    StaleShare { job_id: u32 },

    #[error("Duplicate share")]
    DuplicateShare,

    #[error("Invalid solution: {0}")]
    InvalidSolution(String),

    #[error("Target not met: share difficulty below threshold")]
    TargetNotMet,

    #[error("Encoding error: {0}")]
    EncodingError(String),
}

pub type Result<T> = std::result::Result<T, ProtocolError>;
