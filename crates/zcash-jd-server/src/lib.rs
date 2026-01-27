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
pub mod config;
pub mod error;
pub mod messages;
// pub mod codec;
// pub mod token;
// pub mod server;

// TODO: Re-export types as modules are implemented
pub use config::JdServerConfig;
pub use error::{JdServerError, Result};
pub use messages::*;
// pub use server::JdServer;
