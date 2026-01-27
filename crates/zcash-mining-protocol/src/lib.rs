//! Zcash Mining Protocol for Stratum V2
//!
//! This crate defines the message types for Equihash mining:
//! - NewEquihashJob: Pool → Miner job distribution
//! - SubmitEquihashShare: Miner → Pool share submission
//! - Channel management messages

pub mod error;
pub mod messages;
pub mod codec;

pub use error::ProtocolError;
pub use messages::{NewEquihashJob, SubmitEquihashShare, SubmitSharesResponse, ShareResult};
