//! JD Client error types

use thiserror::Error;

#[derive(Error, Debug)]
pub enum JdClientError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Template provider error: {0}")]
    TemplateProvider(#[from] zcash_template_provider::Error),

    #[error("JD Server error: {0}")]
    JdServer(#[from] zcash_jd_server::JdServerError),

    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Token allocation failed: {0}")]
    TokenAllocationFailed(String),

    #[error("Job declaration rejected: {0}")]
    JobRejected(String),

    #[error("Block submission failed: {0}")]
    BlockSubmissionFailed(String),

    #[error("Protocol error: {0}")]
    Protocol(String),
}

pub type Result<T> = std::result::Result<T, JdClientError>;
