//! Common types shared between pool server components
//!
//! This crate provides shared types used by both the pool server
//! and the JD server, avoiding circular dependencies.

pub mod payout;

pub use payout::{MinerId, MinerStats, PayoutTracker};
