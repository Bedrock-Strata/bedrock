//! JD Server configuration

use std::time::Duration;

/// JD Server configuration
#[derive(Debug, Clone)]
pub struct JdServerConfig {
    /// Token validity duration
    pub token_lifetime: Duration,

    /// Maximum coinbase output size miners can add (bytes)
    pub coinbase_output_max_additional_size: u32,

    /// Pool's payout script (for coinbase output)
    pub pool_payout_script: Vec<u8>,

    /// Allow async mining (start mining before job acknowledged)
    pub async_mining_allowed: bool,

    /// Maximum active tokens per client
    pub max_tokens_per_client: usize,

    /// Enable Noise encryption for JD client connections
    pub noise_enabled: bool,
}

impl Default for JdServerConfig {
    fn default() -> Self {
        Self {
            token_lifetime: Duration::from_secs(300), // 5 minutes
            coinbase_output_max_additional_size: 256,
            pool_payout_script: vec![], // Must be set by operator
            async_mining_allowed: true,
            max_tokens_per_client: 10,
            noise_enabled: false,
        }
    }
}
