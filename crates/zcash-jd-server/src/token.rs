//! Mining job token allocation and tracking

use crate::config::JdServerConfig;
use crate::error::{JdServerError, Result};
use crate::messages::JobDeclarationMode;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;
use std::time::{Duration, Instant};

/// A mining job token
#[derive(Debug, Clone)]
pub struct MiningJobToken {
    /// Unique token bytes
    pub token: Vec<u8>,
    /// When the token was issued
    pub issued_at: Instant,
    /// Token lifetime
    pub lifetime: Duration,
    /// Client identifier
    pub client_id: String,
    /// Granted job declaration mode for this token
    pub granted_mode: JobDeclarationMode,
    /// Associated job info (set when job declared)
    pub job_info: Option<DeclaredJobInfo>,
}

/// Information about a declared job
#[derive(Debug, Clone)]
pub struct DeclaredJobInfo {
    /// Pool-assigned job ID
    pub job_id: u32,
    /// Previous block hash
    pub prev_hash: [u8; 32],
    /// Merkle root
    pub merkle_root: [u8; 32],
    /// Coinbase transaction
    pub coinbase_tx: Vec<u8>,
}

impl MiningJobToken {
    /// Check if the token has expired
    pub fn is_expired(&self) -> bool {
        self.issued_at.elapsed() > self.lifetime
    }
}

/// Token allocation manager
pub struct TokenManager {
    /// Configuration
    config: JdServerConfig,
    /// Active tokens (token bytes -> token info)
    tokens: RwLock<HashMap<Vec<u8>, MiningJobToken>>,
    /// Counter for generating unique tokens
    token_counter: AtomicU64,
}

impl TokenManager {
    pub fn new(config: JdServerConfig) -> Self {
        Self {
            config,
            tokens: RwLock::new(HashMap::new()),
            token_counter: AtomicU64::new(1),
        }
    }

    /// Allocate a new token for a client with CoinbaseOnly mode (default)
    pub fn allocate_token(&self, client_id: &str) -> Result<MiningJobToken> {
        self.allocate_token_with_mode(client_id, JobDeclarationMode::CoinbaseOnly)
    }

    /// Allocate a new token for a client with a specific mode
    pub fn allocate_token_with_mode(
        &self,
        client_id: &str,
        granted_mode: JobDeclarationMode,
    ) -> Result<MiningJobToken> {
        let counter = self.token_counter.fetch_add(1, Ordering::SeqCst);
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Generate token: 8 bytes counter + 8 bytes timestamp
        let mut token = Vec::with_capacity(16);
        token.extend_from_slice(&counter.to_le_bytes());
        token.extend_from_slice(&timestamp.to_le_bytes());

        let mining_token = MiningJobToken {
            token: token.clone(),
            issued_at: Instant::now(),
            lifetime: self.config.token_lifetime,
            client_id: client_id.to_string(),
            granted_mode,
            job_info: None,
        };

        // Store token
        {
            let mut tokens = self.tokens.write().unwrap();
            tokens.insert(token, mining_token.clone());
        }

        // Cleanup expired tokens periodically
        self.cleanup_expired();

        Ok(mining_token)
    }

    /// Validate a token and return its info
    pub fn validate_token(&self, token: &[u8]) -> Result<MiningJobToken> {
        let tokens = self.tokens.read().unwrap();
        let mining_token = tokens.get(token).ok_or(JdServerError::InvalidToken)?;

        if mining_token.is_expired() {
            return Err(JdServerError::TokenExpired);
        }

        Ok(mining_token.clone())
    }

    /// Associate a declared job with a token
    pub fn set_job_info(&self, token: &[u8], job_info: DeclaredJobInfo) -> Result<()> {
        let mut tokens = self.tokens.write().unwrap();
        let mining_token = tokens.get_mut(token).ok_or(JdServerError::InvalidToken)?;

        if mining_token.is_expired() {
            return Err(JdServerError::TokenExpired);
        }

        mining_token.job_info = Some(job_info);
        Ok(())
    }

    /// Get job info for a token
    pub fn get_job_info(&self, token: &[u8]) -> Result<DeclaredJobInfo> {
        let tokens = self.tokens.read().unwrap();
        let mining_token = tokens.get(token).ok_or(JdServerError::InvalidToken)?;

        mining_token
            .job_info
            .clone()
            .ok_or(JdServerError::Protocol("Job not declared".to_string()))
    }

    /// Remove expired tokens
    fn cleanup_expired(&self) {
        let mut tokens = self.tokens.write().unwrap();
        tokens.retain(|_, t| !t.is_expired());
    }

    /// Get config values for token response
    pub fn coinbase_output_max_additional_size(&self) -> u32 {
        self.config.coinbase_output_max_additional_size
    }

    pub fn pool_payout_script(&self) -> &[u8] {
        &self.config.pool_payout_script
    }

    pub fn async_mining_allowed(&self) -> bool {
        self.config.async_mining_allowed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_allocation() {
        let config = JdServerConfig::default();
        let manager = TokenManager::new(config);

        let token = manager.allocate_token("miner-01").unwrap();
        assert!(!token.is_expired());
        assert_eq!(token.client_id, "miner-01");
        assert_eq!(token.token.len(), 16);
        // Default allocation should be CoinbaseOnly mode
        assert_eq!(token.granted_mode, JobDeclarationMode::CoinbaseOnly);
    }

    #[test]
    fn test_token_allocation_with_mode() {
        let config = JdServerConfig::default();
        let manager = TokenManager::new(config);

        // Test allocating with FullTemplate mode
        let token = manager
            .allocate_token_with_mode("miner-01", JobDeclarationMode::FullTemplate)
            .unwrap();
        assert!(!token.is_expired());
        assert_eq!(token.client_id, "miner-01");
        assert_eq!(token.granted_mode, JobDeclarationMode::FullTemplate);

        // Test allocating with CoinbaseOnly mode
        let token2 = manager
            .allocate_token_with_mode("miner-02", JobDeclarationMode::CoinbaseOnly)
            .unwrap();
        assert_eq!(token2.granted_mode, JobDeclarationMode::CoinbaseOnly);
    }

    #[test]
    fn test_token_validation() {
        let config = JdServerConfig::default();
        let manager = TokenManager::new(config);

        let token = manager.allocate_token("miner-01").unwrap();
        let validated = manager.validate_token(&token.token).unwrap();
        assert_eq!(validated.client_id, "miner-01");
        assert_eq!(validated.granted_mode, JobDeclarationMode::CoinbaseOnly);
    }

    #[test]
    fn test_token_validation_preserves_mode() {
        let config = JdServerConfig::default();
        let manager = TokenManager::new(config);

        let token = manager
            .allocate_token_with_mode("miner-01", JobDeclarationMode::FullTemplate)
            .unwrap();
        let validated = manager.validate_token(&token.token).unwrap();
        assert_eq!(validated.granted_mode, JobDeclarationMode::FullTemplate);
    }

    #[test]
    fn test_invalid_token() {
        let config = JdServerConfig::default();
        let manager = TokenManager::new(config);

        let result = manager.validate_token(&[0x00, 0x01, 0x02]);
        assert!(matches!(result, Err(JdServerError::InvalidToken)));
    }

    #[test]
    fn test_job_info() {
        let config = JdServerConfig::default();
        let manager = TokenManager::new(config);

        let token = manager.allocate_token("miner-01").unwrap();

        let job_info = DeclaredJobInfo {
            job_id: 42,
            prev_hash: [0xaa; 32],
            merkle_root: [0xbb; 32],
            coinbase_tx: vec![0x01; 100],
        };

        manager.set_job_info(&token.token, job_info.clone()).unwrap();

        let retrieved = manager.get_job_info(&token.token).unwrap();
        assert_eq!(retrieved.job_id, 42);
    }
}
