//! Fiber relay integration for low-latency block propagation
//!
//! Wraps fiber-zcash library for compact block relay over UDP/FEC.

use std::sync::Arc;

use fiber_zcash::{
    BlockChunker, BlockSender, ClientConfig, CompactBlock,
    PrefilledTx, RelayClient, ShortId, WtxId, AuthDigest, TxId,
};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::config::PoolConfig;
use crate::error::{PoolError, Result};
use zcash_template_provider::types::BlockTemplate;

/// Equihash solution size for Zcash (n=200, k=9)
const EQUIHASH_SOLUTION_SIZE: usize = 1344;

/// Fiber relay wrapper for the pool server
pub struct FiberRelay {
    /// Relay client for sending blocks
    client: Arc<RwLock<RelayClient>>,
    /// Block sender handle
    sender: BlockSender,
    /// Block chunker for manual operations
    #[allow(dead_code)]
    chunker: BlockChunker,
    /// Nonce for short ID computation (use 0 for consistency)
    nonce: u64,
}

impl FiberRelay {
    /// Create a new fiber relay from pool config
    pub fn new(config: &PoolConfig) -> Result<Self> {
        let relay_peers = config.fiber_relay_peers.clone();
        if relay_peers.is_empty() {
            return Err(PoolError::Config("fiber_relay_peers cannot be empty".into()));
        }

        let auth_key = config.fiber_auth_key.unwrap_or([0u8; 32]);

        let client_config = ClientConfig::new(relay_peers, auth_key)
            .with_fec(config.fiber_data_shards, config.fiber_parity_shards)
            .with_bind_addr(config.fiber_bind_addr.unwrap_or_else(|| "0.0.0.0:0".parse().expect("0.0.0.0:0 is a valid address")));

        let client = RelayClient::new(client_config)
            .map_err(|e| PoolError::Config(format!("fiber client creation failed: {}", e)))?;

        let sender = client.sender();

        let chunker = BlockChunker::new(config.fiber_data_shards, config.fiber_parity_shards)
            .map_err(|e| PoolError::Config(format!("fiber chunker creation failed: {}", e)))?;

        Ok(Self {
            client: Arc::new(RwLock::new(client)),
            sender,
            chunker,
            nonce: 0,
        })
    }

    /// Initialize the relay client (bind socket)
    pub async fn init(&self) -> Result<()> {
        let mut client = self.client.write().await;
        client.bind().await
            .map_err(|e| PoolError::Config(format!("fiber bind failed: {}", e)))?;
        info!("Fiber relay bound to {:?}", client.local_addr());
        Ok(())
    }

    /// Start the relay client run loop
    ///
    /// Returns a handle that can be used to stop the client.
    pub async fn start(&self) -> Result<()> {
        let mut client = self.client.write().await;
        // Take the receiver to allow the run loop to work
        if client.take_receiver().is_none() {
            warn!("Fiber relay receiver already taken");
        }
        Ok(())
    }

    /// Announce a new block template to the relay network
    pub async fn announce_template(&self, template: &BlockTemplate) -> Result<()> {
        let compact = self.build_compact_block_from_template(template)?;

        self.sender.send(compact).await
            .map_err(|e| PoolError::Config(format!("fiber send failed: {}", e)))?;

        debug!(
            height = template.height,
            tx_count = template.transactions.len(),
            "Announced compact block to fiber relay"
        );
        Ok(())
    }

    /// Announce a found block to the relay network
    pub async fn announce_block(&self, block_header: &[u8], coinbase: &[u8], tx_hashes: &[[u8; 32]]) -> Result<()> {
        // Build minimal compact block with just header and coinbase prefilled
        let prefilled = vec![PrefilledTx {
            index: 0,
            tx_data: coinbase.to_vec(),
        }];

        // Build short IDs for non-coinbase transactions
        let header_hash = self.compute_header_hash(block_header);
        let short_ids: Vec<ShortId> = tx_hashes.iter()
            .map(|hash| {
                let txid = TxId::from_bytes(*hash);
                let wtxid = WtxId::new(txid, AuthDigest::from_bytes([0u8; 32]));
                ShortId::compute(&wtxid, &header_hash, self.nonce)
            })
            .collect();

        let compact = CompactBlock::new(
            block_header.to_vec(),
            self.nonce,
            short_ids,
            prefilled,
        );

        self.sender.send(compact).await
            .map_err(|e| PoolError::Config(format!("fiber send failed: {}", e)))?;

        info!("Announced found block to fiber relay");
        Ok(())
    }

    /// Build a CompactBlock from a BlockTemplate
    fn build_compact_block_from_template(&self, template: &BlockTemplate) -> Result<CompactBlock> {
        // Serialize the full header (140 bytes header + equihash solution placeholder)
        let header_bytes = template.header.serialize();

        // For templates, we include a placeholder solution (zeros)
        // The actual solution will be filled when a block is found
        let mut full_header = Vec::with_capacity(1487);
        full_header.extend_from_slice(&header_bytes);
        // Add compactSize for solution length (1344 bytes = 0xfd 0x40 0x05)
        full_header.push(0xfd);
        full_header.extend_from_slice(&(EQUIHASH_SOLUTION_SIZE as u16).to_le_bytes());
        // Add placeholder solution
        full_header.extend(std::iter::repeat(0u8).take(EQUIHASH_SOLUTION_SIZE));

        let header_hash = self.compute_header_hash(&full_header);

        // Prefill coinbase
        let prefilled = vec![PrefilledTx {
            index: 0,
            tx_data: template.coinbase.clone(),
        }];

        // Build short IDs for template transactions
        let short_ids: Vec<ShortId> = template.transactions.iter()
            .filter_map(|tx| {
                match hex::decode(&tx.hash) {
                    Ok(hash_bytes) if hash_bytes.len() == 32 => {
                        let mut txid_bytes = [0u8; 32];
                        txid_bytes.copy_from_slice(&hash_bytes);
                        txid_bytes.reverse();
                        let txid = TxId::from_bytes(txid_bytes);
                        let wtxid = WtxId::new(txid, AuthDigest::from_bytes([0u8; 32]));
                        Some(ShortId::compute(&wtxid, &header_hash, self.nonce))
                    }
                    _ => {
                        warn!(tx_hash = %tx.hash, "Failed to decode transaction hash, skipping");
                        None
                    }
                }
            })
            .collect();

        Ok(CompactBlock::new(
            full_header,
            self.nonce,
            short_ids,
            prefilled,
        ))
    }

    /// Compute double-SHA256 header hash
    fn compute_header_hash(&self, header: &[u8]) -> [u8; 32] {
        use sha2::{Digest, Sha256};
        let first = Sha256::digest(header);
        let second = Sha256::digest(first);
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&second);
        hash
    }
}
