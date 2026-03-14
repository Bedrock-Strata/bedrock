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
    /// When the current window started (first share in window)
    window_start: RwLock<Option<Instant>>,
}

impl PayoutTracker {
    pub fn new(window_duration: Duration) -> Self {
        Self {
            miners: RwLock::new(HashMap::new()),
            window_duration,
            window_start: RwLock::new(None),
        }
    }

    /// Record a share for a miner
    ///
    /// Validates that difficulty is finite and positive before recording.
    /// Ignores shares with invalid difficulty (NaN, Infinity, negative, zero)
    /// to prevent poisoning payout calculations.
    pub fn record_share(&self, miner_id: &MinerId, difficulty: f64) {
        // Guard against NaN, Infinity, negative, and zero difficulty
        if !difficulty.is_finite() || difficulty <= 0.0 {
            tracing::warn!(
                "Ignoring share with invalid difficulty {} for miner {}",
                difficulty, miner_id
            );
            return;
        }

        let now = Instant::now();

        // Set window start on first share in window
        {
            let mut window_start = self.window_start.write().unwrap_or_else(|e| e.into_inner());
            if window_start.is_none() {
                *window_start = Some(now);
            }
        }

        // Handle poisoned lock gracefully - continue operating even if another thread panicked
        let mut miners = self.miners.write().unwrap_or_else(|e| e.into_inner());
        let stats = miners.entry(miner_id.clone()).or_default();

        stats.total_shares += 1;
        stats.total_difficulty += difficulty;
        stats.window_shares += 1;
        stats.window_difficulty += difficulty;
        stats.last_share = Some(now);
    }

    /// Get statistics for a miner
    pub fn get_stats(&self, miner_id: &MinerId) -> Option<MinerStats> {
        let miners = self.miners.read().unwrap_or_else(|e| e.into_inner());
        miners.get(miner_id).cloned()
    }

    /// Get all miner statistics
    pub fn get_all_stats(&self) -> HashMap<MinerId, MinerStats> {
        let miners = self.miners.read().unwrap_or_else(|e| e.into_inner());
        miners.clone()
    }

    /// Reset window statistics (call periodically)
    pub fn reset_window(&self) {
        // Reset window start time
        {
            let mut window_start = self.window_start.write().unwrap_or_else(|e| e.into_inner());
            *window_start = None;
        }

        let mut miners = self.miners.write().unwrap_or_else(|e| e.into_inner());
        for stats in miners.values_mut() {
            stats.window_shares = 0;
            stats.window_difficulty = 0.0;
        }
    }

    /// Rotate the rolling window once it has reached the configured duration.
    pub fn rotate_window_if_needed(&self) {
        let should_reset = {
            let window_start = self.window_start.read().unwrap_or_else(|e| e.into_inner());
            window_start
                .map(|start| start.elapsed() >= self.window_duration)
                .unwrap_or(false)
        };

        if should_reset {
            self.reset_window();
        }
    }

    /// Get total pool hashrate estimate (based on difficulty sum over window)
    pub fn estimate_pool_hashrate(&self) -> f64 {
        let miners = self.miners.read().unwrap_or_else(|e| e.into_inner());
        let total_difficulty: f64 = miners.values().map(|s| s.window_difficulty).sum();

        // Use actual elapsed time, capped at window_duration
        let elapsed = {
            let window_start = self.window_start.read().unwrap_or_else(|e| e.into_inner());
            match *window_start {
                Some(start) => start.elapsed().min(self.window_duration),
                None => return 0.0, // No shares yet
            }
        };

        // Require at least 1 second of data to avoid division issues
        let elapsed_secs = elapsed.as_secs_f64().max(1.0);

        // Hashrate = difficulty / time (simplified)
        total_difficulty / elapsed_secs
    }

    /// Number of active miners (submitted share in window)
    pub fn active_miner_count(&self) -> usize {
        let miners = self.miners.read().unwrap_or_else(|e| e.into_inner());
        // Use checked_sub to avoid panic if window_duration > uptime
        let cutoff = match Instant::now().checked_sub(self.window_duration) {
            Some(t) => t,
            None => return miners.values().filter(|s| s.last_share.is_some()).count(),
        };
        miners
            .values()
            .filter(|s| s.last_share.map(|t| t > cutoff).unwrap_or(false))
            .count()
    }

    /// Remove a miner from the tracker (on disconnect)
    pub fn remove_miner(&self, miner_id: &MinerId) {
        let mut miners = self.miners.write().unwrap_or_else(|e| e.into_inner());
        miners.remove(miner_id);
    }

    /// Remove miners that haven't submitted a share within the given duration.
    ///
    /// Prevents unbounded growth of the miners HashMap when miners
    /// disconnect and reconnect with new channel IDs.
    pub fn cleanup_stale_miners(&self, max_idle: Duration) {
        // Use checked_sub to avoid panic if max_idle > uptime
        let cutoff = match Instant::now().checked_sub(max_idle) {
            Some(t) => t,
            None => return, // All miners are within window, nothing to clean
        };
        let mut miners = self.miners.write().unwrap_or_else(|e| e.into_inner());
        let before = miners.len();
        miners.retain(|_, stats| {
            stats.last_share.map(|t| t > cutoff).unwrap_or(false)
        });
        let removed = before - miners.len();
        if removed > 0 {
            tracing::debug!("Cleaned up {} stale miner entries", removed);
        }
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

    #[test]
    fn test_record_share_rejects_invalid_difficulty() {
        let tracker = PayoutTracker::default();
        let miner = "miner1".to_string();

        // Valid share first
        tracker.record_share(&miner, 100.0);

        // These should all be silently rejected
        tracker.record_share(&miner, f64::NAN);
        tracker.record_share(&miner, f64::INFINITY);
        tracker.record_share(&miner, f64::NEG_INFINITY);
        tracker.record_share(&miner, -1.0);
        tracker.record_share(&miner, 0.0);

        let stats = tracker.get_stats(&miner).unwrap();
        assert_eq!(stats.total_shares, 1); // Only the valid share counted
        assert_eq!(stats.total_difficulty, 100.0); // Not poisoned
    }

    #[test]
    fn test_remove_miner() {
        let tracker = PayoutTracker::default();
        let miner = "miner1".to_string();

        tracker.record_share(&miner, 100.0);
        assert!(tracker.get_stats(&miner).is_some());

        tracker.remove_miner(&miner);
        assert!(tracker.get_stats(&miner).is_none());
    }

    #[test]
    fn test_cleanup_stale_miners() {
        let tracker = PayoutTracker::default();

        tracker.record_share(&"miner1".to_string(), 100.0);
        tracker.record_share(&"miner2".to_string(), 200.0);

        // With 0 duration, all miners are "stale"
        tracker.cleanup_stale_miners(Duration::ZERO);
        assert_eq!(tracker.get_all_stats().len(), 0);
    }

    #[test]
    fn test_estimate_pool_hashrate_no_shares() {
        let tracker = PayoutTracker::default();
        assert_eq!(tracker.estimate_pool_hashrate(), 0.0);
    }

    #[test]
    fn test_estimate_pool_hashrate_with_shares() {
        let tracker = PayoutTracker::default();
        tracker.record_share(&"miner1".to_string(), 100.0);
        tracker.record_share(&"miner2".to_string(), 200.0);
        let rate = tracker.estimate_pool_hashrate();
        assert!(rate > 0.0, "hashrate should be positive after shares");
    }

    #[test]
    fn test_active_miner_count_empty() {
        let tracker = PayoutTracker::default();
        assert_eq!(tracker.active_miner_count(), 0);
    }

    #[test]
    fn test_active_miner_count_with_shares() {
        let tracker = PayoutTracker::default();
        tracker.record_share(&"miner1".to_string(), 100.0);
        tracker.record_share(&"miner2".to_string(), 200.0);
        assert_eq!(tracker.active_miner_count(), 2);
    }

    #[test]
    fn test_remove_miner_decreases_count() {
        let tracker = PayoutTracker::default();
        tracker.record_share(&"miner1".to_string(), 100.0);
        tracker.record_share(&"miner2".to_string(), 200.0);
        tracker.remove_miner(&"miner1".to_string());
        assert!(tracker.get_stats(&"miner1".to_string()).is_none());
        assert!(tracker.get_stats(&"miner2".to_string()).is_some());
    }

    #[test]
    fn test_remove_nonexistent_miner_no_panic() {
        let tracker = PayoutTracker::default();
        tracker.remove_miner(&"ghost".to_string());
    }

    #[test]
    fn test_record_share_many_miners() {
        let tracker = PayoutTracker::default();
        for i in 0..1000 {
            tracker.record_share(&format!("miner_{}", i), 1.0);
        }
        let all = tracker.get_all_stats();
        assert_eq!(all.len(), 1000);
    }

    #[test]
    fn test_window_difficulty_accumulation() {
        let tracker = PayoutTracker::default();
        let miner = "miner1".to_string();
        for _ in 0..100 {
            tracker.record_share(&miner, 1.5);
        }
        let stats = tracker.get_stats(&miner).unwrap();
        assert_eq!(stats.total_shares, 100);
        assert!((stats.total_difficulty - 150.0).abs() < 0.001);
    }

    #[test]
    fn test_rotate_window_if_needed_before_duration() {
        let tracker = PayoutTracker::new(Duration::from_secs(3600));
        let miner = "miner1".to_string();
        tracker.record_share(&miner, 100.0);
        tracker.rotate_window_if_needed();
        let stats = tracker.get_stats(&miner).unwrap();
        assert_eq!(stats.window_shares, 1);
    }

    #[test]
    fn test_rotate_window_if_needed_after_duration() {
        let tracker = PayoutTracker::new(Duration::from_millis(1));
        let miner = "miner1".to_string();
        tracker.record_share(&miner, 100.0);
        std::thread::sleep(Duration::from_millis(5));
        tracker.rotate_window_if_needed();
        let stats = tracker.get_stats(&miner).unwrap();
        assert_eq!(stats.window_shares, 0);
        assert_eq!(stats.total_shares, 1);
    }

    #[test]
    fn test_cleanup_stale_miners_preserves_recent() {
        let tracker = PayoutTracker::default();
        tracker.record_share(&"miner1".to_string(), 100.0);
        tracker.record_share(&"miner2".to_string(), 200.0);
        tracker.cleanup_stale_miners(Duration::from_secs(3600));
        assert_eq!(tracker.get_all_stats().len(), 2);
    }
}
