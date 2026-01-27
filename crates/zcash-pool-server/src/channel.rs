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
