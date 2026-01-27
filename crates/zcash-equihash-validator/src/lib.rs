//! Equihash solution validation for Zcash Stratum V2
//!
//! This crate provides:
//! - Equihash (200,9) solution verification
//! - Share difficulty validation
//! - Adaptive variable difficulty (vardiff) algorithm

pub mod error;
pub mod validator;
pub mod difficulty;
pub mod vardiff;

pub use error::ValidationError;
pub use validator::EquihashValidator;
pub use difficulty::{Target, compact_to_target, target_to_difficulty, difficulty_to_target};
pub use vardiff::{VardiffController, VardiffConfig, VardiffStats};
