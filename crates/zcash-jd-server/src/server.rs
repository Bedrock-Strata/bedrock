//! Job Declaration Server implementation
//!
//! This module implements the JD server logic for handling miner connections,
//! token allocation, job declaration, and solution submission in Coinbase-Only mode.

use crate::codec::{
    decode_allocate_token, decode_push_solution, decode_set_custom_job,
    encode_allocate_token_success, encode_set_custom_job_error, encode_set_custom_job_success,
};
use crate::config::JdServerConfig;
use crate::error::{JdServerError, Result};
use crate::messages::{
    message_types, AllocateMiningJobTokenSuccess, PushSolution, SetCustomMiningJob,
    SetCustomMiningJobError, SetCustomMiningJobErrorCode, SetCustomMiningJobSuccess,
};
use crate::token::{DeclaredJobInfo, TokenManager};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::{debug, error, info, warn};
use zcash_mining_protocol::codec::MessageFrame;
use zcash_pool_common::PayoutTracker;

/// JD Server embedded in pool
///
/// Handles the Job Declaration protocol for Coinbase-Only mode mining.
/// This server validates tokens, processes job declarations, and tracks
/// block solutions for miners who want to create their own coinbase transactions.
pub struct JdServer {
    /// Configuration
    config: JdServerConfig,
    /// Token manager
    token_manager: Arc<TokenManager>,
    /// Job ID counter
    next_job_id: AtomicU32,
    /// Payout tracker (shared with pool)
    payout_tracker: Arc<PayoutTracker>,
    /// Current prev_hash (for stale detection)
    current_prev_hash: Arc<tokio::sync::RwLock<Option<[u8; 32]>>>,
}

impl JdServer {
    /// Create a new JD Server
    pub fn new(config: JdServerConfig, payout_tracker: Arc<PayoutTracker>) -> Self {
        let token_manager = Arc::new(TokenManager::new(config.clone()));
        Self {
            config,
            token_manager,
            next_job_id: AtomicU32::new(1),
            payout_tracker,
            current_prev_hash: Arc::new(tokio::sync::RwLock::new(None)),
        }
    }

    /// Update the current prev_hash (called when new block arrives)
    pub async fn set_current_prev_hash(&self, prev_hash: [u8; 32]) {
        let mut lock = self.current_prev_hash.write().await;
        *lock = Some(prev_hash);
        debug!(
            prev_hash = ?hex::encode(prev_hash),
            "Updated current prev_hash"
        );
    }

    /// Get the current prev_hash
    pub async fn get_current_prev_hash(&self) -> Option<[u8; 32]> {
        let lock = self.current_prev_hash.read().await;
        *lock
    }

    /// Handle a token allocation request
    ///
    /// Allocates a new mining job token for the given user.
    pub fn handle_allocate_token(
        &self,
        request_id: u32,
        user_id: &str,
    ) -> Result<AllocateMiningJobTokenSuccess> {
        let token = self.token_manager.allocate_token(user_id)?;

        info!(
            request_id,
            user_id,
            token_len = token.token.len(),
            "Allocated mining job token"
        );

        Ok(AllocateMiningJobTokenSuccess {
            request_id,
            mining_job_token: token.token,
            coinbase_output: self.config.pool_payout_script.clone(),
            coinbase_output_max_additional_size: self.config.coinbase_output_max_additional_size,
            async_mining_allowed: self.config.async_mining_allowed,
            // For now, always grant CoinbaseOnly mode (Full-Template support coming in future phase)
            granted_mode: crate::messages::JobDeclarationMode::CoinbaseOnly,
        })
    }

    /// Handle a custom job declaration
    ///
    /// Validates the token, checks for stale prev_hash, validates the coinbase,
    /// allocates a job ID, and stores job info.
    pub async fn handle_declare_job(
        &self,
        request: SetCustomMiningJob,
    ) -> std::result::Result<SetCustomMiningJobSuccess, SetCustomMiningJobError> {
        // 1. Validate basic structure
        if let Err(e) = request.validate() {
            return Err(SetCustomMiningJobError::new(
                request.channel_id,
                request.request_id,
                e,
                format!("Validation failed: {}", e),
            ));
        }

        // 2. Validate token
        match self.token_manager.validate_token(&request.mining_job_token) {
            Ok(_) => {}
            Err(JdServerError::InvalidToken) => {
                warn!(
                    request_id = request.request_id,
                    "Job declaration rejected: invalid token"
                );
                return Err(SetCustomMiningJobError::invalid_token(
                    request.channel_id,
                    request.request_id,
                ));
            }
            Err(JdServerError::TokenExpired) => {
                warn!(
                    request_id = request.request_id,
                    "Job declaration rejected: token expired"
                );
                return Err(SetCustomMiningJobError::token_expired(
                    request.channel_id,
                    request.request_id,
                ));
            }
            Err(e) => {
                error!(
                    request_id = request.request_id,
                    error = %e,
                    "Unexpected error validating token"
                );
                return Err(SetCustomMiningJobError::new(
                    request.channel_id,
                    request.request_id,
                    SetCustomMiningJobErrorCode::Other,
                    format!("Token validation error: {}", e),
                ));
            }
        }

        // 3. Check prev_hash matches current (stale detection)
        let current_prev_hash = self.current_prev_hash.read().await;
        if let Some(expected_prev_hash) = *current_prev_hash {
            if request.prev_hash != expected_prev_hash {
                warn!(
                    request_id = request.request_id,
                    expected = ?hex::encode(expected_prev_hash),
                    got = ?hex::encode(request.prev_hash),
                    "Job declaration rejected: stale prev_hash"
                );
                return Err(SetCustomMiningJobError::new(
                    request.channel_id,
                    request.request_id,
                    SetCustomMiningJobErrorCode::StalePrevHash,
                    "Previous block hash does not match current chain tip",
                ));
            }
        }
        drop(current_prev_hash);

        // 4. Validate coinbase is not empty
        if request.coinbase_tx.is_empty() {
            return Err(SetCustomMiningJobError::invalid_coinbase(
                request.channel_id,
                request.request_id,
                "Coinbase transaction is empty",
            ));
        }

        // 5. Allocate job_id
        let job_id = self.next_job_id.fetch_add(1, Ordering::SeqCst);

        // 6. Store job info with token
        let job_info = DeclaredJobInfo {
            job_id,
            prev_hash: request.prev_hash,
            merkle_root: request.merkle_root,
            coinbase_tx: request.coinbase_tx.clone(),
        };

        if let Err(e) = self
            .token_manager
            .set_job_info(&request.mining_job_token, job_info)
        {
            error!(
                request_id = request.request_id,
                job_id,
                error = %e,
                "Failed to store job info"
            );
            return Err(SetCustomMiningJobError::new(
                request.channel_id,
                request.request_id,
                SetCustomMiningJobErrorCode::Other,
                format!("Failed to store job info: {}", e),
            ));
        }

        info!(
            request_id = request.request_id,
            channel_id = request.channel_id,
            job_id,
            "Custom mining job declared successfully"
        );

        Ok(SetCustomMiningJobSuccess::new(
            request.channel_id,
            request.request_id,
            job_id,
        ))
    }

    /// Handle a block solution submission
    ///
    /// Note: Per JD protocol spec, PushSolution is a one-way message;
    /// the server does not send a response.
    pub async fn handle_push_solution(&self, solution: PushSolution) -> Result<()> {
        debug!(
            channel_id = solution.channel_id,
            job_id = solution.job_id,
            version = solution.version,
            time = solution.time,
            "Received block solution"
        );

        // Validate solution length (always true for fixed-size array, but good practice)
        if !solution.validate_solution_len() {
            warn!(
                job_id = solution.job_id,
                "Invalid solution length"
            );
            return Err(JdServerError::Protocol("Invalid solution length".to_string()));
        }

        // TODO: In a full implementation, we would:
        // 1. Look up the job info by job_id
        // 2. Reconstruct the full block header
        // 3. Verify the Equihash solution
        // 4. Submit to the Zcash node if valid
        // 5. Record payout credit for the miner

        // For now, just record a share for the miner
        // In production, the miner ID would come from the session/token
        let miner_id = format!("jd-miner-{}", solution.channel_id);
        self.payout_tracker.record_share(&miner_id, 1.0);

        info!(
            channel_id = solution.channel_id,
            job_id = solution.job_id,
            "Block solution recorded"
        );

        Ok(())
    }

    /// Get token manager (for testing)
    pub fn token_manager(&self) -> &TokenManager {
        &self.token_manager
    }

    /// Get configuration
    pub fn config(&self) -> &JdServerConfig {
        &self.config
    }
}

/// Handle a JD client connection
///
/// This function reads frames from the TCP stream, parses message types,
/// dispatches to the appropriate handlers, and writes responses.
///
/// # Noise Protocol Support
///
/// When `noise_enabled` is true in the server config, this function should
/// perform the Noise NK handshake before processing messages. The integration
/// would involve:
///
/// 1. Use `zcash_stratum_noise::NoiseResponder` for the server side
/// 2. Perform handshake: `let mut noise_stream = responder.handshake(stream).await?`
/// 3. Use `noise_stream` (which implements AsyncRead + AsyncWrite) for all I/O
///
/// TODO: Refactor this function to be generic over `AsyncRead + AsyncWrite + Unpin`
/// to support both plain TCP and Noise-encrypted streams. Example signature:
/// ```ignore
/// pub async fn handle_jd_client<S>(
///     mut stream: S,
///     jd_server: Arc<JdServer>,
///     client_id: String,
/// ) -> Result<()>
/// where
///     S: AsyncRead + AsyncWrite + Unpin,
/// ```
pub async fn handle_jd_client(
    mut stream: TcpStream,
    jd_server: Arc<JdServer>,
    client_id: String,
) -> Result<()> {
    // TODO: When noise_enabled is true in config, wrap `stream` with Noise:
    // if jd_server.config().noise_enabled {
    //     let responder = NoiseResponder::new(server_static_keypair);
    //     let noise_stream = responder.handshake(stream).await?;
    //     return handle_jd_client_inner(noise_stream, jd_server, client_id).await;
    // }

    info!(client_id, "JD client connected");

    let mut header_buf = [0u8; MessageFrame::HEADER_SIZE];

    loop {
        // Read frame header
        match stream.read_exact(&mut header_buf).await {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                info!(client_id, "JD client disconnected");
                return Ok(());
            }
            Err(e) => {
                error!(client_id, error = %e, "Error reading from JD client");
                return Err(JdServerError::Io(e));
            }
        }

        // Parse frame header
        let frame = MessageFrame::decode(&header_buf)
            .map_err(|e| JdServerError::Protocol(e.to_string()))?;

        // Read payload
        let mut payload = vec![0u8; frame.length as usize];
        if frame.length > 0 {
            stream.read_exact(&mut payload).await?;
        }

        // Combine header and payload for decoding
        let mut full_message = header_buf.to_vec();
        full_message.extend(payload);

        // Dispatch based on message type
        match frame.msg_type {
            message_types::ALLOCATE_MINING_JOB_TOKEN => {
                let request = decode_allocate_token(&full_message)?;
                debug!(
                    client_id,
                    request_id = request.request_id,
                    user_id = %request.user_identifier,
                    "Received AllocateMiningJobToken"
                );

                match jd_server.handle_allocate_token(request.request_id, &request.user_identifier)
                {
                    Ok(response) => {
                        let encoded = encode_allocate_token_success(&response)?;
                        stream.write_all(&encoded).await?;
                        stream.flush().await?;
                    }
                    Err(e) => {
                        error!(
                            client_id,
                            request_id = request.request_id,
                            error = %e,
                            "Failed to allocate token"
                        );
                        // No error response defined in spec for AllocateMiningJobToken
                    }
                }
            }

            message_types::SET_CUSTOM_MINING_JOB => {
                let request = decode_set_custom_job(&full_message)?;
                debug!(
                    client_id,
                    channel_id = request.channel_id,
                    request_id = request.request_id,
                    "Received SetCustomMiningJob"
                );

                match jd_server.handle_declare_job(request).await {
                    Ok(response) => {
                        let encoded = encode_set_custom_job_success(&response)?;
                        stream.write_all(&encoded).await?;
                        stream.flush().await?;
                    }
                    Err(error) => {
                        let encoded = encode_set_custom_job_error(&error)?;
                        stream.write_all(&encoded).await?;
                        stream.flush().await?;
                    }
                }
            }

            message_types::PUSH_SOLUTION => {
                let solution = decode_push_solution(&full_message)?;
                debug!(
                    client_id,
                    channel_id = solution.channel_id,
                    job_id = solution.job_id,
                    "Received PushSolution"
                );

                // PushSolution is one-way; no response per spec
                if let Err(e) = jd_server.handle_push_solution(solution).await {
                    error!(
                        client_id,
                        error = %e,
                        "Error processing solution"
                    );
                }
            }

            _ => {
                warn!(
                    client_id,
                    msg_type = frame.msg_type,
                    "Unknown message type"
                );
                // Ignore unknown message types
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn test_config() -> JdServerConfig {
        JdServerConfig {
            token_lifetime: Duration::from_secs(300),
            coinbase_output_max_additional_size: 256,
            pool_payout_script: vec![0x76, 0xa9, 0x14], // P2PKH prefix
            async_mining_allowed: true,
            max_tokens_per_client: 10,
            noise_enabled: false,
            full_template_enabled: false,
            full_template_validation: crate::validation::ValidationLevel::Standard,
            min_pool_payout: 0,
        }
    }

    #[test]
    fn test_jd_server_creation() {
        let config = test_config();
        let payout_tracker = Arc::new(PayoutTracker::default());
        let server = JdServer::new(config.clone(), payout_tracker);

        // Verify we can allocate a token
        let result = server.handle_allocate_token(1, "test-miner");
        assert!(result.is_ok());

        let response = result.unwrap();
        assert_eq!(response.request_id, 1);
        assert!(!response.mining_job_token.is_empty());
        assert_eq!(
            response.coinbase_output_max_additional_size,
            config.coinbase_output_max_additional_size
        );
        assert_eq!(response.coinbase_output, config.pool_payout_script);
        assert!(response.async_mining_allowed);
    }

    #[tokio::test]
    async fn test_declare_job() {
        let config = test_config();
        let payout_tracker = Arc::new(PayoutTracker::default());
        let server = JdServer::new(config, payout_tracker);

        // Set current prev_hash
        let prev_hash = [0xaa; 32];
        server.set_current_prev_hash(prev_hash).await;

        // Allocate a token
        let token_response = server.handle_allocate_token(1, "test-miner").unwrap();

        // Declare a job
        let job_request = SetCustomMiningJob {
            channel_id: 1,
            request_id: 2,
            mining_job_token: token_response.mining_job_token,
            version: 5,
            prev_hash,
            merkle_root: [0xbb; 32],
            block_commitments: [0xcc; 32],
            coinbase_tx: vec![0x01, 0x00, 0x00, 0x00],
            time: 1700000000,
            bits: 0x1d00ffff,
        };

        let result = server.handle_declare_job(job_request).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert_eq!(response.channel_id, 1);
        assert_eq!(response.request_id, 2);
        assert!(response.job_id > 0);
    }

    #[tokio::test]
    async fn test_stale_prev_hash_rejected() {
        let config = test_config();
        let payout_tracker = Arc::new(PayoutTracker::default());
        let server = JdServer::new(config, payout_tracker);

        // Set current prev_hash
        let current_prev_hash = [0xaa; 32];
        server.set_current_prev_hash(current_prev_hash).await;

        // Allocate a token
        let token_response = server.handle_allocate_token(1, "test-miner").unwrap();

        // Try to declare a job with a different (stale) prev_hash
        let stale_prev_hash = [0x11; 32]; // Different from current
        let job_request = SetCustomMiningJob {
            channel_id: 1,
            request_id: 2,
            mining_job_token: token_response.mining_job_token,
            version: 5,
            prev_hash: stale_prev_hash, // Stale!
            merkle_root: [0xbb; 32],
            block_commitments: [0xcc; 32],
            coinbase_tx: vec![0x01, 0x00, 0x00, 0x00],
            time: 1700000000,
            bits: 0x1d00ffff,
        };

        let result = server.handle_declare_job(job_request).await;
        assert!(result.is_err());

        let error = result.unwrap_err();
        assert_eq!(error.error_code, SetCustomMiningJobErrorCode::StalePrevHash);
    }

    #[tokio::test]
    async fn test_invalid_token_rejected() {
        let config = test_config();
        let payout_tracker = Arc::new(PayoutTracker::default());
        let server = JdServer::new(config, payout_tracker);

        // Try to declare a job with an invalid token
        let job_request = SetCustomMiningJob {
            channel_id: 1,
            request_id: 2,
            mining_job_token: vec![0x00, 0x01, 0x02], // Invalid token
            version: 5,
            prev_hash: [0xaa; 32],
            merkle_root: [0xbb; 32],
            block_commitments: [0xcc; 32],
            coinbase_tx: vec![0x01, 0x00, 0x00, 0x00],
            time: 1700000000,
            bits: 0x1d00ffff,
        };

        let result = server.handle_declare_job(job_request).await;
        assert!(result.is_err());

        let error = result.unwrap_err();
        assert_eq!(error.error_code, SetCustomMiningJobErrorCode::InvalidToken);
    }

    #[tokio::test]
    async fn test_empty_coinbase_rejected() {
        let config = test_config();
        let payout_tracker = Arc::new(PayoutTracker::default());
        let server = JdServer::new(config, payout_tracker);

        // Allocate a token
        let token_response = server.handle_allocate_token(1, "test-miner").unwrap();

        // Try to declare a job with empty coinbase
        let job_request = SetCustomMiningJob {
            channel_id: 1,
            request_id: 2,
            mining_job_token: token_response.mining_job_token,
            version: 5,
            prev_hash: [0xaa; 32],
            merkle_root: [0xbb; 32],
            block_commitments: [0xcc; 32],
            coinbase_tx: vec![], // Empty!
            time: 1700000000,
            bits: 0x1d00ffff,
        };

        let result = server.handle_declare_job(job_request).await;
        assert!(result.is_err());

        let error = result.unwrap_err();
        assert_eq!(error.error_code, SetCustomMiningJobErrorCode::InvalidCoinbase);
    }

    #[tokio::test]
    async fn test_push_solution() {
        let config = test_config();
        let payout_tracker = Arc::new(PayoutTracker::default());
        let server = JdServer::new(config, payout_tracker.clone());

        let solution = PushSolution::new(
            1,              // channel_id
            42,             // job_id
            5,              // version
            1700000000,     // time
            [0x11; 32],     // nonce
            [0x22; 1344],   // solution
        );

        let result = server.handle_push_solution(solution).await;
        assert!(result.is_ok());

        // Check that a share was recorded
        let stats = payout_tracker.get_stats(&"jd-miner-1".to_string());
        assert!(stats.is_some());
        assert_eq!(stats.unwrap().total_shares, 1);
    }

    #[test]
    fn test_token_manager_access() {
        let config = test_config();
        let payout_tracker = Arc::new(PayoutTracker::default());
        let server = JdServer::new(config.clone(), payout_tracker);

        let token_manager = server.token_manager();
        let token = token_manager.allocate_token("test-miner").unwrap();
        assert!(!token.token.is_empty());
    }

    #[tokio::test]
    async fn test_multiple_jobs_different_ids() {
        let config = test_config();
        let payout_tracker = Arc::new(PayoutTracker::default());
        let server = JdServer::new(config, payout_tracker);

        // Set current prev_hash
        let prev_hash = [0xaa; 32];
        server.set_current_prev_hash(prev_hash).await;

        // Allocate tokens and declare multiple jobs
        let token1 = server.handle_allocate_token(1, "miner1").unwrap();
        let token2 = server.handle_allocate_token(2, "miner2").unwrap();

        let job1 = SetCustomMiningJob {
            channel_id: 1,
            request_id: 10,
            mining_job_token: token1.mining_job_token,
            version: 5,
            prev_hash,
            merkle_root: [0xbb; 32],
            block_commitments: [0xcc; 32],
            coinbase_tx: vec![0x01],
            time: 1700000000,
            bits: 0x1d00ffff,
        };

        let job2 = SetCustomMiningJob {
            channel_id: 2,
            request_id: 20,
            mining_job_token: token2.mining_job_token,
            version: 5,
            prev_hash,
            merkle_root: [0xdd; 32],
            block_commitments: [0xee; 32],
            coinbase_tx: vec![0x02],
            time: 1700000001,
            bits: 0x1d00ffff,
        };

        let result1 = server.handle_declare_job(job1).await.unwrap();
        let result2 = server.handle_declare_job(job2).await.unwrap();

        // Job IDs should be different
        assert_ne!(result1.job_id, result2.job_id);
    }
}
