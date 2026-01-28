//! Pool server configuration

use std::net::SocketAddr;
use std::path::PathBuf;
use zcash_jd_server::ValidationLevel;

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

    /// Enable Noise encryption for miner connections
    pub noise_enabled: bool,

    /// Path to server private key file (hex-encoded)
    pub noise_private_key_path: Option<PathBuf>,

    /// Enable Noise for JD connections
    pub jd_noise_enabled: bool,

    /// Enable Full-Template mode for JD server
    pub jd_full_template_enabled: bool,

    /// Validation level for Full-Template mode
    pub jd_full_template_validation: ValidationLevel,

    /// Minimum pool payout value (zatoshis) for Full-Template mode
    pub jd_min_pool_payout: u64,

    /// Metrics server address
    pub metrics_addr: Option<SocketAddr>,

    /// Use JSON logging format
    pub json_logging: bool,

    /// OTLP endpoint for distributed tracing
    pub otlp_endpoint: Option<String>,

    /// Fiber relay configuration (optional - None disables relay)
    pub fiber_relay_enabled: bool,
    /// UDP bind address for fiber relay (default: 0.0.0.0:8336)
    pub fiber_bind_addr: Option<SocketAddr>,
    /// Relay peer addresses to connect to
    pub fiber_relay_peers: Vec<SocketAddr>,
    /// Shared authentication key for relay network (32 bytes)
    pub fiber_auth_key: Option<[u8; 32]>,
    /// FEC data shards (default: 10)
    pub fiber_data_shards: usize,
    /// FEC parity shards (default: 3)
    pub fiber_parity_shards: usize,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            listen_addr: SocketAddr::from(([0, 0, 0, 0], 3333)),
            zebra_url: "http://127.0.0.1:8232".to_string(),
            template_poll_ms: 1000,
            validation_threads: 4,
            nonce_1_len: 4,
            initial_difficulty: 1.0,
            target_shares_per_minute: 5.0,
            max_connections: 10000,
            jd_listen_addr: None, // Disabled by default
            pool_payout_script: None,
            noise_enabled: false,
            noise_private_key_path: None,
            jd_noise_enabled: false,
            jd_full_template_enabled: false,
            jd_full_template_validation: ValidationLevel::Standard,
            jd_min_pool_payout: 0,
            metrics_addr: Some(SocketAddr::from(([127, 0, 0, 1], 9090))),
            json_logging: false,
            otlp_endpoint: None,
            fiber_relay_enabled: false,
            fiber_bind_addr: Some(SocketAddr::from(([0, 0, 0, 0], 8336))),
            fiber_relay_peers: Vec::new(),
            fiber_auth_key: None,
            fiber_data_shards: 10,
            fiber_parity_shards: 3,
        }
    }
}
