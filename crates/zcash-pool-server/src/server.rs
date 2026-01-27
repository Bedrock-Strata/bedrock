//! Main pool server orchestration
//!
//! Coordinates template provider, sessions, job distribution, and share processing.
//!
//! ## Known Limitations (MVP)
//!
//! - **Vardiff state**: Per-channel vardiff state is owned by Session tasks. The server
//!   creates temporary Channel instances for job creation which don't preserve vardiff
//!   history. A production implementation would add a ChannelManager that maintains
//!   persistent channel state server-side.
//!
//! - **Block target**: Currently uses a simplified block target. Production would
//!   extract the actual target from the template.

use crate::channel::Channel;
use crate::config::PoolConfig;
use crate::duplicate::{DuplicateDetector, InMemoryDuplicateDetector};
use crate::error::{PoolError, Result};
use crate::job::JobDistributor;
use crate::payout::{MinerId, PayoutTracker};
use crate::session::{ServerMessage, Session, SessionMessage};
use crate::share::ShareProcessor;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};
use zcash_equihash_validator::VardiffConfig;
use zcash_jd_server::{handle_jd_client, JdServer, JdServerConfig};
use zcash_stratum_noise::{Keypair, NoiseResponder};
use zcash_stratum_observability::{init_logging, start_metrics_server, LogFormat, PoolMetrics};
use zcash_template_provider::types::BlockTemplate;
use zcash_template_provider::{TemplateProvider, TemplateProviderConfig};

/// Main pool server
pub struct PoolServer {
    /// Server configuration
    config: PoolConfig,
    /// Template provider for fetching block templates
    template_provider: Arc<TemplateProvider>,
    /// Job distributor (creates jobs from templates)
    job_distributor: Arc<RwLock<JobDistributor>>,
    /// Share processor (validates shares)
    share_processor: Arc<ShareProcessor>,
    /// Duplicate share detector
    duplicate_detector: Arc<InMemoryDuplicateDetector>,
    /// PPS payout tracker
    payout_tracker: Arc<PayoutTracker>,
    /// Active sessions (channel_id -> sender)
    sessions: Arc<RwLock<HashMap<u32, mpsc::Sender<ServerMessage>>>>,
    /// Channel for session messages
    session_tx: mpsc::Sender<SessionMessage>,
    /// Receiver for session messages
    session_rx: mpsc::Receiver<SessionMessage>,
    /// Channel ID counter for nonce generation
    next_channel_id: AtomicU32,
    /// Current block target (for checking block finds)
    current_block_target: Arc<RwLock<[u8; 32]>>,
    /// JD Server (embedded, optional)
    jd_server: Arc<JdServer>,
    /// JD Server listen address (None = disabled)
    jd_listen_addr: Option<SocketAddr>,
    /// Noise responder for encrypted connections (None = disabled)
    noise_responder: Option<Arc<NoiseResponder>>,
    /// Pool metrics
    metrics: Arc<PoolMetrics>,
}

impl PoolServer {
    /// Create a new pool server
    pub fn new(config: PoolConfig) -> Result<Self> {
        // Initialize logging based on config
        let log_format = if config.json_logging {
            LogFormat::Json
        } else {
            LogFormat::Pretty
        };
        init_logging(log_format, "info");

        // Create metrics
        let metrics = Arc::new(PoolMetrics::new());

        // Create template provider
        let tp_config = TemplateProviderConfig {
            zebra_url: config.zebra_url.clone(),
            poll_interval_ms: config.template_poll_ms,
        };
        let template_provider = TemplateProvider::new(tp_config)
            .map_err(|e| PoolError::TemplateProvider(e.to_string()))?;

        // Create session channel (buffered to handle bursts)
        let (session_tx, session_rx) = mpsc::channel(10000);

        // Create payout tracker (shared with JD server)
        let payout_tracker = Arc::new(PayoutTracker::default());

        // Create JD Server with shared payout tracker
        let jd_config = JdServerConfig {
            pool_payout_script: config.pool_payout_script.clone().unwrap_or_default(),
            ..JdServerConfig::default()
        };
        let jd_server = Arc::new(JdServer::new(jd_config, Arc::clone(&payout_tracker)));

        let jd_listen_addr = config.jd_listen_addr;

        // Create Noise responder if enabled
        let noise_responder = if config.noise_enabled {
            let keypair = if let Some(ref key_path) = config.noise_private_key_path {
                // Load keypair from file
                let key_hex = std::fs::read_to_string(key_path)
                    .map_err(|e| PoolError::Config(format!("Failed to read noise key file: {}", e)))?;
                Keypair::from_private_hex(key_hex.trim())
                    .map_err(|e| PoolError::Config(format!("Invalid noise private key: {}", e)))?
            } else {
                // Generate a new keypair
                let kp = Keypair::generate();
                info!(
                    "Generated new Noise keypair. Public key: {}",
                    kp.public.to_hex()
                );
                info!("To persist this key, save the private key to a file and set noise_private_key_path");
                kp
            };
            Some(Arc::new(NoiseResponder::new(keypair)))
        } else {
            None
        };

        Ok(Self {
            config,
            template_provider: Arc::new(template_provider),
            job_distributor: Arc::new(RwLock::new(JobDistributor::new())),
            share_processor: Arc::new(ShareProcessor::new()),
            duplicate_detector: Arc::new(InMemoryDuplicateDetector::new()),
            payout_tracker,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            session_tx,
            session_rx,
            next_channel_id: AtomicU32::new(1),
            current_block_target: Arc::new(RwLock::new([0xff; 32])),
            jd_server,
            jd_listen_addr,
            noise_responder,
            metrics,
        })
    }

    /// Run the pool server
    pub async fn run(mut self) -> Result<()> {
        // Start metrics server if configured
        if let Some(metrics_addr) = self.config.metrics_addr {
            let metrics = Arc::clone(&self.metrics);
            tokio::spawn(async move {
                start_metrics_server(metrics_addr, metrics).await;
            });
        }

        // Bind to listen address
        let listener = TcpListener::bind(&self.config.listen_addr).await?;
        info!("Pool server listening on {}", self.config.listen_addr);

        // Optionally bind JD listener
        let jd_listener = if let Some(addr) = self.jd_listen_addr {
            let listener = TcpListener::bind(addr).await?;
            info!("JD Server listening on {}", addr);
            Some(listener)
        } else {
            info!("JD Server disabled (no jd_listen_addr configured)");
            None
        };

        // Subscribe to template updates
        let mut template_rx = self.template_provider.subscribe();

        // Spawn template provider task
        let tp = Arc::clone(&self.template_provider);
        tokio::spawn(async move {
            if let Err(e) = tp.run().await {
                error!("Template provider error: {}", e);
            }
        });

        // Spawn periodic stats logging
        let payout_tracker = Arc::clone(&self.payout_tracker);
        let sessions = Arc::clone(&self.sessions);
        let metrics = Arc::clone(&self.metrics);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                let session_count = sessions.read().await.len();
                let active_miners = payout_tracker.active_miner_count();
                let hashrate = payout_tracker.estimate_pool_hashrate();

                // Update metrics
                metrics.set_hashrate(hashrate);

                info!(
                    "Pool stats: {} connections, {} active miners, {:.2} H/s",
                    session_count, active_miners, hashrate
                );
            }
        });

        // Main event loop
        loop {
            tokio::select! {
                // Accept new miner connections
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((stream, addr)) => {
                            info!("New connection from {}", addr);

                            // Check connection limit
                            let current_connections = self.sessions.read().await.len();
                            if current_connections >= self.config.max_connections {
                                warn!("Connection limit reached, rejecting {}", addr);
                                continue;
                            }

                            // Track connection in metrics
                            self.metrics.record_connection();

                            // Handle Noise handshake if enabled
                            if let Some(ref responder) = self.noise_responder {
                                self.metrics.record_noise_handshake();
                                let responder = Arc::clone(responder);
                                let metrics = Arc::clone(&self.metrics);

                                tokio::spawn(async move {
                                    match responder.accept(stream).await {
                                        Ok(_noise_stream) => {
                                            // TODO: Use noise_stream instead of raw stream
                                            // For now, we just log success; actual encrypted transport
                                            // requires changes to Session to use NoiseStream
                                            info!("Noise handshake successful for {}", addr);
                                            // Note: Full integration would require Session to work with NoiseStream
                                            // This is a placeholder showing the handshake works
                                        }
                                        Err(e) => {
                                            metrics.record_noise_handshake_failed();
                                            metrics.record_disconnection();
                                            warn!("Noise handshake failed for {}: {}", addr, e);
                                        }
                                    }
                                });
                            } else {
                                // No Noise - handle connection directly
                                if let Err(e) = self.handle_new_connection(stream).await {
                                    self.metrics.record_disconnection();
                                    error!("Error handling new connection: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            error!("Accept error: {}", e);
                        }
                    }
                }

                // Accept JD client connections (if JD server is enabled)
                jd_accept_result = async {
                    if let Some(ref listener) = jd_listener {
                        listener.accept().await
                    } else {
                        // If JD is disabled, this branch never completes
                        std::future::pending().await
                    }
                } => {
                    if let Ok((stream, addr)) = jd_accept_result {
                        info!("New JD client from {}", addr);
                        self.metrics.record_jd_connection();
                        let jd_server = Arc::clone(&self.jd_server);
                        let metrics = Arc::clone(&self.metrics);
                        let client_id = format!("jd_{}", addr);
                        tokio::spawn(async move {
                            if let Err(e) = handle_jd_client(stream, jd_server, client_id).await {
                                warn!("JD client error: {}", e);
                            }
                            metrics.record_jd_disconnection();
                        });
                    }
                }

                // Handle template updates
                template_result = template_rx.recv() => {
                    match template_result {
                        Ok(template) => {
                            if let Err(e) = self.handle_new_template(template).await {
                                error!("Error handling template: {}", e);
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            warn!("Missed {} template updates", n);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            error!("Template provider channel closed");
                            return Err(PoolError::TemplateProvider("channel closed".to_string()));
                        }
                    }
                }

                // Handle session messages
                Some(msg) = self.session_rx.recv() => {
                    if let Err(e) = self.handle_session_message(msg).await {
                        error!("Error handling session message: {}", e);
                    }
                }
            }
        }
    }

    /// Handle a new miner connection
    async fn handle_new_connection(
        &self,
        stream: tokio::net::TcpStream,
    ) -> Result<()> {
        // Generate unique nonce_1 for this channel
        let channel_id = self.next_channel_id.fetch_add(1, Ordering::SeqCst);
        let nonce_1 = Channel::generate_nonce_1(channel_id, self.config.nonce_1_len);

        // Create vardiff config
        let vardiff_config = VardiffConfig {
            target_shares_per_minute: self.config.target_shares_per_minute,
            min_difficulty: self.config.initial_difficulty,
            max_difficulty: 1e12,
            retarget_interval: Duration::from_secs(90),
            variance_tolerance: 0.25,
        };

        // Create channel
        let channel = Channel::new(nonce_1, vardiff_config);
        let channel_id = channel.id;

        // Create communication channels
        let (server_to_session_tx, server_to_session_rx) = mpsc::channel(1000);

        // Store session sender
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(channel_id, server_to_session_tx.clone());
        }

        // Create session
        let session = Session::new(
            stream,
            channel,
            self.session_tx.clone(),
            server_to_session_rx,
        );

        // Send initial job if available
        {
            let distributor = self.job_distributor.read().await;
            if distributor.has_template() {
                // We need mutable access to channel for creating job, but session owns it
                // Instead, create the job here and send it
                let temp_channel = Channel::new(
                    Channel::generate_nonce_1(channel_id, self.config.nonce_1_len),
                    VardiffConfig::default(),
                );
                if let Some(job) = distributor.create_job(&temp_channel, true) {
                    let _ = server_to_session_tx.send(ServerMessage::NewJob(job)).await;
                }
            }
        }

        // Spawn session task
        let sessions = Arc::clone(&self.sessions);
        let metrics = Arc::clone(&self.metrics);
        tokio::spawn(async move {
            if let Err(e) = session.run().await {
                debug!("Session {} ended: {}", channel_id, e);
            }
            // Clean up session on exit
            sessions.write().await.remove(&channel_id);
            metrics.record_disconnection();
        });

        info!("Session {} started", channel_id);
        Ok(())
    }

    /// Handle a new block template
    async fn handle_new_template(&self, template: BlockTemplate) -> Result<()> {
        let height = template.height;
        info!("New template at height {}", height);

        // Update block target
        {
            let mut target = self.current_block_target.write().await;
            *target = template.target.0;
        }

        // Update JD Server's current prev_hash (for stale detection)
        self.jd_server
            .set_current_prev_hash(template.header.prev_hash.0)
            .await;

        // Update job distributor
        let is_new_block = {
            let mut distributor = self.job_distributor.write().await;
            distributor.update_template(template)
        };

        // Clear duplicate detector on new block
        if is_new_block {
            self.duplicate_detector.clear_all();
            info!("New block detected, cleared duplicate detector");
        }

        // Broadcast jobs to all sessions
        self.broadcast_jobs(is_new_block).await?;

        Ok(())
    }

    /// Broadcast new jobs to all connected sessions
    async fn broadcast_jobs(&self, clean_jobs: bool) -> Result<()> {
        let sessions = self.sessions.read().await;
        let distributor = self.job_distributor.read().await;

        if !distributor.has_template() {
            return Ok(());
        }

        let mut broadcast_count = 0;
        for (&channel_id, sender) in sessions.iter() {
            // Create a temporary channel for job creation
            // Note: In production, we'd want to track channels separately
            let temp_channel = Channel::new(
                Channel::generate_nonce_1(channel_id, self.config.nonce_1_len),
                VardiffConfig::default(),
            );

            if let Some(job) = distributor.create_job(&temp_channel, clean_jobs) {
                if sender.send(ServerMessage::NewJob(job)).await.is_ok() {
                    broadcast_count += 1;
                }
            }
        }

        debug!(
            "Broadcast job to {} sessions (clean={})",
            broadcast_count, clean_jobs
        );
        Ok(())
    }

    /// Handle a message from a session
    async fn handle_session_message(&self, msg: SessionMessage) -> Result<()> {
        match msg {
            SessionMessage::ShareSubmitted {
                channel_id,
                share,
                response_tx,
            } => {
                self.handle_share_submission(channel_id, share, response_tx)
                    .await
            }
            SessionMessage::Disconnected { channel_id } => {
                info!("Session {} disconnected", channel_id);
                self.sessions.write().await.remove(&channel_id);
                Ok(())
            }
        }
    }

    /// Handle a share submission
    async fn handle_share_submission(
        &self,
        channel_id: u32,
        share: zcash_mining_protocol::messages::SubmitEquihashShare,
        response_tx: tokio::sync::oneshot::Sender<zcash_mining_protocol::messages::ShareResult>,
    ) -> Result<()> {
        // Create a temporary channel for validation
        // Note: In production, we'd track actual channel state
        let channel = Channel::new(
            Channel::generate_nonce_1(channel_id, self.config.nonce_1_len),
            VardiffConfig::default(),
        );

        // Get block target
        let block_target = *self.current_block_target.read().await;

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
                    // Track payout
                    if let Some(difficulty) = validation.difficulty {
                        let miner_id: MinerId = format!("channel_{}", channel_id);
                        self.payout_tracker.record_share(&miner_id, difficulty);
                    }

                    // Check for block find
                    if validation.is_block {
                        info!(
                            "BLOCK FOUND by channel {}! Job: {}, Height: {:?}",
                            channel_id,
                            share.job_id,
                            self.job_distributor.read().await.current_height()
                        );
                        // TODO: Submit block to network
                    }

                    debug!(
                        "Share accepted from channel {} (diff: {:?})",
                        channel_id, validation.difficulty
                    );
                }

                validation.result
            }
            Err(e) => {
                warn!("Share validation error: {}", e);
                zcash_mining_protocol::messages::ShareResult::Rejected(
                    zcash_mining_protocol::messages::RejectReason::InvalidSolution,
                )
            }
        };

        // Send response back to session
        let _ = response_tx.send(share_result);

        Ok(())
    }

    /// Get the number of active sessions
    pub async fn session_count(&self) -> usize {
        self.sessions.read().await.len()
    }

    /// Get payout tracker reference
    pub fn payout_tracker(&self) -> &Arc<PayoutTracker> {
        &self.payout_tracker
    }

    /// Get current pool stats
    pub async fn get_stats(&self) -> PoolStats {
        let sessions = self.session_count().await;
        let active_miners = self.payout_tracker.active_miner_count();
        let hashrate = self.payout_tracker.estimate_pool_hashrate();
        let current_height = self.job_distributor.read().await.current_height();

        PoolStats {
            connected_sessions: sessions,
            active_miners,
            estimated_hashrate: hashrate,
            current_height,
        }
    }
}

/// Pool statistics
#[derive(Debug, Clone)]
pub struct PoolStats {
    /// Number of connected sessions
    pub connected_sessions: usize,
    /// Number of active miners (submitted shares recently)
    pub active_miners: usize,
    /// Estimated pool hashrate
    pub estimated_hashrate: f64,
    /// Current block height
    pub current_height: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_server_creation() {
        let config = PoolConfig::default();
        let server = PoolServer::new(config);
        assert!(server.is_ok());
    }

    #[test]
    fn test_pool_stats_default() {
        let stats = PoolStats {
            connected_sessions: 0,
            active_miners: 0,
            estimated_hashrate: 0.0,
            current_height: None,
        };
        assert_eq!(stats.connected_sessions, 0);
    }
}
