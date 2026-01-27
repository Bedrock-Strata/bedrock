//! Job Declaration Server for Zcash Stratum V2
//!
//! This crate implements the Job Declaration (JD) protocol for Zcash,
//! enabling miners to declare custom mining jobs in Coinbase-Only mode.
//!
//! ## Overview
//!
//! The JD protocol allows miners to:
//! - Request mining job tokens
//! - Declare custom jobs with their own coinbase transactions
//! - Submit block solutions directly
//!
//! ## Protocol Flow
//!
//! 1. Client requests a token via `AllocateMiningJobToken`
//! 2. Server responds with `AllocateMiningJobTokenSuccess` containing the token
//! 3. Client declares a job via `SetCustomMiningJob` with the token
//! 4. Server validates and responds with `SetCustomMiningJobSuccess` or error
//! 5. Client can submit solutions via `PushSolution`

// TODO: Uncomment these modules as they are implemented
pub mod codec;
pub mod config;
pub mod error;
pub mod messages;
pub mod token;
// pub mod server;

// TODO: Re-export types as modules are implemented
pub use codec::{
    decode_allocate_token, decode_allocate_token_success, decode_push_solution,
    decode_set_custom_job, decode_set_custom_job_error, decode_set_custom_job_success,
    encode_allocate_token, encode_allocate_token_success, encode_push_solution,
    encode_set_custom_job, encode_set_custom_job_error, encode_set_custom_job_success,
};
pub use config::JdServerConfig;
pub use error::{JdServerError, Result};
pub use messages::*;
pub use token::{DeclaredJobInfo, MiningJobToken, TokenManager};
// pub use server::JdServer;
