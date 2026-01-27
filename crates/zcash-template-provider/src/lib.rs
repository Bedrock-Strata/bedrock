//! Zcash Template Provider for Stratum V2
//!
//! This crate provides a Template Provider that interfaces with Zebra nodes
//! and produces SV2-compatible block templates for Equihash mining.

pub mod commitments;
pub mod error;
pub mod rpc;
pub mod template;
pub mod types;

pub use commitments::calculate_block_commitments_hash;
pub use error::Error;
pub use template::TemplateProvider;
