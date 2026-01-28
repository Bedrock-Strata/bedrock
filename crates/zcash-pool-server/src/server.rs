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
use crate::session::{ServerMessage, Session, SessionMessage, Transport};
use crate::share::ShareProcessor;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};
use zcash_equihash_validator::VardiffConfig;
use zcash_jd_server::{handle_jd_client_with_transport, JdServer, JdServerConfig, JdTransport};
use zcash_mining_protocol::messages::{NewEquihashJob, ShareResult};
use zcash_stratum_noise::{Keypair, NoiseResponder};
use zcash_stratum_observability::{init_logging, start_metrics_server, LogFormat, PoolMetrics};
use zcash_template_provider::types::BlockTemplate;
use zcash_template_provider::{TemplateProvider, TemplateProviderConfig};
use zcash_mining_protocol::messages::SubmitEquihashShare;
use hex;

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
    /// Channel state (channel_id -> Channel)
    channels: Arc<RwLock<HashMap<u32, Channel>>>,
    /// Channel for session messages
    session_tx: mpsc::Sender<SessionMessage>,
    /// Receiver for session messages
    session_rx: mpsc::Receiver<SessionMessage>,
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
            noise_enabled: config.jd_noise_enabled,
            full_template_enabled: config.jd_full_template_enabled,
            full_template_validation: config.jd_full_template_validation,
            min_pool_payout: config.jd_min_pool_payout,
            ..JdServerConfig::default()
        };
        let jd_server = Arc::new(JdServer::new(jd_config, Arc::clone(&payout_tracker)));

        let jd_listen_addr = config.jd_listen_addr;

        // Create Noise responder if enabled for miner or JD connections
        let noise_responder = if config.noise_enabled || config.jd_noise_enabled {
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
            channels: Arc::new(RwLock::new(HashMap::new())),
            session_tx,
            session_rx,
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
                            if self.config.noise_enabled {
                                if let Some(ref responder) = self.noise_responder {
                                    self.metrics.record_noise_handshake();
                                    match responder.accept(stream).await {
                                        Ok(noise_stream) => {
                                            info!("Noise handshake successful for {}", addr);
                                            if let Err(e) = self.handle_new_connection(Transport::Noise(noise_stream)).await {
                                                self.metrics.record_disconnection();
                                                error!("Error handling new Noise connection: {}", e);
                                            }
                                        }
                                        Err(e) => {
                                            self.metrics.record_noise_handshake_failed();
                                            self.metrics.record_disconnection();
                                            warn!("Noise handshake failed for {}: {}", addr, e);
                                        }
                                    }
                                } else {
                                    warn!("Noise enabled but no responder available; falling back to plaintext");
                                    if let Err(e) = self.handle_new_connection(Transport::Plain(stream)).await {
                                        self.metrics.record_disconnection();
                                        error!("Error handling new connection: {}", e);
                                    }
                                }
                            } else {
                                // No Noise - handle connection directly
                                if let Err(e) = self.handle_new_connection(Transport::Plain(stream)).await {
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
                        let jd_noise_enabled = self.config.jd_noise_enabled;
                        let responder = self.noise_responder.clone();
                        tokio::spawn(async move {
                            let transport = if jd_noise_enabled {
                                if let Some(responder) = responder {
                                    metrics.record_noise_handshake();
                                    match responder.accept(stream).await {
                                        Ok(noise_stream) => JdTransport::Noise(noise_stream),
                                        Err(e) => {
                                            metrics.record_noise_handshake_failed();
                                            warn!("JD Noise handshake failed from {}: {}", addr, e);
                                            metrics.record_jd_disconnection();
                                            return;
                                        }
                                    }
                                } else {
                                    warn!("JD Noise enabled but no responder available; falling back to plaintext");
                                    JdTransport::Plain(stream)
                                }
                            } else {
                                JdTransport::Plain(stream)
                            };

                            if let Err(e) = handle_jd_client_with_transport(transport, jd_server, client_id).await {
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
        transport: Transport,
    ) -> Result<()> {
        // Generate unique nonce_1 for this channel
        let channel_id = Channel::next_id();
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
        let channel = Channel::new_with_id(channel_id, nonce_1, vardiff_config);

        // Create communication channels
        let (server_to_session_tx, server_to_session_rx) = mpsc::channel(1000);

        // Store session sender
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(channel_id, server_to_session_tx.clone());
        }

        // Store channel state
        {
            let mut channels = self.channels.write().await;
            channels.insert(channel_id, channel);
        }

        // Create session
        let session = Session::new(
            transport,
            channel_id,
            self.session_tx.clone(),
            server_to_session_rx,
        );

        // Send initial job if available
        let initial_job = {
            let distributor = self.job_distributor.read().await;
            if distributor.has_template() {
                let mut channels = self.channels.write().await;
                if let Some(channel) = channels.get_mut(&channel_id) {
                    if let Some(job) = distributor.create_job(channel, true) {
                        channel.add_job(job.clone(), true);
                        Some(job)
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        };
        if let Some(job) = initial_job {
            let _ = server_to_session_tx.send(ServerMessage::NewJob(job)).await;
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
        let mut jobs_to_send: Vec<(mpsc::Sender<ServerMessage>, NewEquihashJob)> = Vec::new();

        {
            let mut channels = self.channels.write().await;
            for (&channel_id, sender) in sessions.iter() {
                if let Some(channel) = channels.get_mut(&channel_id) {
                    if let Some(job) = distributor.create_job(channel, clean_jobs) {
                        channel.add_job(job.clone(), clean_jobs);
                        jobs_to_send.push((sender.clone(), job));
                    }
                }
            }
        }

        for (sender, job) in jobs_to_send {
            if sender.send(ServerMessage::NewJob(job)).await.is_ok() {
                broadcast_count += 1;
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
                self.handle_share_submission(channel_id, *share, response_tx)
                    .await
            }
            SessionMessage::Disconnected { channel_id } => {
                info!("Session {} disconnected", channel_id);
                self.sessions.write().await.remove(&channel_id);
                self.channels.write().await.remove(&channel_id);
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
        // Get block target
        let block_target = *self.current_block_target.read().await;

        // Grab the job without holding the lock during validation
        let job = {
            let channels = self.channels.read().await;
            let channel = channels.get(&channel_id).ok_or(PoolError::UnknownChannel(channel_id))?;
            let channel_job = channel
                .get_job(share.job_id)
                .ok_or(PoolError::UnknownJob(share.job_id))?;
            if !channel_job.active {
                return response_tx
                    .send(ShareResult::Rejected(
                        zcash_mining_protocol::messages::RejectReason::StaleJob,
                    ))
                    .map_err(|_| PoolError::ChannelSend)
                    .map(|_| ());
            }
            channel_job.job.clone()
        };

        // Validate share without holding the channel lock
        let result = self.share_processor.validate_share_with_job(
            &share,
            &job,
            self.duplicate_detector.as_ref(),
            &block_target,
        );

        // Apply vardiff update if the share was accepted
        let maybe_new_target = if let Ok(ref validation) = result {
            if validation.accepted {
                let mut channels = self.channels.write().await;
                if let Some(channel) = channels.get_mut(&channel_id) {
                    if channel.record_share().is_some() {
                        Some(channel.current_target())
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

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
                        if let Err(e) = self.submit_block(&job, &share).await {
                            warn!("Failed to submit block for job {}: {}", share.job_id, e);
                        }
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

        // Apply vardiff target update if needed
        if let Some(target) = maybe_new_target {
            if let Some(sender) = self.sessions.read().await.get(&channel_id) {
                let _ = sender
                    .send(ServerMessage::SetTarget { target })
                    .await;
            }
        }

        // Send response back to session
        let _ = response_tx.send(share_result);

        Ok(())
    }

    async fn submit_block(&self, job: &NewEquihashJob, share: &SubmitEquihashShare) -> Result<()> {
        let template = {
            let distributor = self.job_distributor.read().await;
            distributor
                .current_template()
                .ok_or_else(|| PoolError::TemplateProvider("missing current template".to_string()))?
        };

        if template.header.prev_hash.0 != job.prev_hash {
            return Err(PoolError::TemplateProvider(
                "template prev_hash mismatch for solved job".to_string(),
            ));
        }

        let block_bytes = build_block_bytes(job, share, &template)?;
        let block_hex = hex::encode(block_bytes);

        match self.template_provider.submit_block(&block_hex).await {
            Ok(None) => {
                info!("Submitted block for job {} successfully", share.job_id);
            }
            Ok(Some(err)) => {
                warn!("Zebra rejected block for job {}: {}", share.job_id, err);
            }
            Err(e) => {
                return Err(PoolError::TemplateProvider(e.to_string()));
            }
        }

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

fn write_varint(value: u64, out: &mut Vec<u8>) {
    if value < 0xfd {
        out.push(value as u8);
    } else if value <= 0xffff {
        out.push(0xfd);
        out.extend_from_slice(&(value as u16).to_le_bytes());
    } else if value <= 0xffff_ffff {
        out.push(0xfe);
        out.extend_from_slice(&(value as u32).to_le_bytes());
    } else {
        out.push(0xff);
        out.extend_from_slice(&value.to_le_bytes());
    }
}

fn build_block_bytes(
    job: &NewEquihashJob,
    share: &SubmitEquihashShare,
    template: &BlockTemplate,
) -> Result<Vec<u8>> {
    if template.coinbase.is_empty() {
        return Err(PoolError::TemplateProvider(
            "template coinbase is empty".to_string(),
        ));
    }

    let full_nonce = job
        .build_nonce(&share.nonce_2)
        .ok_or_else(|| PoolError::InvalidMessage("Invalid nonce_2 length".to_string()))?;
    let mut header = job.build_header(&full_nonce);
    header[100..104].copy_from_slice(&share.time.to_le_bytes());

    let mut block = Vec::with_capacity(
        header.len()
            + share.solution.len()
            + template.coinbase.len()
            + template.transactions.len() * 100,
    );
    block.extend_from_slice(&header);
    block.extend_from_slice(&share.solution);

    let tx_count = 1 + template.transactions.len() as u64;
    write_varint(tx_count, &mut block);
    block.extend_from_slice(&template.coinbase);

    for tx in &template.transactions {
        let tx_bytes = hex::decode(&tx.data)
            .map_err(|e| PoolError::TemplateProvider(format!("invalid tx data: {}", e)))?;
        block.extend_from_slice(&tx_bytes);
    }

    Ok(block)
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
