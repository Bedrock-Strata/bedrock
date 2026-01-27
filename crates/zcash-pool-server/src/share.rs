//! Share processing and validation
//!
//! Validates submitted shares using the Equihash validator from Phase 2.

use crate::channel::Channel;
use crate::duplicate::DuplicateDetector;
use crate::error::{PoolError, Result};
use tracing::{debug, warn};
use zcash_equihash_validator::EquihashValidator;
use zcash_mining_protocol::messages::{RejectReason, ShareResult, SubmitEquihashShare};

/// Result of share validation
#[derive(Debug)]
pub struct ShareValidationResult {
    /// Whether the share was accepted
    pub accepted: bool,
    /// The share result to send back
    pub result: ShareResult,
    /// The share's difficulty (if valid solution)
    pub difficulty: Option<f64>,
    /// Whether this share is a valid block
    pub is_block: bool,
}

/// Share processor with Equihash validation
pub struct ShareProcessor {
    validator: EquihashValidator,
}

impl ShareProcessor {
    pub fn new() -> Self {
        Self {
            validator: EquihashValidator::new(),
        }
    }

    /// Validate a submitted share
    pub fn validate_share<D: DuplicateDetector>(
        &self,
        share: &SubmitEquihashShare,
        channel: &Channel,
        duplicate_detector: &D,
        block_target: &[u8; 32],
    ) -> Result<ShareValidationResult> {
        // 1. Check job exists and is active
        let channel_job = channel.get_job(share.job_id).ok_or_else(|| {
            warn!("Unknown job {} for channel {}", share.job_id, channel.id);
            PoolError::UnknownJob(share.job_id)
        })?;

        if !channel_job.active {
            debug!("Stale share for job {}", share.job_id);
            return Ok(ShareValidationResult {
                accepted: false,
                result: ShareResult::Rejected(RejectReason::StaleJob),
                difficulty: None,
                is_block: false,
            });
        }

        // 2. Check for duplicate
        if duplicate_detector.check_and_record(share.job_id, &share.nonce_2, &share.solution) {
            debug!("Duplicate share for job {}", share.job_id);
            return Ok(ShareValidationResult {
                accepted: false,
                result: ShareResult::Rejected(RejectReason::Duplicate),
                difficulty: None,
                is_block: false,
            });
        }

        // 3. Build full nonce and header
        let job = &channel_job.job;
        let full_nonce = job.build_nonce(&share.nonce_2).ok_or_else(|| {
            PoolError::InvalidMessage("Invalid nonce_2 length".to_string())
        })?;

        let mut header = job.build_header(&full_nonce);
        // Update time if miner changed it
        header[100..104].copy_from_slice(&share.time.to_le_bytes());

        // 4. Verify Equihash solution
        if let Err(e) = self.validator.verify_solution(&header, &share.solution) {
            debug!("Invalid solution: {}", e);
            return Ok(ShareValidationResult {
                accepted: false,
                result: ShareResult::Rejected(RejectReason::InvalidSolution),
                difficulty: None,
                is_block: false,
            });
        }

        // 5. Check share meets pool target
        let share_target = &job.target;
        match self.validator.verify_share(&header, &share.solution, share_target) {
            Ok(hash) => {
                // Calculate share difficulty
                let difficulty = self.hash_to_difficulty(&hash);

                // Check if this meets block target
                let is_block = self.meets_target(&hash, block_target);

                debug!(
                    "Valid share: difficulty={:.2}, is_block={}",
                    difficulty, is_block
                );

                Ok(ShareValidationResult {
                    accepted: true,
                    result: ShareResult::Accepted,
                    difficulty: Some(difficulty),
                    is_block,
                })
            }
            Err(_) => {
                debug!("Share below target difficulty");
                Ok(ShareValidationResult {
                    accepted: false,
                    result: ShareResult::Rejected(RejectReason::LowDifficulty),
                    difficulty: None,
                    is_block: false,
                })
            }
        }
    }

    /// Convert hash to difficulty
    fn hash_to_difficulty(&self, hash: &[u8; 32]) -> f64 {
        // Simplified: count leading zero bits and estimate
        let mut leading_zeros = 0u32;
        for &byte in hash.iter().rev() {
            if byte == 0 {
                leading_zeros += 8;
            } else {
                leading_zeros += byte.leading_zeros();
                break;
            }
        }
        2.0f64.powi(leading_zeros as i32)
    }

    /// Check if hash meets target
    fn meets_target(&self, hash: &[u8; 32], target: &[u8; 32]) -> bool {
        // Compare as little-endian 256-bit integers
        for i in (0..32).rev() {
            if hash[i] < target[i] {
                return true;
            }
            if hash[i] > target[i] {
                return false;
            }
        }
        true
    }
}

impl Default for ShareProcessor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_to_difficulty() {
        let processor = ShareProcessor::new();

        // All zeros = maximum difficulty
        let hash_zeros = [0u8; 32];
        let diff = processor.hash_to_difficulty(&hash_zeros);
        assert!(diff > 1e70); // Very high

        // All 0xff = minimum difficulty
        let hash_ones = [0xff; 32];
        let diff = processor.hash_to_difficulty(&hash_ones);
        assert_eq!(diff, 1.0);
    }

    #[test]
    fn test_meets_target() {
        let processor = ShareProcessor::new();

        let low_hash = [0x00; 32];
        let high_target = [0xff; 32];
        assert!(processor.meets_target(&low_hash, &high_target));

        let high_hash = [0xff; 32];
        let low_target = [0x00; 32];
        assert!(!processor.meets_target(&high_hash, &low_target));
    }
}
