//! Template Provider - fetches and manages block templates from Zebra

use crate::error::{Error, Result};
use crate::header::{assemble_header, parse_target};
use crate::rpc::ZebraRpc;
use crate::types::{BlockTemplate, GetBlockTemplateResponse};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tokio::time::{interval, Duration};
use tracing::{debug, error, info, warn};

/// Configuration for the Template Provider
#[derive(Debug, Clone)]
pub struct TemplateProviderConfig {
    /// Zebra RPC URL (e.g., "http://127.0.0.1:8232")
    pub zebra_url: String,
    /// Poll interval in milliseconds
    pub poll_interval_ms: u64,
}

impl Default for TemplateProviderConfig {
    fn default() -> Self {
        Self {
            zebra_url: "http://127.0.0.1:8232".to_string(),
            poll_interval_ms: 1000,
        }
    }
}

/// Template Provider that interfaces with Zebra and pushes templates to subscribers
pub struct TemplateProvider {
    config: TemplateProviderConfig,
    rpc: ZebraRpc,
    template_id: AtomicU64,
    current_template: Arc<RwLock<Option<BlockTemplate>>>,
    sender: broadcast::Sender<BlockTemplate>,
}

impl TemplateProvider {
    /// Create a new Template Provider
    pub fn new(config: TemplateProviderConfig) -> Result<Self> {
        let rpc = ZebraRpc::new(&config.zebra_url, None, None)?;
        let (sender, _) = broadcast::channel(16);

        Ok(Self {
            config,
            rpc,
            template_id: AtomicU64::new(1),
            current_template: Arc::new(RwLock::new(None)),
            sender,
        })
    }

    /// Subscribe to template updates
    pub fn subscribe(&self) -> broadcast::Receiver<BlockTemplate> {
        self.sender.subscribe()
    }

    /// Get the current template
    pub async fn get_current_template(&self) -> Option<BlockTemplate> {
        self.current_template.read().await.clone()
    }

    /// Fetch a new template from Zebra
    pub async fn fetch_template(&self) -> Result<BlockTemplate> {
        let response = self.rpc.get_block_template().await?;
        self.process_template(response)
    }

    /// Process a getblocktemplate response into a BlockTemplate
    fn process_template(&self, response: GetBlockTemplateResponse) -> Result<BlockTemplate> {
        let header = assemble_header(&response)?;
        let target = parse_target(&response.target)?;

        let total_fees: i64 = response.transactions.iter().map(|tx| tx.fee).sum();

        // Parse coinbase transaction
        let coinbase = if let Some(data) = response.coinbase_txn.get("data") {
            if let Some(hex_str) = data.as_str() {
                hex::decode(hex_str).map_err(|e| Error::InvalidTemplate(format!("invalid coinbase: {}", e)))?
            } else {
                vec![]
            }
        } else {
            vec![]
        };

        Ok(BlockTemplate {
            template_id: self.template_id.fetch_add(1, Ordering::SeqCst),
            height: response.height,
            header,
            target,
            transactions: response.transactions,
            coinbase,
            total_fees,
        })
    }

    /// Start the polling loop (call this in a spawned task)
    pub async fn run(&self) -> Result<()> {
        let mut poll_interval = interval(Duration::from_millis(self.config.poll_interval_ms));
        let mut last_prev_hash = String::new();

        info!(
            "Template provider starting, polling {} every {}ms",
            self.config.zebra_url, self.config.poll_interval_ms
        );

        loop {
            poll_interval.tick().await;

            match self.rpc.get_block_template().await {
                Ok(response) => {
                    // Only process if prev_hash changed (new block found)
                    if response.previous_block_hash != last_prev_hash {
                        last_prev_hash = response.previous_block_hash.clone();

                        match self.process_template(response) {
                            Ok(template) => {
                                info!(
                                    "New template: height={}, fees={}",
                                    template.height, template.total_fees
                                );

                                // Update current template
                                *self.current_template.write().await = Some(template.clone());

                                // Broadcast to subscribers
                                if self.sender.send(template).is_err() {
                                    debug!("No active subscribers");
                                }
                            }
                            Err(e) => {
                                error!("Failed to process template: {}", e);
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to fetch template: {}", e);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = TemplateProviderConfig::default();
        assert_eq!(config.zebra_url, "http://127.0.0.1:8232");
        assert_eq!(config.poll_interval_ms, 1000);
    }

    #[test]
    fn test_provider_creation() {
        let config = TemplateProviderConfig::default();
        let provider = TemplateProvider::new(config);
        assert!(provider.is_ok());
    }
}
