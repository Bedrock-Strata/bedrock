//! Error types for the template provider

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("RPC error: {0}")]
    Rpc(String),

    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Invalid hex: {0}")]
    Hex(#[from] hex::FromHexError),

    #[error("Invalid template: {0}")]
    InvalidTemplate(String),

    #[error("Connection failed: {0}")]
    Connection(String),
}

pub type Result<T> = std::result::Result<T, Error>;
