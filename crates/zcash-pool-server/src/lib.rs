//! Zcash Pool Server for Stratum V2
//!
//! This crate provides a basic pool server that:
//! - Accepts miner connections over TCP
//! - Distributes Equihash mining jobs
//! - Validates submitted shares
//! - Tracks contributions for PPS payout

pub mod config;
pub mod duplicate;
pub mod error;

// TODO: These modules will be implemented in subsequent tasks
// pub mod server;
// pub mod session;
// pub mod channel;
// pub mod job;
// pub mod share;
// pub mod payout;

pub use config::PoolConfig;
pub use duplicate::{DuplicateDetector, InMemoryDuplicateDetector};
pub use error::PoolError;
// pub use server::PoolServer;
