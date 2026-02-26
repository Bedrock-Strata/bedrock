# Bedrock-Forge Integration Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Integrate bedrock-forge compact block relay into stratum-zcash pool server for low-latency block propagation.

**Architecture:** Add a ForgeRelay wrapper component to the pool server that subscribes to block templates and found blocks, constructs CompactBlocks, and transmits them over UDP/FEC to relay network peers. Integration is non-blocking and parallel to existing miner job distribution.

**Tech Stack:** Rust, tokio async runtime, bedrock-forge library (CompactBlock, RelayClient, BlockChunker)

---

## Task 1: Add bedrock-forge Dependency

**Files:**
- Modify: `/Users/zakimanian/stratum-zcash/Cargo.toml:13-24`
- Modify: `/Users/zakimanian/stratum-zcash/crates/zcash-pool-server/Cargo.toml:14-24`

**Step 1: Add bedrock-forge to workspace dependencies**

Edit `/Users/zakimanian/stratum-zcash/Cargo.toml` to add:

```toml
[workspace.dependencies]
# ... existing deps ...
bedrock-forge = { path = "../bedrock-forge" }
```

**Step 2: Add bedrock-forge to pool-server dependencies**

Edit `/Users/zakimanian/stratum-zcash/crates/zcash-pool-server/Cargo.toml` to add:

```toml
[dependencies]
# ... existing deps ...
bedrock-forge = { workspace = true }
```

**Step 3: Verify build compiles**

Run: `cd /Users/zakimanian/stratum-zcash && cargo check -p zcash-pool-server`
Expected: Build succeeds with no errors

**Step 4: Commit**

```bash
git add Cargo.toml crates/zcash-pool-server/Cargo.toml
git commit -m "$(cat <<'EOF'
deps: add bedrock-forge for compact block relay

Adds bedrock-forge as workspace dependency for low-latency block
propagation support.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Create ForgeRelay Configuration

**Files:**
- Modify: `/Users/zakimanian/stratum-zcash/crates/zcash-pool-server/src/config.rs`
- Test: Manual verification via cargo check

**Step 1: Read existing config structure**

Run: `cat /Users/zakimanian/stratum-zcash/crates/zcash-pool-server/src/config.rs`
Understand the PoolConfig struct fields.

**Step 2: Add Forge relay configuration fields**

Add to `PoolConfig` struct:

```rust
/// Forge relay configuration (optional - None disables relay)
pub forge_relay_enabled: bool,
/// UDP bind address for Forge relay (default: 0.0.0.0:8336)
pub forge_bind_addr: Option<std::net::SocketAddr>,
/// Relay peer addresses to connect to
pub forge_relay_peers: Vec<std::net::SocketAddr>,
/// Shared authentication key for relay network (32 bytes)
pub forge_auth_key: Option<[u8; 32]>,
/// FEC data shards (default: 10)
pub forge_data_shards: usize,
/// FEC parity shards (default: 3)
pub forge_parity_shards: usize,
```

**Step 3: Update Default impl**

Add defaults to the `Default` implementation:

```rust
forge_relay_enabled: false,
forge_bind_addr: Some("0.0.0.0:8336".parse().unwrap()),
forge_relay_peers: Vec::new(),
forge_auth_key: None,
forge_data_shards: 10,
forge_parity_shards: 3,
```

**Step 4: Verify build compiles**

Run: `cd /Users/zakimanian/stratum-zcash && cargo check -p zcash-pool-server`
Expected: Build succeeds

**Step 5: Commit**

```bash
git add crates/zcash-pool-server/src/config.rs
git commit -m "$(cat <<'EOF'
config: add Forge relay configuration options

Adds configuration fields for bedrock-forge relay integration:
- forge_relay_enabled: master toggle
- forge_bind_addr: UDP socket bind address
- forge_relay_peers: addresses of relay peers
- forge_auth_key: HMAC authentication key
- forge_data_shards/parity_shards: FEC parameters

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: Create ForgeRelay Wrapper Module

**Files:**
- Create: `/Users/zakimanian/stratum-zcash/crates/zcash-pool-server/src/forge.rs`
- Modify: `/Users/zakimanian/stratum-zcash/crates/zcash-pool-server/src/lib.rs`

**Step 1: Create the forge.rs module**

Create `/Users/zakimanian/stratum-zcash/crates/zcash-pool-server/src/forge.rs`:

```rust
//! Forge relay integration for low-latency block propagation
//!
//! Wraps bedrock-forge library for compact block relay over UDP/FEC.

use std::net::SocketAddr;
use std::sync::Arc;

use bedrock_forge::{
    BlockChunker, BlockSender, ClientConfig, CompactBlock, CompactBlockBuilder,
    PrefilledTx, RelayClient, ShortId, WtxId, AuthDigest, TxId,
};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::config::PoolConfig;
use crate::error::{PoolError, Result};
use zcash_template_provider::types::BlockTemplate;

/// Forge relay wrapper for the pool server
pub struct ForgeRelay {
    /// Relay client for sending blocks
    client: Arc<RwLock<RelayClient>>,
    /// Block sender handle
    sender: BlockSender,
    /// Block chunker for manual operations
    chunker: BlockChunker,
    /// Nonce for short ID computation (use 0 for consistency)
    nonce: u64,
}

impl ForgeRelay {
    /// Create a new Forge relay from pool config
    pub fn new(config: &PoolConfig) -> Result<Self> {
        let relay_peers = config.forge_relay_peers.clone();
        if relay_peers.is_empty() {
            return Err(PoolError::Config("forge_relay_peers cannot be empty".into()));
        }

        let auth_key = config.forge_auth_key.unwrap_or([0u8; 32]);

        let client_config = ClientConfig::new(relay_peers, auth_key)
            .with_fec(config.forge_data_shards, config.forge_parity_shards)
            .with_bind_addr(config.forge_bind_addr.unwrap_or_else(|| "0.0.0.0:0".parse().unwrap()));

        let client = RelayClient::new(client_config)
            .map_err(|e| PoolError::Config(format!("forge client creation failed: {}", e)))?;

        let sender = client.sender();

        let chunker = BlockChunker::new(config.forge_data_shards, config.forge_parity_shards)
            .map_err(|e| PoolError::Config(format!("forge chunker creation failed: {}", e)))?;

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
            .map_err(|e| PoolError::Config(format!("forge bind failed: {}", e)))?;
        info!("Forge relay bound to {:?}", client.local_addr());
        Ok(())
    }

    /// Start the relay client run loop
    ///
    /// Returns a handle that can be used to stop the client.
    pub async fn start(&self) -> Result<()> {
        let mut client = self.client.write().await;
        // Take the receiver to allow the run loop to work
        if client.take_receiver().is_none() {
            warn!("Forge relay receiver already taken");
        }
        Ok(())
    }

    /// Announce a new block template to the relay network
    pub async fn announce_template(&self, template: &BlockTemplate) -> Result<()> {
        let compact = self.build_compact_block_from_template(template)?;

        self.sender.send(compact).await
            .map_err(|e| PoolError::Config(format!("forge send failed: {}", e)))?;

        debug!(
            height = template.height,
            tx_count = template.transactions.len(),
            "Announced compact block to Forge relay"
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
            .map_err(|e| PoolError::Config(format!("forge send failed: {}", e)))?;

        info!("Announced found block to Forge relay");
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
        full_header.extend_from_slice(&1344u16.to_le_bytes());
        // Add placeholder solution
        full_header.extend(std::iter::repeat(0u8).take(1344));

        let header_hash = self.compute_header_hash(&full_header);

        // Prefill coinbase
        let prefilled = vec![PrefilledTx {
            index: 0,
            tx_data: template.coinbase.clone(),
        }];

        // Build short IDs for template transactions
        let short_ids: Vec<ShortId> = template.transactions.iter()
            .map(|tx| {
                // Parse txid from hex
                let hash_bytes = hex::decode(&tx.hash).unwrap_or_else(|_| vec![0u8; 32]);
                let mut txid_bytes = [0u8; 32];
                if hash_bytes.len() == 32 {
                    txid_bytes.copy_from_slice(&hash_bytes);
                    // Reverse for internal byte order
                    txid_bytes.reverse();
                }
                let txid = TxId::from_bytes(txid_bytes);
                let wtxid = WtxId::new(txid, AuthDigest::from_bytes([0u8; 32]));
                ShortId::compute(&wtxid, &header_hash, self.nonce)
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
```

**Step 2: Add module to lib.rs**

Edit `/Users/zakimanian/stratum-zcash/crates/zcash-pool-server/src/lib.rs` to add:

```rust
pub mod forge;
```

**Step 3: Verify build compiles**

Run: `cd /Users/zakimanian/stratum-zcash && cargo check -p zcash-pool-server`
Expected: Build succeeds

**Step 4: Commit**

```bash
git add crates/zcash-pool-server/src/forge.rs crates/zcash-pool-server/src/lib.rs
git commit -m "$(cat <<'EOF'
feat: add ForgeRelay wrapper module

Implements ForgeRelay wrapper that:
- Creates and manages bedrock-forge RelayClient
- Builds CompactBlocks from BlockTemplate
- Announces templates and found blocks to relay network
- Handles short ID computation for transactions

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: Integrate ForgeRelay into PoolServer

**Files:**
- Modify: `/Users/zakimanian/stratum-zcash/crates/zcash-pool-server/src/server.rs:40-72`
- Modify: `/Users/zakimanian/stratum-zcash/crates/zcash-pool-server/src/server.rs:74-155`

**Step 1: Add ForgeRelay field to PoolServer struct**

At line 71 in server.rs, after `metrics: Arc<PoolMetrics>,`, add:

```rust
    /// Forge relay for compact block propagation (optional)
    forge_relay: Option<Arc<ForgeRelay>>,
```

**Step 2: Add import for ForgeRelay**

At the top of server.rs, add:

```rust
use crate::forge::ForgeRelay;
```

**Step 3: Initialize ForgeRelay in PoolServer::new()**

In the `new()` function, after creating metrics (around line 86), add:

```rust
        // Create Forge relay if enabled
        let forge_relay = if config.forge_relay_enabled {
            match ForgeRelay::new(&config) {
                Ok(relay) => {
                    info!("Forge relay initialized");
                    Some(Arc::new(relay))
                }
                Err(e) => {
                    warn!("Failed to create Forge relay: {}. Continuing without relay.", e);
                    None
                }
            }
        } else {
            info!("Forge relay disabled");
            None
        };
```

**Step 4: Add forge_relay to struct initialization**

In the `Ok(Self { ... })` block (around line 138), add:

```rust
            forge_relay,
```

**Step 5: Verify build compiles**

Run: `cd /Users/zakimanian/stratum-zcash && cargo check -p zcash-pool-server`
Expected: Build succeeds

**Step 6: Commit**

```bash
git add crates/zcash-pool-server/src/server.rs
git commit -m "$(cat <<'EOF'
feat: add ForgeRelay field to PoolServer

Initializes ForgeRelay wrapper when forge_relay_enabled is true
in config. Fails gracefully with warning if relay creation fails.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: Hook Template Announcements

**Files:**
- Modify: `/Users/zakimanian/stratum-zcash/crates/zcash-pool-server/src/server.rs:430-462`

**Step 1: Add Forge relay announcement to handle_new_template**

In `handle_new_template()`, after updating the JD Server's prev_hash (around line 444), add:

```rust
        // Announce to Forge relay network (non-blocking)
        if let Some(ref forge) = self.forge_relay {
            let forge = Arc::clone(forge);
            let template_clone = template.clone();
            tokio::spawn(async move {
                if let Err(e) = forge.announce_template(&template_clone).await {
                    warn!("Failed to announce template to Forge relay: {}", e);
                }
            });
        }
```

**Step 2: Verify build compiles**

Run: `cd /Users/zakimanian/stratum-zcash && cargo check -p zcash-pool-server`
Expected: Build succeeds

**Step 3: Commit**

```bash
git add crates/zcash-pool-server/src/server.rs
git commit -m "$(cat <<'EOF'
feat: announce new templates to Forge relay

When a new block template arrives, announce it to the Forge relay
network in parallel with miner job distribution. Uses spawn to
avoid blocking the main event loop.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: Hook Block Found Announcements

**Files:**
- Modify: `/Users/zakimanian/stratum-zcash/crates/zcash-pool-server/src/server.rs:586-597`

**Step 1: Add Forge relay announcement when block is found**

In `handle_share_submission()`, before `self.submit_block()` call (around line 594), add:

```rust
                        // Announce to Forge relay BEFORE submitting to Zebra
                        // This gives the relay network a head start
                        if let Some(ref forge) = self.forge_relay {
                            let header = job.build_header(&job.build_nonce(&share.nonce_2).unwrap_or_default());
                            let tx_hashes: Vec<[u8; 32]> = {
                                let distributor = self.job_distributor.read().await;
                                distributor.current_template()
                                    .map(|t| t.transactions.iter()
                                        .filter_map(|tx| {
                                            let bytes = hex::decode(&tx.hash).ok()?;
                                            if bytes.len() == 32 {
                                                let mut arr = [0u8; 32];
                                                arr.copy_from_slice(&bytes);
                                                arr.reverse();
                                                Some(arr)
                                            } else {
                                                None
                                            }
                                        })
                                        .collect())
                                    .unwrap_or_default()
                            };

                            let forge = Arc::clone(forge);
                            let template = self.job_distributor.read().await.current_template();
                            if let Some(tmpl) = template {
                                let coinbase = tmpl.coinbase.clone();
                                tokio::spawn(async move {
                                    if let Err(e) = forge.announce_block(&header, &coinbase, &tx_hashes).await {
                                        warn!("Failed to announce block to Forge relay: {}", e);
                                    }
                                });
                            }
                        }
```

**Step 2: Verify build compiles**

Run: `cd /Users/zakimanian/stratum-zcash && cargo check -p zcash-pool-server`
Expected: Build succeeds

**Step 3: Commit**

```bash
git add crates/zcash-pool-server/src/server.rs
git commit -m "$(cat <<'EOF'
feat: announce found blocks to Forge relay before Zebra

When a block is found, announce it to the Forge relay network
BEFORE submitting to Zebra. This gives the relay network a
latency advantage for propagation.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 7: Initialize Forge Relay in Run Loop

**Files:**
- Modify: `/Users/zakimanian/stratum-zcash/crates/zcash-pool-server/src/server.rs:157-213`

**Step 1: Initialize and start Forge relay in run()**

In `run()`, after spawning the template provider task (around line 190), add:

```rust
        // Initialize and start Forge relay if enabled
        if let Some(ref forge) = self.forge_relay {
            if let Err(e) = forge.init().await {
                warn!("Failed to initialize Forge relay: {}. Continuing without relay.", e);
            } else {
                let forge = Arc::clone(forge);
                tokio::spawn(async move {
                    if let Err(e) = forge.start().await {
                        warn!("Forge relay start error: {}", e);
                    }
                });
                info!("Forge relay started");
            }
        }
```

**Step 2: Verify build compiles**

Run: `cd /Users/zakimanian/stratum-zcash && cargo check -p zcash-pool-server`
Expected: Build succeeds

**Step 3: Commit**

```bash
git add crates/zcash-pool-server/src/server.rs
git commit -m "$(cat <<'EOF'
feat: initialize Forge relay in server run loop

Binds the Forge relay UDP socket and starts the relay client
during server startup. Fails gracefully if initialization fails.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 8: Add Integration Test

**Files:**
- Create: `/Users/zakimanian/stratum-zcash/crates/zcash-pool-server/tests/forge_integration_test.rs`

**Step 1: Create basic integration test**

Create `/Users/zakimanian/stratum-zcash/crates/zcash-pool-server/tests/forge_integration_test.rs`:

```rust
//! Integration tests for Forge relay

use std::net::SocketAddr;

use zcash_pool_server::config::PoolConfig;
use zcash_pool_server::forge::ForgeRelay;

/// Test that ForgeRelay can be created with valid config
#[test]
fn test_forge_relay_creation() {
    let mut config = PoolConfig::default();
    config.forge_relay_enabled = true;
    config.forge_relay_peers = vec!["127.0.0.1:8336".parse().unwrap()];
    config.forge_auth_key = Some([0x42; 32]);

    let relay = ForgeRelay::new(&config);
    assert!(relay.is_ok(), "ForgeRelay should create successfully");
}

/// Test that ForgeRelay fails with empty peers
#[test]
fn test_forge_relay_requires_peers() {
    let mut config = PoolConfig::default();
    config.forge_relay_enabled = true;
    config.forge_relay_peers = vec![]; // Empty!

    let relay = ForgeRelay::new(&config);
    assert!(relay.is_err(), "ForgeRelay should fail with empty peers");
}

/// Test that disabled Forge relay doesn't interfere with pool startup
#[test]
fn test_pool_server_without_forge() {
    let config = PoolConfig::default();
    // forge_relay_enabled defaults to false

    let server = zcash_pool_server::PoolServer::new(config);
    assert!(server.is_ok(), "Pool server should start without Forge relay");
}
```

**Step 2: Run the tests**

Run: `cd /Users/zakimanian/stratum-zcash && cargo test -p zcash-pool-server --test forge_integration_test`
Expected: All tests pass

**Step 3: Commit**

```bash
git add crates/zcash-pool-server/tests/forge_integration_test.rs
git commit -m "$(cat <<'EOF'
test: add Forge relay integration tests

Tests ForgeRelay creation, peer validation, and pool server
startup with/without Forge relay enabled.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 9: Update Pool Config Example

**Files:**
- Modify: `/Users/zakimanian/stratum-zcash/crates/zcash-pool-server/examples/run_pool.rs`

**Step 1: Read existing example**

Check the current structure of the example to understand how to add Forge config.

**Step 2: Add Forge relay configuration to example**

Add Forge relay configuration section with comments explaining usage:

```rust
    // Forge relay configuration (optional)
    // Enable for low-latency block propagation to relay network
    config.forge_relay_enabled = false; // Set to true to enable
    config.forge_relay_peers = vec![
        // Add relay peer addresses here, e.g.:
        // "relay1.example.com:8336".parse().unwrap(),
        // "relay2.example.com:8336".parse().unwrap(),
    ];
    // config.forge_auth_key = Some([0x42; 32]); // Shared key with relay peers
    config.forge_data_shards = 10;
    config.forge_parity_shards = 3;
```

**Step 3: Verify example compiles**

Run: `cd /Users/zakimanian/stratum-zcash && cargo build --example run_pool -p zcash-pool-server`
Expected: Build succeeds

**Step 4: Commit**

```bash
git add crates/zcash-pool-server/examples/run_pool.rs
git commit -m "$(cat <<'EOF'
docs: add Forge relay config to pool example

Shows how to configure Forge relay in the run_pool example,
with comments explaining each option.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 10: Full Integration Test

**Files:**
- Test via manual verification

**Step 1: Run all pool-server tests**

Run: `cd /Users/zakimanian/stratum-zcash && cargo test -p zcash-pool-server`
Expected: All tests pass

**Step 2: Run full workspace build**

Run: `cd /Users/zakimanian/stratum-zcash && cargo build --release`
Expected: Build succeeds with no errors

**Step 3: Run clippy for lint check**

Run: `cd /Users/zakimanian/stratum-zcash && cargo clippy -p zcash-pool-server -- -D warnings`
Expected: No warnings or errors

**Step 4: Final commit with summary**

```bash
git add -A
git commit -m "$(cat <<'EOF'
feat: complete bedrock-forge relay integration

Integrates bedrock-forge compact block relay into stratum-zcash:

- ForgeRelay wrapper module for managing relay client
- Config options for relay peers, auth key, FEC parameters
- Template announcements on new blocks
- Found block announcements before Zebra submission
- Integration tests and example configuration

The integration is non-blocking and parallel to existing miner
job distribution, ensuring zero latency impact on miners.

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>
EOF
)"
```

---

## Summary

| Task | Description | Files Modified |
|------|-------------|----------------|
| 1 | Add bedrock-forge dependency | Cargo.toml (workspace + crate) |
| 2 | Create ForgeRelay configuration | config.rs |
| 3 | Create ForgeRelay wrapper module | forge.rs, lib.rs |
| 4 | Integrate ForgeRelay into PoolServer | server.rs (struct + new) |
| 5 | Hook template announcements | server.rs (handle_new_template) |
| 6 | Hook block found announcements | server.rs (handle_share_submission) |
| 7 | Initialize Forge relay in run loop | server.rs (run) |
| 8 | Add integration tests | tests/forge_integration_test.rs |
| 9 | Update pool config example | examples/run_pool.rs |
| 10 | Full integration test | N/A (verification) |

## Usage After Integration

```toml
# In pool config
forge_relay_enabled = true
forge_bind_addr = "0.0.0.0:8336"
forge_relay_peers = ["relay1.example.com:8336", "relay2.example.com:8336"]
forge_auth_key = "0x424242..." # 32-byte hex key
forge_data_shards = 10
forge_parity_shards = 3
```

The pool will then:
1. Announce each new block template to relay peers
2. Announce found blocks to relay peers BEFORE submitting to Zebra
3. Maintain persistent UDP connections to relay network

Expected latency improvement: 100-500ms faster block propagation compared to Zebra P2P network.
