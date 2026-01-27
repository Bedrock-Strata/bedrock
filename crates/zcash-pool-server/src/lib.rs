//! Zcash Pool Server for Stratum V2
//!
//! This crate provides a basic pool server that:
//! - Accepts miner connections over TCP
//! - Distributes Equihash mining jobs
//! - Validates submitted shares
//! - Tracks contributions for PPS payout

pub mod channel;
pub mod config;
pub mod duplicate;
pub mod error;
pub mod job;
pub mod payout;
pub mod session;
pub mod share;

// TODO: These modules will be implemented in subsequent tasks
// pub mod server;

pub use channel::{Channel, ChannelJob};
pub use config::PoolConfig;
pub use duplicate::{DuplicateDetector, InMemoryDuplicateDetector};
pub use error::PoolError;
pub use job::JobDistributor;
pub use payout::{MinerId, MinerStats, PayoutTracker};
pub use session::{ServerMessage, Session, SessionMessage};
pub use share::{ShareProcessor, ShareValidationResult};
// pub use server::PoolServer;
