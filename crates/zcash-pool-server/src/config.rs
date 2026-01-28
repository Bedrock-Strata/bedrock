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

/// Configuration validation errors
#[derive(Debug, Clone, PartialEq)]
pub enum ConfigError {
    /// Invalid nonce_1_len (must be 1-31 to leave room for nonce_2)
    InvalidNonce1Len(u8),
    /// Invalid difficulty (must be positive)
    InvalidDifficulty(f64),
    /// Invalid target shares per minute (must be positive)
    InvalidTargetSharesPerMinute(f64),
    /// Invalid validation threads (must be at least 1)
    InvalidValidationThreads(usize),
    /// Invalid template poll interval (must be at least 100ms)
    InvalidTemplatePollMs(u64),
    /// Invalid max connections (must be at least 1)
    InvalidMaxConnections(usize),
    /// Fiber relay enabled but no auth key provided
    FiberMissingAuthKey,
    /// Invalid FEC shard configuration
    InvalidFecConfig { data: usize, parity: usize },
    /// JD enabled but no pool payout script
    JdMissingPayoutScript,
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::InvalidNonce1Len(v) => write!(f, "nonce_1_len {} must be 1-31", v),
            ConfigError::InvalidDifficulty(v) => write!(f, "initial_difficulty {} must be positive", v),
            ConfigError::InvalidTargetSharesPerMinute(v) => {
                write!(f, "target_shares_per_minute {} must be positive", v)
            }
            ConfigError::InvalidValidationThreads(v) => {
                write!(f, "validation_threads {} must be at least 1", v)
            }
            ConfigError::InvalidTemplatePollMs(v) => {
                write!(f, "template_poll_ms {} must be at least 100", v)
            }
            ConfigError::InvalidMaxConnections(v) => {
                write!(f, "max_connections {} must be at least 1", v)
            }
            ConfigError::FiberMissingAuthKey => {
                write!(f, "fiber_relay_enabled requires fiber_auth_key")
            }
            ConfigError::InvalidFecConfig { data, parity } => {
                write!(f, "FEC config invalid: data={}, parity={} (both must be >= 1)", data, parity)
            }
            ConfigError::JdMissingPayoutScript => {
                write!(f, "jd_listen_addr set but pool_payout_script is missing")
            }
        }
    }
}

impl std::error::Error for ConfigError {}

impl PoolConfig {
    /// Validate the configuration and return any errors
    pub fn validate(&self) -> Result<(), ConfigError> {
        // nonce_1_len must leave room for at least 1 byte of nonce_2
        if self.nonce_1_len == 0 || self.nonce_1_len > 31 {
            return Err(ConfigError::InvalidNonce1Len(self.nonce_1_len));
        }

        // Difficulty must be positive
        if self.initial_difficulty <= 0.0 || !self.initial_difficulty.is_finite() {
            return Err(ConfigError::InvalidDifficulty(self.initial_difficulty));
        }

        // Target shares per minute must be positive
        if self.target_shares_per_minute <= 0.0 || !self.target_shares_per_minute.is_finite() {
            return Err(ConfigError::InvalidTargetSharesPerMinute(
                self.target_shares_per_minute,
            ));
        }

        // Need at least 1 validation thread
        if self.validation_threads == 0 {
            return Err(ConfigError::InvalidValidationThreads(self.validation_threads));
        }

        // Template poll interval should be at least 100ms to avoid hammering Zebra
        if self.template_poll_ms < 100 {
            return Err(ConfigError::InvalidTemplatePollMs(self.template_poll_ms));
        }

        // Need at least 1 connection
        if self.max_connections == 0 {
            return Err(ConfigError::InvalidMaxConnections(self.max_connections));
        }

        // Fiber relay requires auth key
        if self.fiber_relay_enabled && self.fiber_auth_key.is_none() {
            return Err(ConfigError::FiberMissingAuthKey);
        }

        // FEC shards must be valid
        if self.fiber_relay_enabled
            && (self.fiber_data_shards == 0 || self.fiber_parity_shards == 0)
        {
            return Err(ConfigError::InvalidFecConfig {
                data: self.fiber_data_shards,
                parity: self.fiber_parity_shards,
            });
        }

        // JD requires payout script
        if self.jd_listen_addr.is_some() && self.pool_payout_script.is_none() {
            return Err(ConfigError::JdMissingPayoutScript);
        }

        Ok(())
    }
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
