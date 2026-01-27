//! Simple PPS (Pay Per Share) tracking
//!
//! Tracks share submissions per miner for payout calculation.
//! In-memory for Phase 3; can be upgraded to database-backed later.

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

/// Unique identifier for a miner (could be pubkey, address, etc.)
pub type MinerId = String;

/// Per-miner statistics
#[derive(Debug, Clone, Default)]
pub struct MinerStats {
    /// Total shares submitted
    pub total_shares: u64,
    /// Total difficulty (sum of share difficulties)
    pub total_difficulty: f64,
    /// Shares in current window
    pub window_shares: u64,
    /// Difficulty in current window
    pub window_difficulty: f64,
    /// Last share timestamp
    pub last_share: Option<Instant>,
}

/// PPS payout tracker
pub struct PayoutTracker {
    /// Per-miner statistics
    miners: RwLock<HashMap<MinerId, MinerStats>>,
    /// Window duration for rate calculations
    window_duration: Duration,
}

impl PayoutTracker {
    pub fn new(window_duration: Duration) -> Self {
        Self {
            miners: RwLock::new(HashMap::new()),
            window_duration,
        }
    }

    /// Record a share for a miner
    pub fn record_share(&self, miner_id: &MinerId, difficulty: f64) {
        let mut miners = self.miners.write().unwrap();
        let stats = miners.entry(miner_id.clone()).or_default();

        stats.total_shares += 1;
        stats.total_difficulty += difficulty;
        stats.window_shares += 1;
        stats.window_difficulty += difficulty;
        stats.last_share = Some(Instant::now());
    }

    /// Get statistics for a miner
    pub fn get_stats(&self, miner_id: &MinerId) -> Option<MinerStats> {
        let miners = self.miners.read().unwrap();
        miners.get(miner_id).cloned()
    }

    /// Get all miner statistics
    pub fn get_all_stats(&self) -> HashMap<MinerId, MinerStats> {
        let miners = self.miners.read().unwrap();
        miners.clone()
    }

    /// Reset window statistics (call periodically)
    pub fn reset_window(&self) {
        let mut miners = self.miners.write().unwrap();
        for stats in miners.values_mut() {
            stats.window_shares = 0;
            stats.window_difficulty = 0.0;
        }
    }

    /// Get total pool hashrate estimate (based on difficulty sum over window)
    pub fn estimate_pool_hashrate(&self) -> f64 {
        let miners = self.miners.read().unwrap();
        let total_difficulty: f64 = miners.values().map(|s| s.window_difficulty).sum();

        // Hashrate = difficulty / time (simplified)
        total_difficulty / self.window_duration.as_secs_f64()
    }

    /// Number of active miners (submitted share in window)
    pub fn active_miner_count(&self) -> usize {
        let miners = self.miners.read().unwrap();
        let cutoff = Instant::now() - self.window_duration;
        miners
            .values()
            .filter(|s| s.last_share.map(|t| t > cutoff).unwrap_or(false))
            .count()
    }
}

impl Default for PayoutTracker {
    fn default() -> Self {
        Self::new(Duration::from_secs(600)) // 10 minute window
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_share() {
        let tracker = PayoutTracker::default();
        let miner = "miner1".to_string();

        tracker.record_share(&miner, 100.0);
        tracker.record_share(&miner, 200.0);

        let stats = tracker.get_stats(&miner).unwrap();
        assert_eq!(stats.total_shares, 2);
        assert_eq!(stats.total_difficulty, 300.0);
    }

    #[test]
    fn test_multiple_miners() {
        let tracker = PayoutTracker::default();

        tracker.record_share(&"miner1".to_string(), 100.0);
        tracker.record_share(&"miner2".to_string(), 200.0);
        tracker.record_share(&"miner1".to_string(), 50.0);

        let stats1 = tracker.get_stats(&"miner1".to_string()).unwrap();
        let stats2 = tracker.get_stats(&"miner2".to_string()).unwrap();

        assert_eq!(stats1.total_difficulty, 150.0);
        assert_eq!(stats2.total_difficulty, 200.0);
    }

    #[test]
    fn test_reset_window() {
        let tracker = PayoutTracker::default();
        let miner = "miner1".to_string();

        tracker.record_share(&miner, 100.0);
        tracker.reset_window();
        tracker.record_share(&miner, 50.0);

        let stats = tracker.get_stats(&miner).unwrap();
        assert_eq!(stats.total_difficulty, 150.0); // Total preserved
        assert_eq!(stats.window_difficulty, 50.0); // Window reset
    }

    #[test]
    fn test_get_all_stats() {
        let tracker = PayoutTracker::default();

        tracker.record_share(&"miner1".to_string(), 100.0);
        tracker.record_share(&"miner2".to_string(), 200.0);

        let all = tracker.get_all_stats();
        assert_eq!(all.len(), 2);
    }
}
