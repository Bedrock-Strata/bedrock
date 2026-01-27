//! Validation error types

use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum ValidationError {
    #[error("Invalid Equihash solution: {0}")]
    InvalidSolution(String),

    #[error("Solution does not meet target difficulty")]
    TargetNotMet,

    #[error("Invalid header length: expected 140, got {0}")]
    InvalidHeaderLength(usize),

    #[error("Invalid solution length: expected 1344, got {0}")]
    InvalidSolutionLength(usize),

    #[error("Invalid nonce length: expected 32, got {0}")]
    InvalidNonceLength(usize),

    #[error("Hash computation failed: {0}")]
    HashError(String),
}

pub type Result<T> = std::result::Result<T, ValidationError>;
