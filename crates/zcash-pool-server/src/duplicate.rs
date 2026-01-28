//! Duplicate share detection
//!
//! Uses a trait to allow swapping implementations (in-memory, Redis, etc.)

use rustc_hash::FxHashSet;
use std::collections::HashMap;
use std::sync::RwLock;

/// Trait for duplicate share detection
pub trait DuplicateDetector: Send + Sync {
    /// Check if a share is a duplicate (and record it if not)
    /// Returns true if it IS a duplicate, false if it's new
    fn check_and_record(&self, job_id: u32, nonce_2: &[u8], solution: &[u8]) -> bool;

    /// Clear all shares for a job (called when job expires)
    fn clear_job(&self, job_id: u32);

    /// Clear all jobs (called on new block)
    fn clear_all(&self);
}

/// In-memory duplicate detector using hash sets
pub struct InMemoryDuplicateDetector {
    /// Map of job_id -> set of share hashes
    jobs: RwLock<HashMap<u32, FxHashSet<u64>>>,
}

impl InMemoryDuplicateDetector {
    pub fn new() -> Self {
        Self {
            jobs: RwLock::new(HashMap::new()),
        }
    }

    /// Compute a fast hash of the share data
    fn hash_share(nonce_2: &[u8], solution: &[u8]) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = rustc_hash::FxHasher::default();
        nonce_2.hash(&mut hasher);
        // Hash the full solution to prevent collision attacks
        // FxHasher is fast enough that hashing 1344 bytes is negligible
        solution.hash(&mut hasher);
        hasher.finish()
    }
}

impl Default for InMemoryDuplicateDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl DuplicateDetector for InMemoryDuplicateDetector {
    fn check_and_record(&self, job_id: u32, nonce_2: &[u8], solution: &[u8]) -> bool {
        let hash = Self::hash_share(nonce_2, solution);

        // Handle poisoned lock gracefully - continue operating even if another thread panicked
        let mut jobs = self.jobs.write().unwrap_or_else(|e| e.into_inner());
        let shares = jobs.entry(job_id).or_default();

        // insert returns true if the value was NOT present
        // So we return the opposite: true if it IS a duplicate
        !shares.insert(hash)
    }

    fn clear_job(&self, job_id: u32) {
        let mut jobs = self.jobs.write().unwrap_or_else(|e| e.into_inner());
        jobs.remove(&job_id);
    }

    fn clear_all(&self) {
        let mut jobs = self.jobs.write().unwrap_or_else(|e| e.into_inner());
        jobs.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_duplicate_detection() {
        let detector = InMemoryDuplicateDetector::new();

        let nonce_2 = vec![0x01, 0x02, 0x03];
        let solution = vec![0xaa; 1344];

        // First submission - not a duplicate
        assert!(!detector.check_and_record(1, &nonce_2, &solution));

        // Same submission - is a duplicate
        assert!(detector.check_and_record(1, &nonce_2, &solution));

        // Different nonce_2 - not a duplicate
        let nonce_2_b = vec![0x04, 0x05, 0x06];
        assert!(!detector.check_and_record(1, &nonce_2_b, &solution));

        // Different job - not a duplicate
        assert!(!detector.check_and_record(2, &nonce_2, &solution));
    }

    #[test]
    fn test_clear_job() {
        let detector = InMemoryDuplicateDetector::new();

        let nonce_2 = vec![0x01, 0x02, 0x03];
        let solution = vec![0xaa; 1344];

        detector.check_and_record(1, &nonce_2, &solution);
        assert!(detector.check_and_record(1, &nonce_2, &solution)); // duplicate

        detector.clear_job(1);

        // After clear, same share is not a duplicate
        assert!(!detector.check_and_record(1, &nonce_2, &solution));
    }

    #[test]
    fn test_clear_all() {
        let detector = InMemoryDuplicateDetector::new();

        let nonce_2 = vec![0x01, 0x02, 0x03];
        let solution = vec![0xaa; 1344];

        detector.check_and_record(1, &nonce_2, &solution);
        detector.check_and_record(2, &nonce_2, &solution);

        detector.clear_all();

        // After clear_all, both are not duplicates
        assert!(!detector.check_and_record(1, &nonce_2, &solution));
        assert!(!detector.check_and_record(2, &nonce_2, &solution));
    }
}
