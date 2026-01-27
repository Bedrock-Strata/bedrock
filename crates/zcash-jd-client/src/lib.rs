//! Job Declaration Client for Zcash Stratum V2
//!
//! This crate provides the JD Client that:
//! - Connects to a local Zebra node via Template Provider
//! - Builds custom block templates
//! - Declares jobs to a pool's JD Server
//! - Submits found blocks to both Zebra and the pool

pub mod block_submitter;
pub mod client;
pub mod config;
pub mod error;
pub mod full_template;
pub mod template_builder;

pub use block_submitter::BlockSubmitter;
pub use client::JdClient;
pub use config::{JdClientConfig, TxSelectionStrategy};
pub use error::JdClientError;
pub use full_template::FullTemplateBuilder;
pub use template_builder::TemplateBuilder;
