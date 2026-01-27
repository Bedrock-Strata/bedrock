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

// TODO: These modules will be implemented in subsequent tasks
// pub mod server;
// pub mod session;
// pub mod share;

pub use channel::{Channel, ChannelJob};
pub use config::PoolConfig;
pub use duplicate::{DuplicateDetector, InMemoryDuplicateDetector};
pub use error::PoolError;
pub use job::JobDistributor;
pub use payout::{MinerId, MinerStats, PayoutTracker};
// pub use server::PoolServer;
