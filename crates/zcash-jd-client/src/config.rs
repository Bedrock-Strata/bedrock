//! JD Client configuration

use std::net::SocketAddr;

#[derive(Debug, Clone)]
pub struct JdClientConfig {
    pub zebra_url: String,
    pub pool_jd_addr: SocketAddr,
    pub user_identifier: String,
    pub template_poll_ms: u64,
    pub miner_payout_address: Option<String>,
    /// Enable Noise encryption
    pub noise_enabled: bool,
    /// Pool's Noise public key (hex-encoded, required if noise_enabled)
    pub pool_public_key: Option<String>,
}

impl Default for JdClientConfig {
    fn default() -> Self {
        Self {
            zebra_url: "http://127.0.0.1:8232".to_string(),
            pool_jd_addr: "127.0.0.1:3334".parse().unwrap(),
            user_identifier: "zcash-jd-client".to_string(),
            template_poll_ms: 1000,
            miner_payout_address: None,
            noise_enabled: false,
            pool_public_key: None,
        }
    }
}
