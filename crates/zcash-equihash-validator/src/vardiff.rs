//! Adaptive Variable Difficulty (Vardiff) Controller
//!
//! Adjusts share difficulty per-miner to maintain a target share rate.
//! Designed for Equihash's ~15-30 second solve times on ASICs.

use crate::difficulty::{difficulty_to_target, Target};
use std::time::{Duration, Instant};
use tracing::{debug, info};

/// Configuration for the vardiff algorithm
#[derive(Debug, Clone)]
pub struct VardiffConfig {
    /// Target shares per minute from each miner
    pub target_shares_per_minute: f64,
    /// Initial difficulty for new miners (clamped to [min, max])
    pub initial_difficulty: f64,
    /// Minimum allowed difficulty
    pub min_difficulty: f64,
    /// Maximum allowed difficulty
    pub max_difficulty: f64,
    /// How often to recalculate difficulty
    pub retarget_interval: Duration,
    /// Tolerance for share rate variance (0.25 = 25%)
    pub variance_tolerance: f64,
}

impl Default for VardiffConfig {
    fn default() -> Self {
        Self {
            // For Equihash ASICs (~420 KSol/s), target 4-6 shares/min
            target_shares_per_minute: 5.0,
            initial_difficulty: 1.0,
            min_difficulty: 1.0,
            max_difficulty: 1_000_000_000.0,
            retarget_interval: Duration::from_secs(60),
            variance_tolerance: 0.25,
        }
    }
}

/// Per-miner vardiff state
#[derive(Debug)]
pub struct VardiffController {
    config: VardiffConfig,
    current_difficulty: f64,
    shares_since_retarget: u32,
    last_retarget: Instant,
    window_start: Instant,
}

impl VardiffController {
    /// Create a new vardiff controller
    pub fn new(config: VardiffConfig) -> Self {
        let now = Instant::now();
        let initial = config.initial_difficulty.clamp(
            config.min_difficulty,
            config.max_difficulty,
        );
        Self {
            current_difficulty: initial,
            config,
            shares_since_retarget: 0,
            last_retarget: now,
            window_start: now,
        }
    }

    /// Get current difficulty
    pub fn current_difficulty(&self) -> f64 {
        self.current_difficulty
    }

    /// Get current target as 256-bit value
    pub fn current_target(&self) -> Target {
        difficulty_to_target(self.current_difficulty)
    }

    /// Set difficulty directly (for initial connection setup)
    pub fn set_difficulty(&mut self, difficulty: f64) {
        self.current_difficulty = difficulty.clamp(
            self.config.min_difficulty,
            self.config.max_difficulty,
        );
        self.reset_window();
        info!("Difficulty set to {:.2}", self.current_difficulty);
    }

    /// Record a submitted share
    pub fn record_share(&mut self) {
        self.shares_since_retarget += 1;
    }

    /// Check if retargeting is needed and adjust difficulty
    ///
    /// Returns `Some(new_difficulty)` if difficulty changed, `None` otherwise
    pub fn maybe_retarget(&mut self) -> Option<f64> {
        let elapsed = self.last_retarget.elapsed();

        if elapsed < self.config.retarget_interval {
            return None;
        }

        let minutes = elapsed.as_secs_f64() / 60.0;
        let actual_rate = self.shares_since_retarget as f64 / minutes;
        let target_rate = self.config.target_shares_per_minute;

        debug!(
            "Vardiff check: {} shares in {:.1}s = {:.2}/min (target: {:.2}/min)",
            self.shares_since_retarget,
            elapsed.as_secs_f64(),
            actual_rate,
            target_rate
        );

        // Check if we're within tolerance
        let ratio = actual_rate / target_rate;
        let lower_bound = 1.0 - self.config.variance_tolerance;
        let upper_bound = 1.0 + self.config.variance_tolerance;

        if ratio >= lower_bound && ratio <= upper_bound {
            // Within tolerance, no change needed
            self.reset_window();
            return None;
        }

        // Calculate new difficulty
        // If shares are coming too fast (ratio > 1), increase difficulty
        // If shares are coming too slow (ratio < 1), decrease difficulty
        let adjustment = if ratio > 0.0 { ratio } else { 0.5 };
        let new_difficulty = (self.current_difficulty * adjustment).clamp(
            self.config.min_difficulty,
            self.config.max_difficulty,
        );

        // Apply smoothing to avoid large jumps
        let smoothed = self.current_difficulty * 0.5 + new_difficulty * 0.5;
        let final_difficulty = smoothed.clamp(
            self.config.min_difficulty,
            self.config.max_difficulty,
        );

        if (final_difficulty - self.current_difficulty).abs() > 0.01 {
            info!(
                "Vardiff adjustment: {:.2} -> {:.2} (share rate: {:.2}/min)",
                self.current_difficulty, final_difficulty, actual_rate
            );
            self.current_difficulty = final_difficulty;
            self.reset_window();
            return Some(final_difficulty);
        }

        self.reset_window();
        None
    }

    /// Reset the measurement window
    fn reset_window(&mut self) {
        self.shares_since_retarget = 0;
        self.last_retarget = Instant::now();
        self.window_start = Instant::now();
    }

    /// Get statistics about current window
    pub fn stats(&self) -> VardiffStats {
        let elapsed = self.window_start.elapsed();
        let minutes = elapsed.as_secs_f64() / 60.0;
        let rate = if minutes > 0.0 {
            self.shares_since_retarget as f64 / minutes
        } else {
            0.0
        };

        VardiffStats {
            current_difficulty: self.current_difficulty,
            shares_in_window: self.shares_since_retarget,
            window_duration: elapsed,
            current_rate: rate,
            target_rate: self.config.target_shares_per_minute,
        }
    }
}

/// Statistics from vardiff controller
#[derive(Debug, Clone)]
pub struct VardiffStats {
    pub current_difficulty: f64,
    pub shares_in_window: u32,
    pub window_duration: Duration,
    pub current_rate: f64,
    pub target_rate: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = VardiffConfig::default();
        assert!(config.target_shares_per_minute > 0.0);
        assert!(config.min_difficulty > 0.0);
        assert!(config.max_difficulty > config.min_difficulty);
    }

    #[test]
    fn test_difficulty_clamping() {
        let config = VardiffConfig {
            min_difficulty: 10.0,
            max_difficulty: 100.0,
            ..Default::default()
        };
        let mut controller = VardiffController::new(config);

        controller.set_difficulty(5.0);
        assert_eq!(controller.current_difficulty(), 10.0);

        controller.set_difficulty(500.0);
        assert_eq!(controller.current_difficulty(), 100.0);
    }

    #[test]
    fn test_target_generation() {
        let config = VardiffConfig::default();
        let controller = VardiffController::new(config);

        let target = controller.current_target();
        // Target should be non-zero
        assert!(target.0.iter().any(|&b| b != 0));
    }
}
