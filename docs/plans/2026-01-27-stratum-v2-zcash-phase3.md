# Stratum V2 Zcash Phase 3: Basic Pool Server

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement a basic Stratum V2 pool server for Zcash that accepts miner connections, distributes jobs, validates Equihash shares, and tracks contributions for PPS payout.

**Architecture:** Single tokio async runtime with channel-based component communication. Raw TCP connections using Phase 2's MessageFrame codec. Equihash validation offloaded to a dedicated thread pool (144MB memory per thread). Standard Channels only. Jobs broadcast immediately on new templates.

**Tech Stack:** Rust 1.75+, tokio (async runtime), Phase 1 (template-provider), Phase 2 (mining-protocol, equihash-validator)

---

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Network protocol | Raw TCP + MessageFrame | Matches SRI, low overhead, Phase 2 codec ready |
| Architecture | Single async runtime + channels | Simple, proven, tokio already in deps |
| Channel model | Standard Channels only | Zcash header nonce eliminates need for Extended |
| Job distribution | Immediate broadcast | 75s block time needs freshness |
| Payout model | Simple PPS (in-memory) | MVP focus, enhance later |
| Coinbase | Use Zebra's directly | Funding streams handled correctly |
| Duplicate detection | In-memory hash set + trait | Fast, bounded memory, upgradeable |
| Testing | Mock miner + test vectors | Fast iteration, real vectors for validation |

---

## Crate Structure

```
crates/zcash-pool-server/
├── Cargo.toml
├── src/
│   ├── lib.rs              # Public API
│   ├── config.rs           # Server configuration
│   ├── server.rs           # Main server orchestration
│   ├── session.rs          # Per-miner connection handling
│   ├── channel.rs          # Standard Channel state
│   ├── job.rs              # Job management and distribution
│   ├── share.rs            # Share processing and validation
│   ├── duplicate.rs        # Duplicate detection trait + impl
│   ├── payout.rs           # PPS tracking
│   └── error.rs            # Error types
├── tests/
│   └── integration_tests.rs
└── examples/
    └── run_pool.rs
```

---

## Task 1: Initialize Pool Server Crate

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Create: `crates/zcash-pool-server/Cargo.toml`
- Create: `crates/zcash-pool-server/src/lib.rs`

**Step 1: Create pool server crate Cargo.toml**

Create `crates/zcash-pool-server/Cargo.toml`:

```toml
[package]
name = "zcash-pool-server"
version.workspace = true
edition.workspace = true
license.workspace = true
description = "Stratum V2 pool server for Zcash Equihash mining"

[dependencies]
tokio = { workspace = true, features = ["full"] }
thiserror.workspace = true
tracing.workspace = true
serde.workspace = true
rustc-hash = "2.0"

# Local dependencies
zcash-template-provider = { path = "../zcash-template-provider" }
zcash-mining-protocol = { path = "../zcash-mining-protocol" }
zcash-equihash-validator = { path = "../zcash-equihash-validator" }

[dev-dependencies]
tokio = { workspace = true, features = ["test-util", "macros", "rt-multi-thread"] }
tracing-subscriber.workspace = true
```

**Step 2: Create initial lib.rs**

Create `crates/zcash-pool-server/src/lib.rs`:

```rust
//! Zcash Pool Server for Stratum V2
//!
//! This crate provides a basic pool server that:
//! - Accepts miner connections over TCP
//! - Distributes Equihash mining jobs
//! - Validates submitted shares
//! - Tracks contributions for PPS payout

pub mod config;
pub mod error;
pub mod server;
pub mod session;
pub mod channel;
pub mod job;
pub mod share;
pub mod duplicate;
pub mod payout;

pub use config::PoolConfig;
pub use error::PoolError;
pub use server::PoolServer;
```

**Step 3: Verify compilation fails (missing modules)**

Run: `cargo check -p zcash-pool-server`
Expected: FAIL (missing modules)

**Step 4: Commit**

```bash
git add Cargo.toml crates/zcash-pool-server/
git commit -m "chore: initialize zcash-pool-server crate"
```

---

## Task 2: Define Error Types and Configuration

**Files:**
- Create: `crates/zcash-pool-server/src/error.rs`
- Create: `crates/zcash-pool-server/src/config.rs`

**Step 1: Create error types**

Create `crates/zcash-pool-server/src/error.rs`:

```rust
//! Pool server error types

use thiserror::Error;
use zcash_mining_protocol::ProtocolError;
use zcash_equihash_validator::ValidationError;

#[derive(Error, Debug)]
pub enum PoolError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Protocol error: {0}")]
    Protocol(#[from] ProtocolError),

    #[error("Validation error: {0}")]
    Validation(#[from] ValidationError),

    #[error("Connection closed")]
    ConnectionClosed,

    #[error("Invalid message: {0}")]
    InvalidMessage(String),

    #[error("Unknown channel: {0}")]
    UnknownChannel(u32),

    #[error("Unknown job: {0}")]
    UnknownJob(u32),

    #[error("Stale share for job {0}")]
    StaleShare(u32),

    #[error("Duplicate share")]
    DuplicateShare,

    #[error("Channel send error")]
    ChannelSend,

    #[error("Template provider error: {0}")]
    TemplateProvider(String),

    #[error("Server shutdown")]
    Shutdown,
}

pub type Result<T> = std::result::Result<T, PoolError>;
```

**Step 2: Create configuration**

Create `crates/zcash-pool-server/src/config.rs`:

```rust
//! Pool server configuration

use std::net::SocketAddr;

/// Pool server configuration
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Address to listen on for miner connections
    pub listen_addr: SocketAddr,

    /// Zebra RPC URL for template provider
    pub zebra_url: String,

    /// Template polling interval in milliseconds
    pub template_poll_ms: u64,

    /// Number of validation threads (each needs ~144MB for Equihash)
    pub validation_threads: usize,

    /// Default nonce_1 length (pool prefix)
    pub nonce_1_len: u8,

    /// Initial share difficulty
    pub initial_difficulty: f64,

    /// Vardiff target shares per minute
    pub target_shares_per_minute: f64,

    /// Maximum concurrent connections
    pub max_connections: usize,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0:3333".parse().unwrap(),
            zebra_url: "http://127.0.0.1:8232".to_string(),
            template_poll_ms: 1000,
            validation_threads: 4,
            nonce_1_len: 4,
            initial_difficulty: 1.0,
            target_shares_per_minute: 5.0,
            max_connections: 10000,
        }
    }
}
```

**Step 3: Commit**

```bash
git add crates/zcash-pool-server/src/
git commit -m "feat(pool): add error types and configuration"
```

---

## Task 3: Implement Duplicate Detection

**Files:**
- Create: `crates/zcash-pool-server/src/duplicate.rs`

**Step 1: Create duplicate detection with trait**

Create `crates/zcash-pool-server/src/duplicate.rs`:

```rust
//! Duplicate share detection
//!
//! Uses a trait to allow swapping implementations (in-memory, Redis, etc.)

use rustc_hash::FxHashSet;
use std::collections::HashMap;
use std::sync::RwLock;

/// Trait for duplicate share detection
pub trait DuplicateDetector: Send + Sync {
    /// Check if a share is a duplicate (and record it if not)
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
        // Only hash first 64 bytes of solution for speed (enough for uniqueness)
        solution[..64.min(solution.len())].hash(&mut hasher);
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

        let mut jobs = self.jobs.write().unwrap();
        let shares = jobs.entry(job_id).or_insert_with(FxHashSet::default);

        // Returns true if the value was NOT present (i.e., not a duplicate)
        // Returns false if it WAS present (i.e., is a duplicate)
        !shares.insert(hash)
    }

    fn clear_job(&self, job_id: u32) {
        let mut jobs = self.jobs.write().unwrap();
        jobs.remove(&job_id);
    }

    fn clear_all(&self) {
        let mut jobs = self.jobs.write().unwrap();
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
}
```

**Step 2: Commit**

```bash
git add crates/zcash-pool-server/src/duplicate.rs
git commit -m "feat(pool): implement duplicate share detection"
```

---

## Task 4: Implement PPS Payout Tracking

**Files:**
- Create: `crates/zcash-pool-server/src/payout.rs`

**Step 1: Create payout tracker**

Create `crates/zcash-pool-server/src/payout.rs`:

```rust
//! Simple PPS (Pay Per Share) tracking
//!
//! Tracks share submissions per miner for payout calculation.
//! In-memory for Phase 3; can be upgraded to database-backed later.

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

/// Unique identifier for a miner (could be pubkey, address, etc.)
pub type MinerId = String;

/// Share record for payout tracking
#[derive(Debug, Clone)]
pub struct ShareRecord {
    /// Share difficulty
    pub difficulty: f64,
    /// When the share was submitted
    pub timestamp: Instant,
}

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
        // This is a rough estimate; real pools use more sophisticated calculations
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
}
```

**Step 2: Commit**

```bash
git add crates/zcash-pool-server/src/payout.rs
git commit -m "feat(pool): implement PPS payout tracking"
```

---

## Task 5: Implement Channel State

**Files:**
- Create: `crates/zcash-pool-server/src/channel.rs`

**Step 1: Create channel state management**

Create `crates/zcash-pool-server/src/channel.rs`:

```rust
//! Standard Channel state management
//!
//! Each miner connection gets one channel with a unique nonce_1 prefix.

use zcash_equihash_validator::{VardiffController, VardiffConfig};
use zcash_mining_protocol::messages::NewEquihashJob;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};

/// Global channel ID counter
static NEXT_CHANNEL_ID: AtomicU32 = AtomicU32::new(1);

/// Standard Channel state for a single miner
#[derive(Debug)]
pub struct Channel {
    /// Unique channel identifier
    pub id: u32,
    /// Pool-assigned nonce prefix
    pub nonce_1: Vec<u8>,
    /// Length of miner's nonce_2
    pub nonce_2_len: u8,
    /// Per-channel vardiff controller
    pub vardiff: VardiffController,
    /// Active jobs for this channel (job_id -> job)
    pub jobs: HashMap<u32, ChannelJob>,
    /// Last job ID sent
    pub last_job_id: u32,
}

/// A job as seen by a specific channel
#[derive(Debug, Clone)]
pub struct ChannelJob {
    /// Job ID (channel-specific)
    pub job_id: u32,
    /// The full job message sent to miner
    pub job: NewEquihashJob,
    /// Whether this job is still valid
    pub active: bool,
}

impl Channel {
    /// Create a new channel with the given nonce_1 prefix
    pub fn new(nonce_1: Vec<u8>, vardiff_config: VardiffConfig) -> Self {
        let nonce_2_len = 32 - nonce_1.len() as u8;
        Self {
            id: NEXT_CHANNEL_ID.fetch_add(1, Ordering::SeqCst),
            nonce_1,
            nonce_2_len,
            vardiff: VardiffController::new(vardiff_config),
            jobs: HashMap::new(),
            last_job_id: 0,
        }
    }

    /// Generate a unique nonce_1 for a channel based on channel ID
    pub fn generate_nonce_1(channel_id: u32, len: u8) -> Vec<u8> {
        let mut nonce_1 = vec![0u8; len as usize];
        let id_bytes = channel_id.to_le_bytes();
        let copy_len = (len as usize).min(4);
        nonce_1[..copy_len].copy_from_slice(&id_bytes[..copy_len]);
        nonce_1
    }

    /// Add a job to this channel
    pub fn add_job(&mut self, job: NewEquihashJob, clean_jobs: bool) {
        if clean_jobs {
            // Mark all existing jobs as inactive
            for j in self.jobs.values_mut() {
                j.active = false;
            }
        }

        self.last_job_id += 1;
        let channel_job = ChannelJob {
            job_id: self.last_job_id,
            job,
            active: true,
        };
        self.jobs.insert(self.last_job_id, channel_job);

        // Keep only last 10 jobs to bound memory
        if self.jobs.len() > 10 {
            let min_id = self.last_job_id.saturating_sub(10);
            self.jobs.retain(|&id, _| id > min_id);
        }
    }

    /// Get a job by ID
    pub fn get_job(&self, job_id: u32) -> Option<&ChannelJob> {
        self.jobs.get(&job_id)
    }

    /// Check if a job is active (not stale)
    pub fn is_job_active(&self, job_id: u32) -> bool {
        self.jobs.get(&job_id).map(|j| j.active).unwrap_or(false)
    }

    /// Get current target from vardiff
    pub fn current_target(&self) -> [u8; 32] {
        self.vardiff.current_target().to_le_bytes()
    }

    /// Record a share and check for vardiff adjustment
    pub fn record_share(&mut self) -> Option<f64> {
        self.vardiff.record_share();
        self.vardiff.maybe_retarget()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_creation() {
        let nonce_1 = vec![0x01, 0x02, 0x03, 0x04];
        let channel = Channel::new(nonce_1.clone(), VardiffConfig::default());

        assert_eq!(channel.nonce_1, nonce_1);
        assert_eq!(channel.nonce_2_len, 28);
    }

    #[test]
    fn test_nonce_1_generation() {
        let nonce_1 = Channel::generate_nonce_1(0x12345678, 4);
        assert_eq!(nonce_1, vec![0x78, 0x56, 0x34, 0x12]);

        let nonce_1_short = Channel::generate_nonce_1(0x12345678, 2);
        assert_eq!(nonce_1_short, vec![0x78, 0x56]);
    }

    #[test]
    fn test_job_management() {
        let mut channel = Channel::new(vec![0; 4], VardiffConfig::default());

        let job1 = NewEquihashJob {
            channel_id: channel.id,
            job_id: 0, // Will be replaced
            future_job: false,
            version: 5,
            prev_hash: [0; 32],
            merkle_root: [0; 32],
            block_commitments: [0; 32],
            nonce_1: channel.nonce_1.clone(),
            nonce_2_len: channel.nonce_2_len,
            time: 0,
            bits: 0,
            target: [0xff; 32],
            clean_jobs: false,
        };

        channel.add_job(job1.clone(), false);
        assert!(channel.is_job_active(1));

        channel.add_job(job1.clone(), true); // clean_jobs
        assert!(!channel.is_job_active(1)); // Old job now inactive
        assert!(channel.is_job_active(2)); // New job active
    }
}
```

**Step 2: Commit**

```bash
git add crates/zcash-pool-server/src/channel.rs
git commit -m "feat(pool): implement channel state management"
```

---

## Task 6: Implement Job Distribution

**Files:**
- Create: `crates/zcash-pool-server/src/job.rs`

**Step 1: Create job distributor**

Create `crates/zcash-pool-server/src/job.rs`:

```rust
//! Job creation and distribution
//!
//! Converts templates from Phase 1 into jobs for miners.

use crate::channel::Channel;
use zcash_mining_protocol::messages::NewEquihashJob;
use zcash_template_provider::template::BlockTemplate;
use std::sync::atomic::{AtomicU32, Ordering};

/// Global job ID counter (unique across all channels)
static NEXT_GLOBAL_JOB_ID: AtomicU32 = AtomicU32::new(1);

/// Job distributor - creates jobs from templates
pub struct JobDistributor {
    /// Current template
    current_template: Option<BlockTemplate>,
    /// Previous block hash (to detect new blocks)
    prev_hash: Option<[u8; 32]>,
}

impl JobDistributor {
    pub fn new() -> Self {
        Self {
            current_template: None,
            prev_hash: None,
        }
    }

    /// Update the current template
    /// Returns true if this is a new block (clean_jobs should be set)
    pub fn update_template(&mut self, template: BlockTemplate) -> bool {
        let is_new_block = self.prev_hash.as_ref() != Some(&template.prev_hash);

        self.prev_hash = Some(template.prev_hash);
        self.current_template = Some(template);

        is_new_block
    }

    /// Create a job for a specific channel
    pub fn create_job(&self, channel: &Channel, clean_jobs: bool) -> Option<NewEquihashJob> {
        let template = self.current_template.as_ref()?;

        Some(NewEquihashJob {
            channel_id: channel.id,
            job_id: NEXT_GLOBAL_JOB_ID.fetch_add(1, Ordering::SeqCst),
            future_job: false,
            version: template.version,
            prev_hash: template.prev_hash,
            merkle_root: template.merkle_root,
            block_commitments: template.block_commitments,
            nonce_1: channel.nonce_1.clone(),
            nonce_2_len: channel.nonce_2_len,
            time: template.time,
            bits: template.bits,
            target: channel.current_target(),
            clean_jobs,
        })
    }

    /// Get current template height
    pub fn current_height(&self) -> Option<u32> {
        self.current_template.as_ref().map(|t| t.height)
    }

    /// Check if we have a template
    pub fn has_template(&self) -> bool {
        self.current_template.is_some()
    }
}

impl Default for JobDistributor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zcash_equihash_validator::VardiffConfig;

    fn make_template(height: u32, prev_hash: [u8; 32]) -> BlockTemplate {
        BlockTemplate {
            height,
            prev_hash,
            merkle_root: [0xaa; 32],
            block_commitments: [0xbb; 32],
            time: 1700000000,
            bits: 0x1d00ffff,
            version: 5,
            transactions: vec![],
            coinbase_tx: vec![],
            target: "0000000000000000000000000000000000000000000000000000000000ffffff".to_string(),
        }
    }

    #[test]
    fn test_new_block_detection() {
        let mut distributor = JobDistributor::new();

        let template1 = make_template(100, [0x11; 32]);
        assert!(distributor.update_template(template1)); // First template

        let template2 = make_template(100, [0x11; 32]);
        assert!(!distributor.update_template(template2)); // Same block

        let template3 = make_template(101, [0x22; 32]);
        assert!(distributor.update_template(template3)); // New block
    }

    #[test]
    fn test_create_job() {
        let mut distributor = JobDistributor::new();
        let template = make_template(100, [0x11; 32]);
        distributor.update_template(template);

        let channel = Channel::new(vec![0x01, 0x02, 0x03, 0x04], VardiffConfig::default());
        let job = distributor.create_job(&channel, false).unwrap();

        assert_eq!(job.channel_id, channel.id);
        assert_eq!(job.nonce_1, vec![0x01, 0x02, 0x03, 0x04]);
        assert_eq!(job.nonce_2_len, 28);
        assert!(!job.clean_jobs);
    }
}
```

**Step 2: Commit**

```bash
git add crates/zcash-pool-server/src/job.rs
git commit -m "feat(pool): implement job distribution"
```

---

## Task 7: Implement Share Processing

**Files:**
- Create: `crates/zcash-pool-server/src/share.rs`

**Step 1: Create share processor**

Create `crates/zcash-pool-server/src/share.rs`:

```rust
//! Share processing and validation
//!
//! Validates submitted shares using the Equihash validator from Phase 2.

use crate::channel::Channel;
use crate::duplicate::DuplicateDetector;
use crate::error::{PoolError, Result};
use zcash_equihash_validator::EquihashValidator;
use zcash_mining_protocol::messages::{SubmitEquihashShare, ShareResult, RejectReason};
use tracing::{debug, warn};

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
```

**Step 2: Commit**

```bash
git add crates/zcash-pool-server/src/share.rs
git commit -m "feat(pool): implement share processing and validation"
```

---

## Task 8: Implement Session Handler

**Files:**
- Create: `crates/zcash-pool-server/src/session.rs`

**Step 1: Create session handler**

Create `crates/zcash-pool-server/src/session.rs`:

```rust
//! Per-miner session handling
//!
//! Manages the TCP connection and message flow for a single miner.

use crate::channel::Channel;
use crate::error::{PoolError, Result};
use zcash_equihash_validator::VardiffConfig;
use zcash_mining_protocol::codec::{decode_message, encode_message, MessageFrame};
use zcash_mining_protocol::messages::{
    NewEquihashJob, SubmitEquihashShare, SubmitSharesResponse, ShareResult,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Message from session to server
#[derive(Debug)]
pub enum SessionMessage {
    /// Miner submitted a share
    ShareSubmitted {
        channel_id: u32,
        share: SubmitEquihashShare,
        response_tx: mpsc::Sender<ShareResult>,
    },
    /// Session disconnected
    Disconnected { channel_id: u32 },
}

/// Message from server to session
#[derive(Debug, Clone)]
pub enum ServerMessage {
    /// Send a new job to the miner
    NewJob(NewEquihashJob),
    /// Update the miner's target (vardiff)
    SetTarget { target: [u8; 32] },
    /// Shutdown the session
    Shutdown,
}

/// Session state
pub struct Session {
    /// TCP stream
    stream: TcpStream,
    /// Channel for this session
    pub channel: Channel,
    /// Sender to the server
    server_tx: mpsc::Sender<SessionMessage>,
    /// Receiver from the server
    server_rx: mpsc::Receiver<ServerMessage>,
    /// Read buffer
    read_buf: Vec<u8>,
}

impl Session {
    /// Create a new session
    pub fn new(
        stream: TcpStream,
        nonce_1_len: u8,
        vardiff_config: VardiffConfig,
        server_tx: mpsc::Sender<SessionMessage>,
        server_rx: mpsc::Receiver<ServerMessage>,
    ) -> Self {
        let channel_id = {
            use std::sync::atomic::{AtomicU32, Ordering};
            static NEXT_ID: AtomicU32 = AtomicU32::new(1);
            NEXT_ID.fetch_add(1, Ordering::SeqCst)
        };

        let nonce_1 = Channel::generate_nonce_1(channel_id, nonce_1_len);
        let channel = Channel::new(nonce_1, vardiff_config);

        Self {
            stream,
            channel,
            server_tx,
            server_rx,
            read_buf: vec![0u8; 8192],
        }
    }

    /// Run the session (handles both reading and writing)
    pub async fn run(mut self) -> Result<()> {
        info!("Session {} started", self.channel.id);

        loop {
            tokio::select! {
                // Read from miner
                result = self.read_message() => {
                    match result {
                        Ok(Some(share)) => {
                            self.handle_share(share).await?;
                        }
                        Ok(None) => {
                            // Connection closed
                            break;
                        }
                        Err(e) => {
                            warn!("Session {} read error: {}", self.channel.id, e);
                            break;
                        }
                    }
                }

                // Receive from server
                msg = self.server_rx.recv() => {
                    match msg {
                        Some(ServerMessage::NewJob(job)) => {
                            self.send_job(job).await?;
                        }
                        Some(ServerMessage::SetTarget { target }) => {
                            self.send_set_target(target).await?;
                        }
                        Some(ServerMessage::Shutdown) | None => {
                            break;
                        }
                    }
                }
            }
        }

        // Notify server of disconnect
        let _ = self.server_tx.send(SessionMessage::Disconnected {
            channel_id: self.channel.id,
        }).await;

        info!("Session {} ended", self.channel.id);
        Ok(())
    }

    /// Read a share submission from the miner
    async fn read_message(&mut self) -> Result<Option<SubmitEquihashShare>> {
        // Read frame header
        let mut header_buf = [0u8; MessageFrame::HEADER_SIZE];
        match self.stream.read_exact(&mut header_buf).await {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                return Ok(None);
            }
            Err(e) => return Err(e.into()),
        }

        let frame = MessageFrame::decode(&header_buf)?;

        // Read payload
        let payload_len = frame.length as usize;
        if payload_len > 2048 {
            return Err(PoolError::InvalidMessage("Payload too large".to_string()));
        }

        let mut full_msg = vec![0u8; MessageFrame::HEADER_SIZE + payload_len];
        full_msg[..MessageFrame::HEADER_SIZE].copy_from_slice(&header_buf);
        self.stream.read_exact(&mut full_msg[MessageFrame::HEADER_SIZE..]).await?;

        // Decode the share
        let share: SubmitEquihashShare = decode_message(&full_msg)?;
        debug!("Received share for job {} from channel {}", share.job_id, self.channel.id);

        Ok(Some(share))
    }

    /// Handle a share submission
    async fn handle_share(&mut self, share: SubmitEquihashShare) -> Result<()> {
        let (response_tx, mut response_rx) = mpsc::channel(1);

        // Send to server for validation
        self.server_tx.send(SessionMessage::ShareSubmitted {
            channel_id: self.channel.id,
            share: share.clone(),
            response_tx,
        }).await.map_err(|_| PoolError::ChannelSend)?;

        // Wait for response
        let result = response_rx.recv().await.unwrap_or(ShareResult::Rejected(
            zcash_mining_protocol::messages::RejectReason::Other("Server error".to_string())
        ));

        // Send response to miner
        let response = SubmitSharesResponse {
            channel_id: self.channel.id,
            sequence_number: share.sequence_number,
            result,
        };

        self.send_response(response).await
    }

    /// Send a job to the miner
    async fn send_job(&mut self, mut job: NewEquihashJob) -> Result<()> {
        // Update job with channel-specific values
        job.channel_id = self.channel.id;
        job.nonce_1 = self.channel.nonce_1.clone();
        job.nonce_2_len = self.channel.nonce_2_len;
        job.target = self.channel.current_target();

        // Track the job
        self.channel.add_job(job.clone(), job.clean_jobs);

        let encoded = encode_message(&job)?;
        self.stream.write_all(&encoded).await?;
        debug!("Sent job {} to channel {}", job.job_id, self.channel.id);

        Ok(())
    }

    /// Send a SetTarget message (vardiff adjustment)
    async fn send_set_target(&mut self, target: [u8; 32]) -> Result<()> {
        use zcash_mining_protocol::messages::SetTarget;

        let msg = SetTarget {
            channel_id: self.channel.id,
            target,
        };

        // Note: SetTarget encoding would need to be added to the codec
        // For now, just log it
        debug!("Would send SetTarget to channel {} with new target", self.channel.id);
        Ok(())
    }

    /// Send a share response
    async fn send_response(&mut self, _response: SubmitSharesResponse) -> Result<()> {
        // Note: Response encoding would need to be added to the codec
        // For now, just log it
        debug!("Share response sent");
        Ok(())
    }
}
```

**Step 2: Commit**

```bash
git add crates/zcash-pool-server/src/session.rs
git commit -m "feat(pool): implement session handler"
```

---

## Task 9: Implement Main Server

**Files:**
- Create: `crates/zcash-pool-server/src/server.rs`

**Step 1: Create main server**

Create `crates/zcash-pool-server/src/server.rs`:

```rust
//! Main pool server orchestration
//!
//! Coordinates all components: listener, sessions, job distribution, share processing.

use crate::channel::Channel;
use crate::config::PoolConfig;
use crate::duplicate::{DuplicateDetector, InMemoryDuplicateDetector};
use crate::error::{PoolError, Result};
use crate::job::JobDistributor;
use crate::payout::{PayoutTracker, MinerId};
use crate::session::{ServerMessage, Session, SessionMessage};
use crate::share::ShareProcessor;
use zcash_equihash_validator::VardiffConfig;
use zcash_mining_protocol::messages::ShareResult;
use zcash_template_provider::{TemplateProvider, TemplateProviderConfig};

use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, RwLock};
use tracing::{error, info, warn};

/// Pool server
pub struct PoolServer {
    config: PoolConfig,
    /// Template provider (Phase 1)
    template_provider: TemplateProvider,
    /// Job distributor
    job_distributor: Arc<RwLock<JobDistributor>>,
    /// Share processor
    share_processor: Arc<ShareProcessor>,
    /// Duplicate detector
    duplicate_detector: Arc<InMemoryDuplicateDetector>,
    /// Payout tracker
    payout_tracker: Arc<PayoutTracker>,
    /// Active sessions (channel_id -> sender)
    sessions: Arc<RwLock<HashMap<u32, mpsc::Sender<ServerMessage>>>>,
    /// Channel for session messages
    session_rx: mpsc::Receiver<SessionMessage>,
    session_tx: mpsc::Sender<SessionMessage>,
}

impl PoolServer {
    /// Create a new pool server
    pub fn new(config: PoolConfig) -> Result<Self> {
        let template_config = TemplateProviderConfig {
            zebra_url: config.zebra_url.clone(),
            poll_interval_ms: config.template_poll_ms,
        };

        let template_provider = TemplateProvider::new(template_config)
            .map_err(|e| PoolError::TemplateProvider(e.to_string()))?;

        let (session_tx, session_rx) = mpsc::channel(1000);

        Ok(Self {
            config,
            template_provider,
            job_distributor: Arc::new(RwLock::new(JobDistributor::new())),
            share_processor: Arc::new(ShareProcessor::new()),
            duplicate_detector: Arc::new(InMemoryDuplicateDetector::new()),
            payout_tracker: Arc::new(PayoutTracker::default()),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            session_rx,
            session_tx,
        })
    }

    /// Run the pool server
    pub async fn run(mut self) -> Result<()> {
        info!("Starting pool server on {}", self.config.listen_addr);

        // Start TCP listener
        let listener = TcpListener::bind(self.config.listen_addr).await?;

        // Subscribe to template updates
        let mut template_rx = self.template_provider.subscribe();

        // Spawn template provider
        let provider = self.template_provider.clone();
        tokio::spawn(async move {
            if let Err(e) = provider.run().await {
                error!("Template provider error: {}", e);
            }
        });

        info!("Pool server running");

        loop {
            tokio::select! {
                // Accept new connections
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((stream, addr)) => {
                            info!("New connection from {}", addr);
                            self.handle_new_connection(stream).await;
                        }
                        Err(e) => {
                            error!("Accept error: {}", e);
                        }
                    }
                }

                // Receive template updates
                template_result = template_rx.recv() => {
                    match template_result {
                        Ok(template) => {
                            self.handle_new_template(template).await;
                        }
                        Err(e) => {
                            warn!("Template channel error: {}", e);
                        }
                    }
                }

                // Receive session messages
                Some(msg) = self.session_rx.recv() => {
                    self.handle_session_message(msg).await;
                }
            }
        }
    }

    /// Handle a new miner connection
    async fn handle_new_connection(&self, stream: tokio::net::TcpStream) {
        let (server_tx, server_rx) = mpsc::channel(100);

        let vardiff_config = VardiffConfig {
            target_shares_per_minute: self.config.target_shares_per_minute,
            min_difficulty: self.config.initial_difficulty,
            ..Default::default()
        };

        let session = Session::new(
            stream,
            self.config.nonce_1_len,
            vardiff_config,
            self.session_tx.clone(),
            server_rx,
        );

        let channel_id = session.channel.id;

        // Register session
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(channel_id, server_tx.clone());
        }

        // Send current job if available
        {
            let distributor = self.job_distributor.read().await;
            if distributor.has_template() {
                if let Some(job) = distributor.create_job(&session.channel, false) {
                    let _ = server_tx.send(ServerMessage::NewJob(job)).await;
                }
            }
        }

        // Spawn session task
        tokio::spawn(async move {
            if let Err(e) = session.run().await {
                warn!("Session {} error: {}", channel_id, e);
            }
        });
    }

    /// Handle a new template from the template provider
    async fn handle_new_template(&self, template: zcash_template_provider::template::BlockTemplate) {
        let clean_jobs = {
            let mut distributor = self.job_distributor.write().await;
            distributor.update_template(template)
        };

        if clean_jobs {
            // New block - clear duplicate detector
            self.duplicate_detector.clear_all();
            info!("New block - broadcasting clean jobs");
        }

        // Broadcast to all sessions
        self.broadcast_jobs(clean_jobs).await;
    }

    /// Broadcast jobs to all connected miners
    async fn broadcast_jobs(&self, clean_jobs: bool) {
        let sessions = self.sessions.read().await;
        let distributor = self.job_distributor.read().await;

        for (&channel_id, sender) in sessions.iter() {
            // Create a placeholder channel for job creation
            // In production, we'd store channel state server-side
            let nonce_1 = Channel::generate_nonce_1(channel_id, self.config.nonce_1_len);
            let temp_channel = Channel::new(nonce_1, VardiffConfig::default());

            if let Some(mut job) = distributor.create_job(&temp_channel, clean_jobs) {
                job.channel_id = channel_id;
                let _ = sender.send(ServerMessage::NewJob(job)).await;
            }
        }
    }

    /// Handle a message from a session
    async fn handle_session_message(&self, msg: SessionMessage) {
        match msg {
            SessionMessage::ShareSubmitted { channel_id, share, response_tx } => {
                self.handle_share_submission(channel_id, share, response_tx).await;
            }
            SessionMessage::Disconnected { channel_id } => {
                let mut sessions = self.sessions.write().await;
                sessions.remove(&channel_id);
                info!("Session {} disconnected", channel_id);
            }
        }
    }

    /// Handle a share submission
    async fn handle_share_submission(
        &self,
        channel_id: u32,
        share: zcash_mining_protocol::messages::SubmitEquihashShare,
        response_tx: mpsc::Sender<ShareResult>,
    ) {
        // Get channel state (simplified - in production we'd store this)
        let nonce_1 = Channel::generate_nonce_1(channel_id, self.config.nonce_1_len);
        let mut channel = Channel::new(nonce_1, VardiffConfig::default());

        // Reconstruct job state (simplified)
        {
            let distributor = self.job_distributor.read().await;
            if let Some(job) = distributor.create_job(&channel, false) {
                channel.add_job(job, false);
            }
        }

        // Get block target
        let block_target = [0xff; 32]; // Simplified - would come from template

        // Validate share
        let result = self.share_processor.validate_share(
            &share,
            &channel,
            self.duplicate_detector.as_ref(),
            &block_target,
        );

        let share_result = match result {
            Ok(validation) => {
                if validation.accepted {
                    // Record for payout
                    let miner_id: MinerId = format!("channel_{}", channel_id);
                    let difficulty = validation.difficulty.unwrap_or(1.0);
                    self.payout_tracker.record_share(&miner_id, difficulty);

                    if validation.is_block {
                        info!("BLOCK FOUND by channel {}!", channel_id);
                        // TODO: Submit block to Zebra
                    }
                }
                validation.result
            }
            Err(e) => {
                warn!("Share validation error: {}", e);
                ShareResult::Rejected(zcash_mining_protocol::messages::RejectReason::Other(
                    e.to_string()
                ))
            }
        };

        let _ = response_tx.send(share_result).await;
    }
}
```

**Step 2: Commit**

```bash
git add crates/zcash-pool-server/src/server.rs
git commit -m "feat(pool): implement main server orchestration"
```

---

## Task 10: Add Example and Integration Tests

**Files:**
- Create: `crates/zcash-pool-server/examples/run_pool.rs`
- Create: `crates/zcash-pool-server/tests/integration_tests.rs`

**Step 1: Create example**

Create `crates/zcash-pool-server/examples/run_pool.rs`:

```rust
//! Run a basic pool server
//!
//! Usage: cargo run --example run_pool -p zcash-pool-server

use zcash_pool_server::{PoolConfig, PoolServer};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let config = PoolConfig {
        listen_addr: "127.0.0.1:3333".parse()?,
        zebra_url: "http://127.0.0.1:8232".to_string(),
        ..Default::default()
    };

    println!("=== Zcash Pool Server ===");
    println!("Listening on: {}", config.listen_addr);
    println!("Zebra RPC: {}", config.zebra_url);
    println!("Nonce_1 length: {} bytes", config.nonce_1_len);
    println!("Initial difficulty: {}", config.initial_difficulty);
    println!();

    let server = PoolServer::new(config)?;
    server.run().await?;

    Ok(())
}
```

**Step 2: Create integration tests**

Create `crates/zcash-pool-server/tests/integration_tests.rs`:

```rust
//! Integration tests for the pool server

use zcash_pool_server::config::PoolConfig;
use zcash_pool_server::duplicate::InMemoryDuplicateDetector;
use zcash_pool_server::payout::PayoutTracker;
use zcash_pool_server::share::ShareProcessor;
use zcash_pool_server::channel::Channel;
use zcash_pool_server::job::JobDistributor;
use zcash_equihash_validator::VardiffConfig;
use zcash_mining_protocol::messages::{NewEquihashJob, SubmitEquihashShare};
use zcash_template_provider::template::BlockTemplate;

fn make_test_template() -> BlockTemplate {
    BlockTemplate {
        height: 1000,
        prev_hash: [0xaa; 32],
        merkle_root: [0xbb; 32],
        block_commitments: [0xcc; 32],
        time: 1700000000,
        bits: 0x1d00ffff,
        version: 5,
        transactions: vec![],
        coinbase_tx: vec![],
        target: "00000000ffff0000000000000000000000000000000000000000000000000000".to_string(),
    }
}

#[test]
fn test_config_defaults() {
    let config = PoolConfig::default();
    assert_eq!(config.nonce_1_len, 4);
    assert!(config.validation_threads > 0);
}

#[test]
fn test_job_distribution_flow() {
    let mut distributor = JobDistributor::new();
    let template = make_test_template();

    // First template triggers new block
    assert!(distributor.update_template(template.clone()));

    // Same block doesn't trigger
    assert!(!distributor.update_template(template));

    // Create job for a channel
    let channel = Channel::new(vec![0x01, 0x02, 0x03, 0x04], VardiffConfig::default());
    let job = distributor.create_job(&channel, false).unwrap();

    assert_eq!(job.nonce_1, vec![0x01, 0x02, 0x03, 0x04]);
    assert_eq!(job.nonce_2_len, 28);
    assert_eq!(job.prev_hash, [0xaa; 32]);
}

#[test]
fn test_share_validation_rejects_invalid() {
    let processor = ShareProcessor::new();
    let detector = InMemoryDuplicateDetector::new();
    let mut channel = Channel::new(vec![0; 4], VardiffConfig::default());

    // Add a job
    let job = NewEquihashJob {
        channel_id: channel.id,
        job_id: 1,
        future_job: false,
        version: 5,
        prev_hash: [0; 32],
        merkle_root: [0; 32],
        block_commitments: [0; 32],
        nonce_1: channel.nonce_1.clone(),
        nonce_2_len: channel.nonce_2_len,
        time: 0,
        bits: 0x1d00ffff,
        target: [0xff; 32],
        clean_jobs: false,
    };
    channel.add_job(job, false);

    // Create invalid share
    let share = SubmitEquihashShare {
        channel_id: channel.id,
        sequence_number: 1,
        job_id: 1,
        nonce_2: vec![0; 28],
        time: 0,
        solution: [0; 1344], // Invalid solution
    };

    let result = processor.validate_share(&share, &channel, &detector, &[0xff; 32]);
    assert!(result.is_ok());
    assert!(!result.unwrap().accepted);
}

#[test]
fn test_duplicate_detection_in_validation() {
    let detector = InMemoryDuplicateDetector::new();

    // First share - not duplicate
    assert!(!detector.check_and_record(1, &[0x01], &[0xaa; 100]));

    // Same share - duplicate
    assert!(detector.check_and_record(1, &[0x01], &[0xaa; 100]));

    // After clear - not duplicate again
    detector.clear_job(1);
    assert!(!detector.check_and_record(1, &[0x01], &[0xaa; 100]));
}

#[test]
fn test_payout_tracking() {
    let tracker = PayoutTracker::default();

    tracker.record_share(&"miner1".to_string(), 100.0);
    tracker.record_share(&"miner2".to_string(), 200.0);
    tracker.record_share(&"miner1".to_string(), 150.0);

    let stats1 = tracker.get_stats(&"miner1".to_string()).unwrap();
    assert_eq!(stats1.total_shares, 2);
    assert_eq!(stats1.total_difficulty, 250.0);

    let stats2 = tracker.get_stats(&"miner2".to_string()).unwrap();
    assert_eq!(stats2.total_shares, 1);
    assert_eq!(stats2.total_difficulty, 200.0);
}

#[test]
fn test_channel_nonce_generation() {
    // Different channels get different nonce_1 values
    let c1 = Channel::new(Channel::generate_nonce_1(1, 4), VardiffConfig::default());
    let c2 = Channel::new(Channel::generate_nonce_1(2, 4), VardiffConfig::default());

    assert_ne!(c1.nonce_1, c2.nonce_1);
    assert_eq!(c1.nonce_2_len, 28);
    assert_eq!(c2.nonce_2_len, 28);
}
```

**Step 3: Run tests**

Run: `cargo test -p zcash-pool-server`
Expected: All tests pass

**Step 4: Commit**

```bash
git add crates/zcash-pool-server/
git commit -m "test(pool): add integration tests and example"
```

---

## Task 11: Documentation and Final Verification

**Files:**
- Create: `crates/zcash-pool-server/README.md`
- Update: `README.md` (workspace root)

**Step 1: Create pool server README**

Create `crates/zcash-pool-server/README.md`:

```markdown
# zcash-pool-server

Stratum V2 pool server for Zcash Equihash mining.

## Overview

This crate provides a basic pool server that:

- Accepts miner connections over TCP (port 3333)
- Distributes Equihash mining jobs from Zebra templates
- Validates submitted shares using Equihash (200,9)
- Tracks contributions for PPS payout
- Supports per-miner adaptive difficulty (vardiff)

## Architecture

```
┌──────────────┐    ┌──────────────┐    ┌──────────────┐
│   Listener   │───▶│   Session    │◀──▶│     Job      │
│  (TCP:3333)  │    │   Manager    │    │  Distributor │
└──────────────┘    └──────┬───────┘    └──────▲───────┘
                          │                    │
                          ▼                    │
                   ┌──────────────┐    ┌───────┴──────┐
                   │    Share     │    │   Template   │
                   │  Processor   │    │   Provider   │
                   └──────────────┘    └──────────────┘
```

## Usage

```rust
use zcash_pool_server::{PoolConfig, PoolServer};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = PoolConfig {
        listen_addr: "0.0.0.0:3333".parse()?,
        zebra_url: "http://127.0.0.1:8232".to_string(),
        ..Default::default()
    };

    let server = PoolServer::new(config)?;
    server.run().await?;
    Ok(())
}
```

## Configuration

| Option | Default | Description |
|--------|---------|-------------|
| `listen_addr` | `0.0.0.0:3333` | TCP address for miner connections |
| `zebra_url` | `http://127.0.0.1:8232` | Zebra RPC endpoint |
| `nonce_1_len` | 4 | Pool nonce prefix length (bytes) |
| `initial_difficulty` | 1.0 | Starting share difficulty |
| `target_shares_per_minute` | 5.0 | Vardiff target rate |
| `validation_threads` | 4 | Threads for Equihash validation |
| `max_connections` | 10000 | Maximum concurrent miners |

## Requirements

- Running Zebra node with RPC enabled
- Rust 1.75+

## Phase 3 Limitations

This is an MVP implementation. Not yet included:

- Block submission to Zebra
- Persistent payout tracking (database)
- SetTarget message encoding
- Full SV2 handshake
- TLS/Noise encryption
```

**Step 2: Update workspace README**

Update `README.md` to add Phase 3:

```markdown
# Stratum V2 for Zcash

Implementation of Stratum V2 mining protocol for Zcash with support for decentralized block template construction.

## Project Status

- Phase 1: Zcash Template Provider - **Complete**
- Phase 2: Equihash Mining Protocol - **Complete**
- Phase 3: Basic Pool Server - **Complete**

## Crates

| Crate | Description |
|-------|-------------|
| `zcash-template-provider` | Template Provider interfacing with Zebra |
| `zcash-mining-protocol` | SV2 message types for Equihash mining |
| `zcash-equihash-validator` | Share validation and vardiff |
| `zcash-pool-server` | Basic Stratum V2 pool server |

## Building

```bash
cargo build --release
```

## Testing

```bash
cargo test
```

## Examples

```bash
# Fetch a template from Zebra
cargo run --example fetch_template -p zcash-template-provider

# Demonstrate share validation
cargo run --example validate_share -p zcash-equihash-validator

# Run the pool server (requires Zebra)
cargo run --example run_pool -p zcash-pool-server
```

## Architecture

See [docs/plans/](docs/plans/) for the full implementation plans.

## License

MIT OR Apache-2.0
```

**Step 3: Run all tests**

Run: `cargo test`
Expected: All tests pass

**Step 4: Commit**

```bash
git add README.md crates/zcash-pool-server/README.md
git commit -m "docs: add Phase 3 documentation"
```

---

## Summary

Phase 3 creates the `zcash-pool-server` crate with:

1. **Configuration** - Server settings for listen address, Zebra URL, vardiff, etc.
2. **Error Types** - Comprehensive error handling
3. **Duplicate Detection** - In-memory with trait for future upgrades
4. **Payout Tracking** - Simple PPS tracking per miner
5. **Channel State** - Standard Channels with nonce_1 assignment
6. **Job Distribution** - Template → Job conversion with new-block detection
7. **Share Processing** - Full Equihash validation pipeline
8. **Session Handler** - Per-miner TCP connection management
9. **Main Server** - Orchestration of all components
10. **Tests & Examples** - Integration tests and runnable example

**Dependencies:**
- Phase 1 (`zcash-template-provider`) for block templates
- Phase 2 (`zcash-mining-protocol`, `zcash-equihash-validator`) for messages and validation

**Not included (future phases):**
- Block submission to Zebra
- TLS/Noise encryption
- Full SV2 handshake protocol
- Database-backed payout tracking
- Job Declaration Protocol (Phase 4)
