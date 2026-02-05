//! Security utilities for Stratum V2 protocol
//!
//! Implements mitigations for known attack vectors:
//! - Replay attack protection via sequence validation
//! - Short-lived connection detection (EROSION attack indicator)
//! - Decryption failure tracking
//! - Timing attack mitigation via response jitter

use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;
use std::time::{Duration, Instant};
use tracing::{debug, warn};

// ============================================================================
// Sequence Validation (Replay Protection)
// ============================================================================

/// Sequence validator for replay attack protection
///
/// The Noise NK pattern doesn't provide application-layer replay protection.
/// This module tracks sequence numbers per channel to detect replayed messages.
///
/// ## Attack Context
///
/// Without sequence validation, an attacker who captures encrypted messages
/// could replay them later (though they can't read or modify the contents).
/// This is particularly relevant if an attacker has access to network traffic.
#[derive(Debug)]
pub struct SequenceValidator {
    /// Map of channel_id -> last seen sequence number
    channels: RwLock<HashMap<u32, SequenceState>>,
    /// Maximum gap allowed in sequence numbers (for out-of-order delivery)
    max_gap: u32,
    /// Window size for tracking seen sequences
    window_size: usize,
}

/// Sequence state for a single channel
#[derive(Debug)]
struct SequenceState {
    /// Highest sequence number seen
    highest_seen: u32,
    /// Window of recently seen sequence numbers (for gap handling)
    seen_window: VecDeque<u32>,
    /// Count of sequence anomalies (reordering, duplicates, gaps)
    anomaly_count: u32,
    /// Last update time
    last_update: Instant,
}

impl SequenceState {
    fn new() -> Self {
        Self {
            highest_seen: 0,
            seen_window: VecDeque::with_capacity(128),
            anomaly_count: 0,
            last_update: Instant::now(),
        }
    }
}

/// Result of sequence validation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SequenceCheckResult {
    /// Sequence is valid and in expected order
    Valid,
    /// Sequence is valid but out of order (within acceptable gap)
    ValidOutOfOrder,
    /// Sequence appears to be a replay (duplicate)
    Replay,
    /// Sequence has an unacceptable gap (potential attack or corruption)
    GapTooLarge,
    /// Sequence number went backwards beyond window
    StaleSequence,
}

impl SequenceCheckResult {
    /// Check if the result indicates the message should be processed
    pub fn should_process(&self) -> bool {
        matches!(self, SequenceCheckResult::Valid | SequenceCheckResult::ValidOutOfOrder)
    }

    /// Check if this might indicate an attack
    pub fn is_suspicious(&self) -> bool {
        matches!(
            self,
            SequenceCheckResult::Replay | SequenceCheckResult::GapTooLarge
        )
    }
}

impl Default for SequenceValidator {
    fn default() -> Self {
        Self::new(1000, 128) // Allow gaps up to 1000, track last 128 sequences
    }
}

impl SequenceValidator {
    /// Create a new sequence validator
    pub fn new(max_gap: u32, window_size: usize) -> Self {
        Self {
            channels: RwLock::new(HashMap::new()),
            max_gap,
            window_size,
        }
    }

    /// Validate a sequence number for a channel
    ///
    /// Returns whether the sequence is valid and should be processed.
    pub fn validate(&self, channel_id: u32, sequence: u32) -> SequenceCheckResult {
        let mut channels = self.channels.write().unwrap_or_else(|e| {
            warn!("Sequence validator lock was poisoned in validate, recovering");
            e.into_inner()
        });
        let state = channels
            .entry(channel_id)
            .or_insert_with(SequenceState::new);

        state.last_update = Instant::now();

        // First message for this channel
        if state.highest_seen == 0 && state.seen_window.is_empty() {
            state.highest_seen = sequence;
            state.seen_window.push_back(sequence);
            return SequenceCheckResult::Valid;
        }

        // Check for duplicate (replay)
        if state.seen_window.contains(&sequence) {
            state.anomaly_count += 1;
            debug!(
                "Replay detected: channel {} seq {} (anomalies: {})",
                channel_id, sequence, state.anomaly_count
            );
            return SequenceCheckResult::Replay;
        }

        // Check if sequence is in expected order
        let expected = state.highest_seen.wrapping_add(1);

        if sequence == expected {
            // Perfect order
            state.highest_seen = sequence;
            Self::add_to_window(&mut state.seen_window, sequence, self.window_size);
            return SequenceCheckResult::Valid;
        }

        // Check for acceptable out-of-order
        if sequence > state.highest_seen {
            let gap = sequence.wrapping_sub(state.highest_seen);
            if gap <= self.max_gap {
                state.highest_seen = sequence;
                Self::add_to_window(&mut state.seen_window, sequence, self.window_size);
                state.anomaly_count += 1;
                return SequenceCheckResult::ValidOutOfOrder;
            } else {
                state.anomaly_count += 1;
                warn!(
                    "Large gap detected: channel {} seq {} (expected ~{}), gap={}",
                    channel_id, sequence, expected, gap
                );
                return SequenceCheckResult::GapTooLarge;
            }
        }

        // Sequence is lower than highest seen - check if within window
        let behind = state.highest_seen.wrapping_sub(sequence);
        if behind <= self.window_size as u32 {
            // Within window, could be reordering
            Self::add_to_window(&mut state.seen_window, sequence, self.window_size);
            state.anomaly_count += 1;
            return SequenceCheckResult::ValidOutOfOrder;
        }

        // Too far behind - stale or replay
        state.anomaly_count += 1;
        SequenceCheckResult::StaleSequence
    }

    /// Get anomaly count for a channel
    pub fn anomaly_count(&self, channel_id: u32) -> u32 {
        self.channels
            .read()
            .unwrap_or_else(|e| {
                warn!("Sequence validator lock was poisoned in anomaly_count, recovering");
                e.into_inner()
            })
            .get(&channel_id)
            .map(|s| s.anomaly_count)
            .unwrap_or(0)
    }

    /// Remove channel state (on disconnect)
    pub fn remove_channel(&self, channel_id: u32) {
        self.channels
            .write()
            .unwrap_or_else(|e| {
                warn!("Sequence validator lock was poisoned in remove_channel, recovering");
                e.into_inner()
            })
            .remove(&channel_id);
    }

    /// Clean up stale channel entries
    pub fn cleanup_stale(&self, max_age: Duration) {
        let mut channels = self.channels.write().unwrap_or_else(|e| {
            warn!("Sequence validator lock was poisoned in cleanup_stale, recovering");
            e.into_inner()
        });
        let now = Instant::now();
        channels.retain(|_, state| now.duration_since(state.last_update) < max_age);
    }

    fn add_to_window(window: &mut VecDeque<u32>, seq: u32, max_size: usize) {
        window.push_back(seq);
        while window.len() > max_size {
            window.pop_front();
        }
    }
}

// ============================================================================
// Short-lived Connection Detection
// ============================================================================

/// Tracks connection patterns to detect potential attacks
///
/// ## Attack Context
///
/// The EROSION attack can cause repeated short-lived connections when
/// an attacker corrupts encrypted packets. Tracking connection durations
/// helps detect this attack pattern.
#[derive(Debug)]
pub struct ConnectionTracker {
    /// Recent connections by source address
    connections: RwLock<HashMap<SocketAddr, ConnectionHistory>>,
    /// Threshold for "short-lived" connections
    short_lived_threshold: Duration,
    /// Window for tracking connection patterns
    tracking_window: Duration,
    /// Maximum short-lived connections before flagging
    max_short_lived_per_window: usize,
}

/// Connection history for a single address
#[derive(Debug)]
struct ConnectionHistory {
    /// Recent connection durations
    recent_durations: VecDeque<ConnectionRecord>,
    /// Number of connections flagged as suspicious
    suspicious_count: u32,
    /// Whether this address is currently flagged
    is_flagged: bool,
}

#[derive(Debug, Clone)]
struct ConnectionRecord {
    connected_at: Instant,
    disconnected_at: Instant,
    duration: Duration,
    decryption_error: bool,
}

impl Default for ConnectionTracker {
    fn default() -> Self {
        Self::new(
            Duration::from_secs(5),   // Connections < 5s are "short-lived"
            Duration::from_secs(300), // Track over 5 minute window
            10,                        // Flag after 10 short-lived connections
        )
    }
}

impl ConnectionTracker {
    /// Create a new connection tracker
    pub fn new(
        short_lived_threshold: Duration,
        tracking_window: Duration,
        max_short_lived_per_window: usize,
    ) -> Self {
        Self {
            connections: RwLock::new(HashMap::new()),
            short_lived_threshold,
            tracking_window,
            max_short_lived_per_window,
        }
    }

    /// Record a new connection
    pub fn on_connect(&self, addr: SocketAddr) -> Instant {
        let now = Instant::now();
        let mut connections = self.connections.write().unwrap();
        connections.entry(addr).or_insert_with(|| ConnectionHistory {
            recent_durations: VecDeque::with_capacity(32),
            suspicious_count: 0,
            is_flagged: false,
        });
        now
    }

    /// Record a disconnection
    ///
    /// Returns `true` if this address should be flagged as suspicious
    pub fn on_disconnect(
        &self,
        addr: SocketAddr,
        connected_at: Instant,
        decryption_error: bool,
    ) -> bool {
        let now = Instant::now();
        let duration = now.duration_since(connected_at);
        let is_short_lived = duration < self.short_lived_threshold;

        let mut connections = self.connections.write().unwrap();
        let history = match connections.get_mut(&addr) {
            Some(h) => h,
            None => return false,
        };

        // Record this connection
        history.recent_durations.push_back(ConnectionRecord {
            connected_at,
            disconnected_at: now,
            duration,
            decryption_error,
        });

        // Clean old records
        while let Some(front) = history.recent_durations.front() {
            if now.duration_since(front.disconnected_at) > self.tracking_window {
                history.recent_durations.pop_front();
            } else {
                break;
            }
        }

        // Count short-lived connections
        let short_lived_count = history
            .recent_durations
            .iter()
            .filter(|r| r.duration < self.short_lived_threshold)
            .count();

        // Count decryption errors
        let decrypt_error_count = history
            .recent_durations
            .iter()
            .filter(|r| r.decryption_error)
            .count();

        // Check if we should flag this address
        let should_flag = short_lived_count >= self.max_short_lived_per_window
            || decrypt_error_count >= 3;

        if should_flag && !history.is_flagged {
            history.is_flagged = true;
            history.suspicious_count += 1;
            warn!(
                "Suspicious connection pattern from {}: {} short-lived, {} decrypt errors in {:?}",
                addr, short_lived_count, decrypt_error_count, self.tracking_window
            );
        }

        // Log short-lived connections with decryption errors
        if is_short_lived && decryption_error {
            warn!(
                "Short-lived connection ({:?}) with decryption error from {} - potential EROSION attack",
                duration, addr
            );
        }

        should_flag
    }

    /// Check if an address is flagged as suspicious
    pub fn is_flagged(&self, addr: &SocketAddr) -> bool {
        self.connections
            .read()
            .unwrap()
            .get(addr)
            .map(|h| h.is_flagged)
            .unwrap_or(false)
    }

    /// Clear flag for an address (manual reset)
    pub fn clear_flag(&self, addr: &SocketAddr) {
        if let Some(history) = self.connections.write().unwrap().get_mut(addr) {
            history.is_flagged = false;
        }
    }

    /// Get statistics for an address
    pub fn get_stats(&self, addr: &SocketAddr) -> Option<ConnectionStats> {
        let connections = self.connections.read().unwrap();
        let history = connections.get(addr)?;

        let total = history.recent_durations.len();
        let short_lived = history
            .recent_durations
            .iter()
            .filter(|r| r.duration < self.short_lived_threshold)
            .count();
        let decrypt_errors = history
            .recent_durations
            .iter()
            .filter(|r| r.decryption_error)
            .count();
        let avg_duration = if total > 0 {
            let sum: Duration = history.recent_durations.iter().map(|r| r.duration).sum();
            sum / total as u32
        } else {
            Duration::ZERO
        };

        Some(ConnectionStats {
            total_connections: total,
            short_lived_connections: short_lived,
            decryption_errors: decrypt_errors,
            average_duration: avg_duration,
            is_flagged: history.is_flagged,
            suspicious_count: history.suspicious_count,
        })
    }

    /// Cleanup old entries
    pub fn cleanup(&self, max_age: Duration) {
        let mut connections = self.connections.write().unwrap();
        let now = Instant::now();

        connections.retain(|_, history| {
            history
                .recent_durations
                .back()
                .map(|r| now.duration_since(r.disconnected_at) < max_age)
                .unwrap_or(false)
        });
    }
}

/// Statistics about an address's connection history
#[derive(Debug, Clone)]
pub struct ConnectionStats {
    /// Total connections in tracking window
    pub total_connections: usize,
    /// Short-lived connections
    pub short_lived_connections: usize,
    /// Connections that ended with decryption errors
    pub decryption_errors: usize,
    /// Average connection duration
    pub average_duration: Duration,
    /// Whether the address is currently flagged
    pub is_flagged: bool,
    /// Number of times this address has been flagged
    pub suspicious_count: u32,
}

// ============================================================================
// Response Timing Jitter
// ============================================================================

/// Provides timing jitter for responses to mitigate timing attacks
///
/// ## Attack Context
///
/// The StraTap and ISP Log attacks can infer miner earnings by analyzing
/// timing patterns. Adding random delays to responses makes this harder.
#[derive(Debug)]
pub struct TimingJitter {
    /// Minimum delay to add
    min_delay: Duration,
    /// Maximum delay to add
    max_delay: Duration,
}

impl Default for TimingJitter {
    fn default() -> Self {
        Self::new(Duration::from_millis(0), Duration::from_millis(50))
    }
}

impl TimingJitter {
    /// Create a new timing jitter generator
    pub fn new(min_delay: Duration, max_delay: Duration) -> Self {
        Self {
            min_delay,
            max_delay,
        }
    }

    /// Get a jittered delay
    ///
    /// Uses cryptographically random values so that an observer cannot
    /// predict future jitter from past observations.
    pub fn get_delay(&self) -> Duration {
        if self.max_delay <= self.min_delay {
            return self.min_delay;
        }

        let rand: f64 = rand::random();

        let range = self.max_delay - self.min_delay;
        self.min_delay + range.mul_f64(rand)
    }

    /// Apply jitter delay asynchronously
    pub async fn apply(&self) {
        let delay = self.get_delay();
        if delay > Duration::ZERO {
            tokio::time::sleep(delay).await;
        }
    }
}

// ============================================================================
// Security Metrics Collection
// ============================================================================

/// Collects security-related metrics for monitoring
#[derive(Debug, Default)]
pub struct SecurityMetrics {
    /// Total decryption failures
    pub decryption_failures: AtomicU64,
    /// Total replay attempts detected
    pub replay_attempts: AtomicU64,
    /// Total sequence anomalies
    pub sequence_anomalies: AtomicU64,
    /// Total flagged addresses
    pub flagged_addresses: AtomicU64,
    /// Total short-lived connections
    pub short_lived_connections: AtomicU64,
}

impl SecurityMetrics {
    /// Create new security metrics
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a decryption failure
    pub fn record_decryption_failure(&self) {
        self.decryption_failures.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a replay attempt
    pub fn record_replay_attempt(&self) {
        self.replay_attempts.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a sequence anomaly
    pub fn record_sequence_anomaly(&self) {
        self.sequence_anomalies.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a flagged address
    pub fn record_flagged_address(&self) {
        self.flagged_addresses.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a short-lived connection
    pub fn record_short_lived_connection(&self) {
        self.short_lived_connections.fetch_add(1, Ordering::Relaxed);
    }

    /// Get current metrics snapshot
    pub fn snapshot(&self) -> SecurityMetricsSnapshot {
        SecurityMetricsSnapshot {
            decryption_failures: self.decryption_failures.load(Ordering::Relaxed),
            replay_attempts: self.replay_attempts.load(Ordering::Relaxed),
            sequence_anomalies: self.sequence_anomalies.load(Ordering::Relaxed),
            flagged_addresses: self.flagged_addresses.load(Ordering::Relaxed),
            short_lived_connections: self.short_lived_connections.load(Ordering::Relaxed),
        }
    }
}

/// Snapshot of security metrics
#[derive(Debug, Clone)]
pub struct SecurityMetricsSnapshot {
    pub decryption_failures: u64,
    pub replay_attempts: u64,
    pub sequence_anomalies: u64,
    pub flagged_addresses: u64,
    pub short_lived_connections: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========== Sequence Validator Tests ==========

    #[test]
    fn test_sequence_first_message() {
        let validator = SequenceValidator::default();
        assert_eq!(
            validator.validate(1, 1),
            SequenceCheckResult::Valid
        );
    }

    #[test]
    fn test_sequence_in_order() {
        let validator = SequenceValidator::default();
        validator.validate(1, 1);
        assert_eq!(validator.validate(1, 2), SequenceCheckResult::Valid);
        assert_eq!(validator.validate(1, 3), SequenceCheckResult::Valid);
    }

    #[test]
    fn test_sequence_replay_detection() {
        let validator = SequenceValidator::default();
        validator.validate(1, 1);
        validator.validate(1, 2);
        assert_eq!(validator.validate(1, 2), SequenceCheckResult::Replay);
    }

    #[test]
    fn test_sequence_out_of_order() {
        let validator = SequenceValidator::new(100, 64);
        validator.validate(1, 1);
        validator.validate(1, 2);
        validator.validate(1, 5); // Skip 3, 4
        assert_eq!(
            validator.validate(1, 3),
            SequenceCheckResult::ValidOutOfOrder
        );
    }

    #[test]
    fn test_sequence_large_gap() {
        let validator = SequenceValidator::new(10, 64);
        validator.validate(1, 1);
        assert_eq!(
            validator.validate(1, 100),
            SequenceCheckResult::GapTooLarge
        );
    }

    #[test]
    fn test_sequence_multiple_channels() {
        let validator = SequenceValidator::default();
        validator.validate(1, 10);
        validator.validate(2, 20);
        assert_eq!(validator.validate(1, 11), SequenceCheckResult::Valid);
        assert_eq!(validator.validate(2, 21), SequenceCheckResult::Valid);
    }

    // ========== Connection Tracker Tests ==========

    #[test]
    fn test_connection_tracker_normal() {
        let tracker = ConnectionTracker::new(
            Duration::from_secs(5),
            Duration::from_secs(300),
            10,
        );

        let addr: SocketAddr = "127.0.0.1:12345".parse().unwrap();
        let connected_at = tracker.on_connect(addr);

        // Simulate a normal connection duration
        std::thread::sleep(Duration::from_millis(10));

        // This shouldn't flag the address (single connection)
        assert!(!tracker.on_disconnect(addr, connected_at, false));
        assert!(!tracker.is_flagged(&addr));
    }

    #[test]
    fn test_connection_tracker_short_lived_flagging() {
        let tracker = ConnectionTracker::new(
            Duration::from_millis(100), // Very short threshold for testing
            Duration::from_secs(300),
            3, // Flag after just 3 short-lived connections
        );

        let addr: SocketAddr = "127.0.0.1:12345".parse().unwrap();

        // Simulate multiple short-lived connections
        for _ in 0..3 {
            let connected_at = tracker.on_connect(addr);
            tracker.on_disconnect(addr, connected_at, false);
        }

        assert!(tracker.is_flagged(&addr));
    }

    #[test]
    fn test_connection_stats() {
        let tracker = ConnectionTracker::default();
        let addr: SocketAddr = "127.0.0.1:12345".parse().unwrap();

        let connected_at = tracker.on_connect(addr);
        tracker.on_disconnect(addr, connected_at, true);

        let stats = tracker.get_stats(&addr).unwrap();
        assert_eq!(stats.total_connections, 1);
        assert_eq!(stats.decryption_errors, 1);
    }

    // ========== Timing Jitter Tests ==========

    #[test]
    fn test_timing_jitter_range() {
        let jitter = TimingJitter::new(
            Duration::from_millis(10),
            Duration::from_millis(100),
        );

        for _ in 0..100 {
            let delay = jitter.get_delay();
            assert!(delay >= Duration::from_millis(10));
            assert!(delay <= Duration::from_millis(100));
        }
    }

    #[test]
    fn test_timing_jitter_zero() {
        let jitter = TimingJitter::new(Duration::ZERO, Duration::ZERO);
        assert_eq!(jitter.get_delay(), Duration::ZERO);
    }

    // ========== Security Metrics Tests ==========

    #[test]
    fn test_security_metrics() {
        let metrics = SecurityMetrics::new();

        metrics.record_decryption_failure();
        metrics.record_decryption_failure();
        metrics.record_replay_attempt();

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.decryption_failures, 2);
        assert_eq!(snapshot.replay_attempts, 1);
    }
}
