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

    /// Optional: JD Server listen address (enables Job Declaration support)
    pub jd_listen_addr: Option<SocketAddr>,

    /// Pool's payout script for coinbase (used by JD Server)
    pub pool_payout_script: Option<Vec<u8>>,
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
            jd_listen_addr: None, // Disabled by default
            pool_payout_script: None,
        }
    }
}
