//! JD Server error types

use thiserror::Error;

#[derive(Error, Debug)]
pub enum JdServerError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid token")]
    InvalidToken,

    #[error("Token expired")]
    TokenExpired,

    #[error("Invalid coinbase: {0}")]
    InvalidCoinbase(String),

    #[error("Invalid merkle root")]
    InvalidMerkleRoot,

    #[error("Stale prev_hash")]
    StalePrevHash,

    #[error("Channel send error")]
    ChannelSend,

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("Pool error: {0}")]
    Pool(#[from] zcash_pool_server::PoolError),
}

pub type Result<T> = std::result::Result<T, JdServerError>;
