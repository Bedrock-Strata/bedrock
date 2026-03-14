//! Share processing and validation
//!
//! Validates submitted shares using the Equihash validator from Phase 2.

use crate::channel::Channel;
use crate::duplicate::DuplicateDetector;
use crate::error::{PoolError, Result};
use tracing::{debug, warn};
use zcash_equihash_validator::EquihashValidator;
use zcash_equihash_validator::difficulty::{Target, target_to_difficulty};
use zcash_mining_protocol::messages::{NewEquihashJob, RejectReason, ShareResult, SubmitEquihashShare};

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

        self.validate_share_with_job(share, &channel_job.job, duplicate_detector, block_target)
    }

    /// Validate a submitted share given the job data
    pub fn validate_share_with_job<D: DuplicateDetector>(
        &self,
        share: &SubmitEquihashShare,
        job: &NewEquihashJob,
        duplicate_detector: &D,
        block_target: &[u8; 32],
    ) -> Result<ShareValidationResult> {
        // 1. Check for duplicate
        if duplicate_detector.check_and_record(share.job_id, &share.nonce_2, &share.solution) {
            debug!("Duplicate share for job {}", share.job_id);
            return Ok(ShareValidationResult {
                accepted: false,
                result: ShareResult::Rejected(RejectReason::Duplicate),
                difficulty: None,
                is_block: false,
            });
        }

        // 2. Validate share timestamp is within consensus-acceptable range.
        //    Miners can roll ntime forward but not too far. A block-qualifying
        //    share with an invalid timestamp gets rejected by Zebra, wasting
        //    the block find.
        const MAX_TIME_FORWARD: u32 = 7200; // 2 hours, matches Zcash/Bitcoin consensus
        const MAX_TIME_BACKWARD: u32 = 60; // 1 minute tolerance for clock skew
        if share.time < job.time.saturating_sub(MAX_TIME_BACKWARD)
            || share.time > job.time.saturating_add(MAX_TIME_FORWARD)
        {
            debug!(
                "Share timestamp {} out of range (job time: {}, allowed: {}-{})",
                share.time,
                job.time,
                job.time.saturating_sub(MAX_TIME_BACKWARD),
                job.time.saturating_add(MAX_TIME_FORWARD),
            );
            return Ok(ShareValidationResult {
                accepted: false,
                result: ShareResult::Rejected(RejectReason::Other(
                    "timestamp out of range".to_string(),
                )),
                difficulty: None,
                is_block: false,
            });
        }

        // 3. Build full nonce and header
        let full_nonce = job.build_nonce(&share.nonce_2).ok_or_else(|| {
            PoolError::InvalidMessage("Invalid nonce_2 length".to_string())
        })?;

        let mut header = job.build_header(&full_nonce);
        // Update time if miner changed it (already validated above)
        header[100..104].copy_from_slice(&share.time.to_le_bytes());

        // 4. Verify Equihash solution AND check share meets pool target.
        //    verify_share calls verify_solution internally, so we only call it
        //    once to avoid the expensive (~144 MB) duplicate Equihash verification.
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
            Err(zcash_equihash_validator::ValidationError::TargetNotMet) => {
                debug!("Share below target difficulty");
                Ok(ShareValidationResult {
                    accepted: false,
                    result: ShareResult::Rejected(RejectReason::LowDifficulty),
                    difficulty: None,
                    is_block: false,
                })
            }
            Err(e) => {
                debug!("Invalid solution: {}", e);
                Ok(ShareValidationResult {
                    accepted: false,
                    result: ShareResult::Rejected(RejectReason::InvalidSolution),
                    difficulty: None,
                    is_block: false,
                })
            }
        }
    }

    /// Convert hash to difficulty
    fn hash_to_difficulty(&self, hash: &[u8; 32]) -> f64 {
        let target = Target::from_le_bytes(*hash);
        target_to_difficulty(&target)
    }

    /// Check if hash meets target using the canonical Target::is_met_by()
    fn meets_target(&self, hash: &[u8; 32], target: &[u8; 32]) -> bool {
        let target = Target::from_le_bytes(*target);
        target.is_met_by(hash)
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

        // All 0xff = very low difficulty
        let hash_ones = [0xff; 32];
        let diff = processor.hash_to_difficulty(&hash_ones);
        assert!(diff < 1.0);
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

    #[test]
    fn test_share_time_validation() {
        use crate::duplicate::InMemoryDuplicateDetector;
        let processor = ShareProcessor::new();
        let detector = InMemoryDuplicateDetector::new();
        let block_target = [0xff; 32];
        let job_time: u32 = 1_700_000_000;

        let job = NewEquihashJob {
            channel_id: 1,
            job_id: 1,
            future_job: false,
            version: 5,
            prev_hash: [0; 32],
            merkle_root: [0; 32],
            block_commitments: [0; 32],
            nonce_1: vec![0; 4],
            nonce_2_len: 28,
            time: job_time,
            bits: 0x2007ffff,
            target: [0xff; 32],
            clean_jobs: false,
        };

        // Share with timestamp too far in the future (>2 hours)
        let future_share = SubmitEquihashShare {
            channel_id: 1,
            sequence_number: 1,
            job_id: 1,
            nonce_2: vec![0; 28],
            time: job_time + 7201, // 2 hours + 1 second
            solution: [0; 1344],
        };
        let result = processor.validate_share_with_job(&future_share, &job, &detector, &block_target).unwrap();
        assert!(!result.accepted, "Share with timestamp >2h in future should be rejected");

        // Share with timestamp too far in the past (>60s before job)
        let past_share = SubmitEquihashShare {
            channel_id: 1,
            sequence_number: 2,
            job_id: 1,
            nonce_2: vec![0; 28],
            time: job_time - 61,
            solution: [0; 1344],
        };
        let result = processor.validate_share_with_job(&past_share, &job, &detector, &block_target).unwrap();
        assert!(!result.accepted, "Share with timestamp >60s before job time should be rejected");

        // Share with valid timestamp (same as job time) should pass time check
        // (it will fail Equihash validation, but that's expected - the point is
        // it doesn't get rejected for timestamp)
        let valid_time_share = SubmitEquihashShare {
            channel_id: 1,
            sequence_number: 3,
            job_id: 1,
            nonce_2: vec![0; 28],
            time: job_time,
            solution: [0; 1344],
        };
        let result = processor.validate_share_with_job(&valid_time_share, &job, &detector, &block_target).unwrap();
        // Should NOT be rejected for timestamp - will be rejected for invalid solution instead
        match result.result {
            ShareResult::Rejected(RejectReason::Other(_)) => {
                panic!("Valid timestamp should not trigger timestamp rejection");
            }
            _ => {} // Any other result (accepted or rejected for solution) is fine
        }
    }

    /// Helper to create a Channel with a given nonce_1 for tests
    fn make_test_channel(nonce_1: Vec<u8>) -> Channel {
        use zcash_equihash_validator::VardiffConfig;
        Channel::new_with_id(1, nonce_1, VardiffConfig::default()).unwrap()
    }

    /// Helper to create a NewEquihashJob for tests
    fn make_test_job(job_id: u32, nonce_1: &[u8], nonce_2_len: u8, time: u32) -> NewEquihashJob {
        NewEquihashJob {
            channel_id: 1,
            job_id,
            future_job: false,
            version: 5,
            prev_hash: [0; 32],
            merkle_root: [0; 32],
            block_commitments: [0; 32],
            nonce_1: nonce_1.to_vec(),
            nonce_2_len,
            time,
            bits: 0x2007ffff,
            target: [0xff; 32],
            clean_jobs: false,
        }
    }

    #[test]
    fn test_validate_share_unknown_job() {
        use crate::duplicate::InMemoryDuplicateDetector;

        let mut channel = make_test_channel(vec![0; 4]);
        let job = make_test_job(1, &channel.nonce_1, channel.nonce_2_len, 1_700_000_000);
        channel.add_job(job, false);

        let processor = ShareProcessor::new();
        let detector = InMemoryDuplicateDetector::new();
        let block_target = [0xff; 32];

        let share = SubmitEquihashShare {
            channel_id: 1,
            sequence_number: 1,
            job_id: 999,
            nonce_2: vec![0; 28],
            time: 1_700_000_000,
            solution: [0; 1344],
        };

        let result = processor.validate_share(&share, &channel, &detector, &block_target);
        assert!(result.is_err());
        match result.unwrap_err() {
            PoolError::UnknownJob(id) => assert_eq!(id, 999),
            other => panic!("Expected UnknownJob(999), got: {:?}", other),
        }
    }

    #[test]
    fn test_validate_share_stale_job() {
        use crate::duplicate::InMemoryDuplicateDetector;

        let mut channel = make_test_channel(vec![0; 4]);
        let job1 = make_test_job(1, &channel.nonce_1, channel.nonce_2_len, 1_700_000_000);
        channel.add_job(job1, false);

        let job2 = make_test_job(2, &channel.nonce_1, channel.nonce_2_len, 1_700_000_000);
        channel.add_job(job2, true); // clean_jobs=true marks job 1 stale

        let processor = ShareProcessor::new();
        let detector = InMemoryDuplicateDetector::new();
        let block_target = [0xff; 32];

        let share = SubmitEquihashShare {
            channel_id: 1,
            sequence_number: 1,
            job_id: 1,
            nonce_2: vec![0; 28],
            time: 1_700_000_000,
            solution: [0; 1344],
        };

        let result = processor.validate_share(&share, &channel, &detector, &block_target).unwrap();
        assert!(!result.accepted);
        assert!(
            matches!(result.result, ShareResult::Rejected(RejectReason::StaleJob)),
            "Expected StaleJob rejection, got: {:?}",
            result.result
        );
    }

    #[test]
    fn test_validate_share_wrong_nonce2_length() {
        use crate::duplicate::InMemoryDuplicateDetector;

        let mut channel = make_test_channel(vec![0; 4]); // nonce_2_len = 28
        let job = make_test_job(1, &channel.nonce_1, channel.nonce_2_len, 1_700_000_000);
        channel.add_job(job, false);

        let processor = ShareProcessor::new();
        let detector = InMemoryDuplicateDetector::new();
        let block_target = [0xff; 32];

        let share = SubmitEquihashShare {
            channel_id: 1,
            sequence_number: 1,
            job_id: 1,
            nonce_2: vec![0; 10], // Wrong length: 10 instead of 28
            time: 1_700_000_000,
            solution: [0; 1344],
        };

        let result = processor.validate_share(&share, &channel, &detector, &block_target);
        assert!(result.is_err());
        match result.unwrap_err() {
            PoolError::InvalidMessage(msg) => {
                assert!(msg.contains("nonce"), "Error message should mention nonce: {}", msg);
            }
            other => panic!("Expected InvalidMessage, got: {:?}", other),
        }
    }

    #[test]
    fn test_validate_share_duplicate_via_channel() {
        use crate::duplicate::InMemoryDuplicateDetector;

        let mut channel = make_test_channel(vec![0; 4]);
        let job = make_test_job(1, &channel.nonce_1, channel.nonce_2_len, 1_700_000_000);
        channel.add_job(job, false);

        let processor = ShareProcessor::new();
        let detector = InMemoryDuplicateDetector::new();
        let block_target = [0xff; 32];

        let share = SubmitEquihashShare {
            channel_id: 1,
            sequence_number: 1,
            job_id: 1,
            nonce_2: vec![0; 28],
            time: 1_700_000_000,
            solution: [0; 1344],
        };

        // First submission -- will get InvalidSolution (dummy solution), but not Duplicate
        let result1 = processor.validate_share(&share, &channel, &detector, &block_target).unwrap();
        assert!(
            !matches!(result1.result, ShareResult::Rejected(RejectReason::Duplicate)),
            "First submission should not be duplicate, got: {:?}",
            result1.result
        );

        // Second submission of exact same share -- should get Duplicate
        let result2 = processor.validate_share(&share, &channel, &detector, &block_target).unwrap();
        assert!(
            matches!(result2.result, ShareResult::Rejected(RejectReason::Duplicate)),
            "Second submission should be duplicate, got: {:?}",
            result2.result
        );
    }

    #[test]
    fn test_timestamp_boundary_acceptance() {
        use crate::duplicate::InMemoryDuplicateDetector;

        let mut channel = make_test_channel(vec![0; 4]);
        let job_time: u32 = 1_700_000_000;
        let job = make_test_job(1, &channel.nonce_1, channel.nonce_2_len, job_time);
        channel.add_job(job, false);

        let processor = ShareProcessor::new();
        let block_target = [0xff; 32];

        // Share at exactly job_time - 60 (boundary, should NOT be rejected for timestamp)
        let detector1 = InMemoryDuplicateDetector::new();
        let share_at_lower_bound = SubmitEquihashShare {
            channel_id: 1,
            sequence_number: 1,
            job_id: 1,
            nonce_2: vec![0; 28],
            time: job_time - 60,
            solution: [0; 1344],
        };
        let result = processor.validate_share(&share_at_lower_bound, &channel, &detector1, &block_target).unwrap();
        match result.result {
            ShareResult::Rejected(RejectReason::Other(ref msg)) if msg.contains("timestamp") => {
                panic!("time=job_time-60 should be accepted by timestamp check, got: {:?}", result.result);
            }
            _ => {} // InvalidSolution or anything else is fine
        }

        // Share at exactly job_time + 7200 (boundary, should NOT be rejected for timestamp)
        let detector2 = InMemoryDuplicateDetector::new();
        let share_at_upper_bound = SubmitEquihashShare {
            channel_id: 1,
            sequence_number: 2,
            job_id: 1,
            nonce_2: vec![1; 28], // Different nonce to avoid duplicate
            time: job_time + 7200,
            solution: [0; 1344],
        };
        let result = processor.validate_share(&share_at_upper_bound, &channel, &detector2, &block_target).unwrap();
        match result.result {
            ShareResult::Rejected(RejectReason::Other(ref msg)) if msg.contains("timestamp") => {
                panic!("time=job_time+7200 should be accepted by timestamp check, got: {:?}", result.result);
            }
            _ => {} // InvalidSolution or anything else is fine
        }
    }
}
