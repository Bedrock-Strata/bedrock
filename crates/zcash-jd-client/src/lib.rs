//! Job Declaration Client for Zcash Stratum V2
//!
//! This crate provides the JD Client that:
//! - Connects to a local Zebra node via Template Provider
//! - Builds custom block templates
//! - Declares jobs to a pool's JD Server
//! - Submits found blocks to both Zebra and the pool

// pub mod client;
pub mod config;
pub mod error;
// pub mod template_builder;
// pub mod block_submitter;

// pub use client::JdClient;
pub use config::JdClientConfig;
pub use error::JdClientError;
