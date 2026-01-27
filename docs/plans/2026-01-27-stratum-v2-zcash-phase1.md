# Stratum V2 Zcash Template Provider - Phase 1 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a Zcash Template Provider that interfaces with Zebra nodes via RPC and produces SV2-compatible templates with proper Equihash header assembly.

**Architecture:** A Rust library crate (`zcash-template-provider`) that polls Zebra's `getblocktemplate` RPC, constructs 140-byte Equihash input headers, calculates `hashBlockCommitments`, and pushes templates to subscribers via async channels. Uses tokio for async runtime and jsonrpc for Zebra communication.

**Tech Stack:** Rust 1.75+, tokio, serde/serde_json, jsonrpc-core-client, blake2b_simd, hex

---

## Project Setup

### Task 1: Initialize Rust Workspace

**Files:**
- Create: `Cargo.toml` (workspace root)
- Create: `crates/zcash-template-provider/Cargo.toml`
- Create: `crates/zcash-template-provider/src/lib.rs`

**Step 1: Create workspace Cargo.toml**

```toml
[workspace]
resolver = "2"
members = [
    "crates/*",
]

[workspace.package]
version = "0.1.0"
edition = "2021"
license = "MIT OR Apache-2.0"
repository = "https://github.com/zcash/stratum-v2-zcash"

[workspace.dependencies]
tokio = { version = "1.35", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
thiserror = "1.0"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
hex = "0.4"
blake2b_simd = "1.0"
```

**Step 2: Create template-provider crate Cargo.toml**

Create `crates/zcash-template-provider/Cargo.toml`:

```toml
[package]
name = "zcash-template-provider"
version.workspace = true
edition.workspace = true
license.workspace = true
description = "Zcash Template Provider for Stratum V2"

[dependencies]
tokio.workspace = true
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
tracing.workspace = true
hex.workspace = true
blake2b_simd.workspace = true
reqwest = { version = "0.11", features = ["json"] }

[dev-dependencies]
tokio = { workspace = true, features = ["test-util", "macros"] }
```

**Step 3: Create initial lib.rs**

Create `crates/zcash-template-provider/src/lib.rs`:

```rust
//! Zcash Template Provider for Stratum V2
//!
//! This crate provides a Template Provider that interfaces with Zebra nodes
//! and produces SV2-compatible block templates for Equihash mining.

pub mod error;
pub mod rpc;
pub mod template;
pub mod types;

pub use error::Error;
pub use template::TemplateProvider;
```

**Step 4: Verify project compiles**

Run: `cargo check`
Expected: Compilation errors for missing modules (expected at this stage)

**Step 5: Commit**

```bash
git init
git add Cargo.toml crates/
git commit -m "chore: initialize Rust workspace with template-provider crate"
```

---

### Task 2: Define Core Types

**Files:**
- Create: `crates/zcash-template-provider/src/types.rs`
- Create: `crates/zcash-template-provider/src/error.rs`

**Step 1: Write types module**

Create `crates/zcash-template-provider/src/types.rs`:

```rust
//! Core types for Zcash block templates

use serde::{Deserialize, Serialize};

/// 32-byte hash type used throughout Zcash
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Hash256(pub [u8; 32]);

impl Hash256 {
    pub fn from_hex(s: &str) -> Result<Self, hex::FromHexError> {
        let mut bytes = [0u8; 32];
        hex::decode_to_slice(s, &mut bytes)?;
        // Zcash RPC returns hashes in little-endian display order
        bytes.reverse();
        Ok(Self(bytes))
    }

    pub fn to_hex(&self) -> String {
        let mut bytes = self.0;
        bytes.reverse();
        hex::encode(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Zcash block header for Equihash mining (140 bytes before solution)
#[derive(Debug, Clone)]
pub struct EquihashHeader {
    /// Block version (4 bytes)
    pub version: u32,
    /// Hash of previous block (32 bytes)
    pub prev_hash: Hash256,
    /// Merkle root of transactions (32 bytes)
    pub merkle_root: Hash256,
    /// Block commitments hash (32 bytes) - post-NU5
    pub hash_block_commitments: Hash256,
    /// Block timestamp (4 bytes)
    pub time: u32,
    /// Difficulty target (4 bytes, compact format)
    pub bits: u32,
    /// Full 32-byte nonce space
    pub nonce: [u8; 32],
}

impl EquihashHeader {
    /// Serialize header to 140 bytes for Equihash input
    pub fn serialize(&self) -> [u8; 140] {
        let mut out = [0u8; 140];
        out[0..4].copy_from_slice(&self.version.to_le_bytes());
        out[4..36].copy_from_slice(self.prev_hash.as_bytes());
        out[36..68].copy_from_slice(self.merkle_root.as_bytes());
        out[68..100].copy_from_slice(self.hash_block_commitments.as_bytes());
        out[100..104].copy_from_slice(&self.time.to_le_bytes());
        out[104..108].copy_from_slice(&self.bits.to_le_bytes());
        out[108..140].copy_from_slice(&self.nonce);
        out
    }
}

/// Transaction data from getblocktemplate
#[derive(Debug, Clone, Deserialize)]
pub struct TemplateTransaction {
    /// Raw transaction hex
    pub data: String,
    /// Transaction hash
    pub hash: String,
    /// Transaction fee in zatoshis
    pub fee: i64,
    /// Indices of transactions this depends on
    #[serde(default)]
    pub depends: Vec<u32>,
}

/// Default roots from Zebra getblocktemplate
#[derive(Debug, Clone, Deserialize)]
pub struct DefaultRoots {
    #[serde(rename = "merkleroot")]
    pub merkle_root: String,
    #[serde(rename = "chainhistoryroot")]
    pub chain_history_root: String,
    #[serde(rename = "authdataroot")]
    pub auth_data_root: String,
    #[serde(rename = "blockcommitmentshash")]
    pub block_commitments_hash: String,
}

/// Raw getblocktemplate response from Zebra
#[derive(Debug, Clone, Deserialize)]
pub struct GetBlockTemplateResponse {
    pub version: u32,
    #[serde(rename = "previousblockhash")]
    pub previous_block_hash: String,
    #[serde(rename = "defaultroots")]
    pub default_roots: DefaultRoots,
    pub transactions: Vec<TemplateTransaction>,
    #[serde(rename = "coinbasetxn")]
    pub coinbase_txn: serde_json::Value,
    pub target: String,
    pub height: u64,
    pub bits: String,
    #[serde(rename = "curtime")]
    pub cur_time: u64,
}

/// Processed block template ready for mining
#[derive(Debug, Clone)]
pub struct BlockTemplate {
    /// Template ID for tracking
    pub template_id: u64,
    /// Block height
    pub height: u64,
    /// Assembled header (without nonce/solution)
    pub header: EquihashHeader,
    /// Difficulty target as 256-bit value
    pub target: Hash256,
    /// Transactions to include
    pub transactions: Vec<TemplateTransaction>,
    /// Coinbase transaction
    pub coinbase: Vec<u8>,
    /// Total fees available
    pub total_fees: i64,
}
```

**Step 2: Write error module**

Create `crates/zcash-template-provider/src/error.rs`:

```rust
//! Error types for the template provider

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("RPC error: {0}")]
    Rpc(String),

    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Invalid hex: {0}")]
    Hex(#[from] hex::FromHexError),

    #[error("Invalid template: {0}")]
    InvalidTemplate(String),

    #[error("Connection failed: {0}")]
    Connection(String),
}

pub type Result<T> = std::result::Result<T, Error>;
```

**Step 3: Verify compilation**

Run: `cargo check`
Expected: PASS (may warn about unused code)

**Step 4: Commit**

```bash
git add crates/zcash-template-provider/src/
git commit -m "feat: add core types for Zcash block templates"
```

---

### Task 3: Implement Zebra RPC Client

**Files:**
- Create: `crates/zcash-template-provider/src/rpc.rs`
- Create: `crates/zcash-template-provider/tests/rpc_tests.rs`

**Step 1: Write failing test for RPC client**

Create `crates/zcash-template-provider/tests/rpc_tests.rs`:

```rust
use zcash_template_provider::rpc::ZebraRpc;

#[tokio::test]
async fn test_rpc_client_creation() {
    let rpc = ZebraRpc::new("http://127.0.0.1:8232", None, None);
    assert!(rpc.is_ok());
}

#[tokio::test]
async fn test_rpc_request_format() {
    // Test that we format JSON-RPC requests correctly
    let request = serde_json::json!({
        "jsonrpc": "1.0",
        "id": "test",
        "method": "getblocktemplate",
        "params": []
    });

    assert_eq!(request["jsonrpc"], "1.0");
    assert_eq!(request["method"], "getblocktemplate");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p zcash-template-provider --test rpc_tests`
Expected: FAIL with "cannot find value `rpc` in module"

**Step 3: Implement RPC client**

Create `crates/zcash-template-provider/src/rpc.rs`:

```rust
//! Zebra JSON-RPC client

use crate::error::{Error, Result};
use crate::types::GetBlockTemplateResponse;
use reqwest::Client;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};

/// Zebra RPC client
pub struct ZebraRpc {
    client: Client,
    url: String,
    request_id: AtomicU64,
}

impl ZebraRpc {
    /// Create a new RPC client
    pub fn new(url: &str, _user: Option<&str>, _pass: Option<&str>) -> Result<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        Ok(Self {
            client,
            url: url.to_string(),
            request_id: AtomicU64::new(1),
        })
    }

    /// Make a JSON-RPC request
    async fn request<T: DeserializeOwned, P: Serialize>(
        &self,
        method: &str,
        params: P,
    ) -> Result<T> {
        let id = self.request_id.fetch_add(1, Ordering::SeqCst);

        let request = serde_json::json!({
            "jsonrpc": "1.0",
            "id": id.to_string(),
            "method": method,
            "params": params,
        });

        let response = self
            .client
            .post(&self.url)
            .json(&request)
            .send()
            .await?;

        let body: Value = response.json().await?;

        if let Some(error) = body.get("error") {
            if !error.is_null() {
                return Err(Error::Rpc(error.to_string()));
            }
        }

        let result = body
            .get("result")
            .ok_or_else(|| Error::Rpc("missing result field".into()))?;

        serde_json::from_value(result.clone()).map_err(Error::Json)
    }

    /// Get a block template from Zebra
    pub async fn get_block_template(&self) -> Result<GetBlockTemplateResponse> {
        self.request("getblocktemplate", serde_json::json!([])).await
    }

    /// Submit a solved block to Zebra
    pub async fn submit_block(&self, block_hex: &str) -> Result<Option<String>> {
        self.request("submitblock", vec![block_hex]).await
    }

    /// Get the best block hash
    pub async fn get_best_block_hash(&self) -> Result<String> {
        self.request("getbestblockhash", serde_json::json!([])).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let rpc = ZebraRpc::new("http://127.0.0.1:8232", None, None);
        assert!(rpc.is_ok());
    }
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test -p zcash-template-provider`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/zcash-template-provider/
git commit -m "feat: implement Zebra JSON-RPC client"
```

---

### Task 4: Implement Block Commitments Hash Calculation

**Files:**
- Create: `crates/zcash-template-provider/src/commitments.rs`
- Modify: `crates/zcash-template-provider/src/lib.rs`

**Step 1: Write failing test for commitments calculation**

Add to `crates/zcash-template-provider/tests/rpc_tests.rs` (rename to `template_tests.rs`):

```rust
use zcash_template_provider::commitments::calculate_block_commitments_hash;
use zcash_template_provider::types::Hash256;

#[test]
fn test_block_commitments_hash() {
    // Test vector: known history root + auth data root should produce expected hash
    let history_root = Hash256([0u8; 32]);
    let auth_data_root = Hash256([0u8; 32]);

    let result = calculate_block_commitments_hash(&history_root, &auth_data_root);

    // BLAKE2b-256("ZcashBlockCommit" || history_root || auth_data_root || zeros)
    // should be deterministic
    assert_eq!(result.as_bytes().len(), 32);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p zcash-template-provider test_block_commitments`
Expected: FAIL with "cannot find function"

**Step 3: Implement commitments calculation**

Create `crates/zcash-template-provider/src/commitments.rs`:

```rust
//! Block commitments hash calculation for NU5+
//!
//! hashBlockCommitments = BLAKE2b-256("ZcashBlockCommit" || historyRoot || authDataRoot || 0x00...00)
//!
//! The personalization is "ZcashBlockCommit" (16 bytes)
//! historyRoot is the chain history tree root (32 bytes)
//! authDataRoot is the auth data merkle root (32 bytes)
//! The terminator is 32 zero bytes

use crate::types::Hash256;
use blake2b_simd::Params;

const BLOCK_COMMIT_PERSONALIZATION: &[u8; 16] = b"ZcashBlockCommit";

/// Calculate hashBlockCommitments for NU5+ blocks
///
/// # Arguments
/// * `history_root` - The chain history tree root
/// * `auth_data_root` - The auth data merkle root
///
/// # Returns
/// The 32-byte block commitments hash
pub fn calculate_block_commitments_hash(
    history_root: &Hash256,
    auth_data_root: &Hash256,
) -> Hash256 {
    let mut params = Params::new();
    params.hash_length(32);
    params.personal(BLOCK_COMMIT_PERSONALIZATION);

    let mut state = params.to_state();
    state.update(history_root.as_bytes());
    state.update(auth_data_root.as_bytes());
    state.update(&[0u8; 32]); // terminator

    let hash = state.finalize();
    let mut result = [0u8; 32];
    result.copy_from_slice(hash.as_bytes());

    Hash256(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_commitments_deterministic() {
        let h1 = Hash256([1u8; 32]);
        let a1 = Hash256([2u8; 32]);

        let result1 = calculate_block_commitments_hash(&h1, &a1);
        let result2 = calculate_block_commitments_hash(&h1, &a1);

        assert_eq!(result1, result2);
    }

    #[test]
    fn test_block_commitments_changes_with_input() {
        let h1 = Hash256([1u8; 32]);
        let h2 = Hash256([2u8; 32]);
        let auth = Hash256([0u8; 32]);

        let result1 = calculate_block_commitments_hash(&h1, &auth);
        let result2 = calculate_block_commitments_hash(&h2, &auth);

        assert_ne!(result1, result2);
    }
}
```

**Step 4: Update lib.rs**

Add to `crates/zcash-template-provider/src/lib.rs`:

```rust
pub mod commitments;
pub use commitments::calculate_block_commitments_hash;
```

**Step 5: Run tests to verify they pass**

Run: `cargo test -p zcash-template-provider`
Expected: PASS

**Step 6: Commit**

```bash
git add crates/zcash-template-provider/
git commit -m "feat: implement block commitments hash calculation (NU5+)"
```

---

### Task 5: Implement Header Assembly

**Files:**
- Modify: `crates/zcash-template-provider/src/types.rs`
- Create: `crates/zcash-template-provider/src/header.rs`

**Step 1: Write failing test for header assembly**

Create `crates/zcash-template-provider/tests/header_tests.rs`:

```rust
use zcash_template_provider::header::assemble_header;
use zcash_template_provider::types::{GetBlockTemplateResponse, Hash256};

#[test]
fn test_header_serialization_length() {
    // Header without nonce/solution should be 140 bytes
    let header = zcash_template_provider::types::EquihashHeader {
        version: 5,
        prev_hash: Hash256([0u8; 32]),
        merkle_root: Hash256([0u8; 32]),
        hash_block_commitments: Hash256([0u8; 32]),
        time: 1700000000,
        bits: 0x1d00ffff,
        nonce: [0u8; 32],
    };

    let serialized = header.serialize();
    assert_eq!(serialized.len(), 140);
}

#[test]
fn test_header_field_positions() {
    let header = zcash_template_provider::types::EquihashHeader {
        version: 0x05000000,
        prev_hash: Hash256([0xaa; 32]),
        merkle_root: Hash256([0xbb; 32]),
        hash_block_commitments: Hash256([0xcc; 32]),
        time: 0x12345678,
        bits: 0xaabbccdd,
        nonce: [0xff; 32],
    };

    let serialized = header.serialize();

    // Version at offset 0 (little-endian)
    assert_eq!(&serialized[0..4], &[0x00, 0x00, 0x00, 0x05]);

    // prev_hash at offset 4
    assert_eq!(&serialized[4..36], &[0xaa; 32]);

    // merkle_root at offset 36
    assert_eq!(&serialized[36..68], &[0xbb; 32]);

    // hash_block_commitments at offset 68
    assert_eq!(&serialized[68..100], &[0xcc; 32]);

    // time at offset 100 (little-endian)
    assert_eq!(&serialized[100..104], &[0x78, 0x56, 0x34, 0x12]);

    // bits at offset 104 (little-endian)
    assert_eq!(&serialized[104..108], &[0xdd, 0xcc, 0xbb, 0xaa]);

    // nonce at offset 108
    assert_eq!(&serialized[108..140], &[0xff; 32]);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p zcash-template-provider --test header_tests`
Expected: FAIL

**Step 3: Implement header assembly**

Create `crates/zcash-template-provider/src/header.rs`:

```rust
//! Zcash block header assembly for Equihash mining

use crate::error::{Error, Result};
use crate::types::{DefaultRoots, EquihashHeader, GetBlockTemplateResponse, Hash256};

/// Assemble an EquihashHeader from a getblocktemplate response
///
/// # Arguments
/// * `template` - The raw getblocktemplate response from Zebra
///
/// # Returns
/// An assembled header ready for Equihash mining (nonce will be zeroed)
pub fn assemble_header(template: &GetBlockTemplateResponse) -> Result<EquihashHeader> {
    let prev_hash = Hash256::from_hex(&template.previous_block_hash)
        .map_err(|e| Error::InvalidTemplate(format!("invalid prev_hash: {}", e)))?;

    let merkle_root = Hash256::from_hex(&template.default_roots.merkle_root)
        .map_err(|e| Error::InvalidTemplate(format!("invalid merkle_root: {}", e)))?;

    let hash_block_commitments = Hash256::from_hex(&template.default_roots.block_commitments_hash)
        .map_err(|e| Error::InvalidTemplate(format!("invalid block_commitments_hash: {}", e)))?;

    let bits = u32::from_str_radix(&template.bits, 16)
        .map_err(|e| Error::InvalidTemplate(format!("invalid bits: {}", e)))?;

    Ok(EquihashHeader {
        version: template.version,
        prev_hash,
        merkle_root,
        hash_block_commitments,
        time: template.cur_time as u32,
        bits,
        nonce: [0u8; 32],
    })
}

/// Parse target from hex string to Hash256
pub fn parse_target(target_hex: &str) -> Result<Hash256> {
    Hash256::from_hex(target_hex)
        .map_err(|e| Error::InvalidTemplate(format!("invalid target: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_template() -> GetBlockTemplateResponse {
        GetBlockTemplateResponse {
            version: 5,
            previous_block_hash: "0".repeat(64),
            default_roots: DefaultRoots {
                merkle_root: "0".repeat(64),
                chain_history_root: "0".repeat(64),
                auth_data_root: "0".repeat(64),
                block_commitments_hash: "0".repeat(64),
            },
            transactions: vec![],
            coinbase_txn: serde_json::Value::Null,
            target: "0".repeat(64),
            height: 1000000,
            bits: "1d00ffff".to_string(),
            cur_time: 1700000000,
        }
    }

    #[test]
    fn test_assemble_header_basic() {
        let template = make_test_template();
        let header = assemble_header(&template).unwrap();

        assert_eq!(header.version, 5);
        assert_eq!(header.time, 1700000000);
        assert_eq!(header.bits, 0x1d00ffff);
    }
}
```

**Step 4: Update lib.rs**

Add to `crates/zcash-template-provider/src/lib.rs`:

```rust
pub mod header;
pub use header::{assemble_header, parse_target};
```

**Step 5: Run tests to verify they pass**

Run: `cargo test -p zcash-template-provider`
Expected: PASS

**Step 6: Commit**

```bash
git add crates/zcash-template-provider/
git commit -m "feat: implement Zcash header assembly from getblocktemplate"
```

---

### Task 6: Implement Template Provider Core

**Files:**
- Create: `crates/zcash-template-provider/src/template.rs`
- Modify: `crates/zcash-template-provider/src/lib.rs`

**Step 1: Write failing test for template provider**

Create `crates/zcash-template-provider/tests/provider_tests.rs`:

```rust
use zcash_template_provider::template::{TemplateProvider, TemplateProviderConfig};

#[tokio::test]
async fn test_template_provider_creation() {
    let config = TemplateProviderConfig {
        zebra_url: "http://127.0.0.1:8232".to_string(),
        poll_interval_ms: 1000,
    };

    let provider = TemplateProvider::new(config);
    assert!(provider.is_ok());
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p zcash-template-provider --test provider_tests`
Expected: FAIL

**Step 3: Implement Template Provider**

Create `crates/zcash-template-provider/src/template.rs`:

```rust
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
```

**Step 4: Update lib.rs**

Ensure `crates/zcash-template-provider/src/lib.rs` has:

```rust
//! Zcash Template Provider for Stratum V2
//!
//! This crate provides a Template Provider that interfaces with Zebra nodes
//! and produces SV2-compatible block templates for Equihash mining.

pub mod commitments;
pub mod error;
pub mod header;
pub mod rpc;
pub mod template;
pub mod types;

pub use commitments::calculate_block_commitments_hash;
pub use error::Error;
pub use header::{assemble_header, parse_target};
pub use template::{TemplateProvider, TemplateProviderConfig};
```

**Step 5: Run tests to verify they pass**

Run: `cargo test -p zcash-template-provider`
Expected: PASS

**Step 6: Commit**

```bash
git add crates/zcash-template-provider/
git commit -m "feat: implement Template Provider with polling and broadcast"
```

---

### Task 7: Add Integration Test Binary

**Files:**
- Create: `crates/zcash-template-provider/examples/fetch_template.rs`

**Step 1: Create example binary**

Create `crates/zcash-template-provider/examples/fetch_template.rs`:

```rust
//! Example: Fetch a block template from Zebra
//!
//! Usage: cargo run --example fetch_template -- [zebra_url]
//! Default: http://127.0.0.1:8232

use zcash_template_provider::{TemplateProvider, TemplateProviderConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let zebra_url = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "http://127.0.0.1:8232".to_string());

    println!("Connecting to Zebra at {}", zebra_url);

    let config = TemplateProviderConfig {
        zebra_url,
        poll_interval_ms: 1000,
    };

    let provider = TemplateProvider::new(config)?;

    match provider.fetch_template().await {
        Ok(template) => {
            println!("\n=== Block Template ===");
            println!("Template ID: {}", template.template_id);
            println!("Height: {}", template.height);
            println!("Version: {}", template.header.version);
            println!("Prev Hash: {}", template.header.prev_hash.to_hex());
            println!("Merkle Root: {}", template.header.merkle_root.to_hex());
            println!("Block Commitments: {}", template.header.hash_block_commitments.to_hex());
            println!("Time: {}", template.header.time);
            println!("Bits: 0x{:08x}", template.header.bits);
            println!("Target: {}", template.target.to_hex());
            println!("Transactions: {}", template.transactions.len());
            println!("Total Fees: {} zatoshis", template.total_fees);
            println!("Coinbase Size: {} bytes", template.coinbase.len());

            println!("\n=== Header (140 bytes hex) ===");
            let header_bytes = template.header.serialize();
            println!("{}", hex::encode(header_bytes));
        }
        Err(e) => {
            eprintln!("Failed to fetch template: {}", e);
            eprintln!("\nMake sure Zebra is running with RPC enabled.");
            std::process::exit(1);
        }
    }

    Ok(())
}
```

**Step 2: Add tracing-subscriber to dev-dependencies**

Update `crates/zcash-template-provider/Cargo.toml`:

```toml
[dev-dependencies]
tokio = { workspace = true, features = ["test-util", "macros"] }
tracing-subscriber.workspace = true
```

**Step 3: Verify compilation**

Run: `cargo build --example fetch_template`
Expected: PASS

**Step 4: Commit**

```bash
git add crates/zcash-template-provider/
git commit -m "feat: add fetch_template example binary for integration testing"
```

---

### Task 8: Add Nonce Space Partitioning

**Files:**
- Create: `crates/zcash-template-provider/src/nonce.rs`
- Modify: `crates/zcash-template-provider/src/lib.rs`

**Step 1: Write failing test for nonce partitioning**

Create `crates/zcash-template-provider/tests/nonce_tests.rs`:

```rust
use zcash_template_provider::nonce::{NoncePartitioner, NonceRange};

#[test]
fn test_nonce_partition_basic() {
    let partitioner = NoncePartitioner::new(8); // 8-byte nonce_1
    let range = partitioner.get_range(0);

    assert_eq!(range.nonce_1.len(), 8);
    assert_eq!(range.nonce_2_len, 24); // 32 - 8 = 24
}

#[test]
fn test_nonce_partitions_unique() {
    let partitioner = NoncePartitioner::new(8);
    let range1 = partitioner.get_range(0);
    let range2 = partitioner.get_range(1);

    assert_ne!(range1.nonce_1, range2.nonce_1);
}

#[test]
fn test_nonce_1_length_validation() {
    // nonce_1 must be <= 32 bytes
    let result = std::panic::catch_unwind(|| NoncePartitioner::new(33));
    assert!(result.is_err());
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p zcash-template-provider --test nonce_tests`
Expected: FAIL

**Step 3: Implement nonce partitioning**

Create `crates/zcash-template-provider/src/nonce.rs`:

```rust
//! Nonce space partitioning for Zcash mining
//!
//! Zcash uses a 32-byte nonce split as:
//! - NONCE_1: Pool-assigned prefix (assigned to each miner)
//! - NONCE_2: Miner-controlled suffix (incremented during mining)
//!
//! len(NONCE_1) + len(NONCE_2) = 32

use std::sync::atomic::{AtomicU64, Ordering};

/// A partitioned nonce range for a miner
#[derive(Debug, Clone)]
pub struct NonceRange {
    /// Pool-assigned nonce prefix
    pub nonce_1: Vec<u8>,
    /// Length of nonce_2 (miner-controlled portion)
    pub nonce_2_len: usize,
}

impl NonceRange {
    /// Construct a full 32-byte nonce from nonce_1 and nonce_2
    pub fn make_nonce(&self, nonce_2: &[u8]) -> [u8; 32] {
        assert_eq!(nonce_2.len(), self.nonce_2_len, "nonce_2 length mismatch");

        let mut nonce = [0u8; 32];
        nonce[..self.nonce_1.len()].copy_from_slice(&self.nonce_1);
        nonce[self.nonce_1.len()..].copy_from_slice(nonce_2);
        nonce
    }
}

/// Partitions the 32-byte nonce space for multiple miners
pub struct NoncePartitioner {
    nonce_1_len: usize,
    next_id: AtomicU64,
}

impl NoncePartitioner {
    /// Create a new partitioner with the given nonce_1 length
    ///
    /// # Panics
    /// Panics if nonce_1_len > 32
    pub fn new(nonce_1_len: usize) -> Self {
        assert!(nonce_1_len <= 32, "nonce_1 cannot exceed 32 bytes");
        Self {
            nonce_1_len,
            next_id: AtomicU64::new(0),
        }
    }

    /// Get a unique nonce range for a new miner connection
    pub fn allocate_range(&self) -> NonceRange {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        self.get_range(id)
    }

    /// Get a nonce range for a specific ID (useful for deterministic testing)
    pub fn get_range(&self, id: u64) -> NonceRange {
        let mut nonce_1 = vec![0u8; self.nonce_1_len];

        // Encode the ID into nonce_1 (big-endian, truncated to fit)
        let id_bytes = id.to_be_bytes();
        let copy_len = std::cmp::min(self.nonce_1_len, 8);
        let start = self.nonce_1_len.saturating_sub(8);
        nonce_1[start..start + copy_len].copy_from_slice(&id_bytes[8 - copy_len..]);

        NonceRange {
            nonce_1,
            nonce_2_len: 32 - self.nonce_1_len,
        }
    }

    /// Get the nonce_1 length
    pub fn nonce_1_len(&self) -> usize {
        self.nonce_1_len
    }

    /// Get the nonce_2 length
    pub fn nonce_2_len(&self) -> usize {
        32 - self.nonce_1_len
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_make_nonce() {
        let range = NonceRange {
            nonce_1: vec![0x01, 0x02, 0x03, 0x04],
            nonce_2_len: 28,
        };

        let nonce_2 = vec![0xaa; 28];
        let full_nonce = range.make_nonce(&nonce_2);

        assert_eq!(&full_nonce[0..4], &[0x01, 0x02, 0x03, 0x04]);
        assert_eq!(&full_nonce[4..32], &[0xaa; 28]);
    }

    #[test]
    fn test_allocate_increments() {
        let partitioner = NoncePartitioner::new(8);

        let r1 = partitioner.allocate_range();
        let r2 = partitioner.allocate_range();
        let r3 = partitioner.allocate_range();

        assert_ne!(r1.nonce_1, r2.nonce_1);
        assert_ne!(r2.nonce_1, r3.nonce_1);
    }
}
```

**Step 4: Update lib.rs**

Add to `crates/zcash-template-provider/src/lib.rs`:

```rust
pub mod nonce;
pub use nonce::{NoncePartitioner, NonceRange};
```

**Step 5: Run tests to verify they pass**

Run: `cargo test -p zcash-template-provider`
Expected: PASS

**Step 6: Commit**

```bash
git add crates/zcash-template-provider/
git commit -m "feat: implement 32-byte nonce space partitioning (NONCE_1/NONCE_2)"
```

---

### Task 9: Add Documentation and README

**Files:**
- Create: `crates/zcash-template-provider/README.md`
- Create: `README.md` (workspace root)

**Step 1: Create crate README**

Create `crates/zcash-template-provider/README.md`:

```markdown
# zcash-template-provider

Zcash Template Provider for Stratum V2 mining.

## Overview

This crate provides a Template Provider that:
- Interfaces with Zebra nodes via JSON-RPC
- Produces SV2-compatible block templates
- Assembles 140-byte Equihash input headers
- Handles 32-byte nonce space partitioning
- Broadcasts templates to subscribers on new blocks

## Usage

```rust
use zcash_template_provider::{TemplateProvider, TemplateProviderConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = TemplateProviderConfig {
        zebra_url: "http://127.0.0.1:8232".to_string(),
        poll_interval_ms: 1000,
    };

    let provider = TemplateProvider::new(config)?;

    // Fetch a single template
    let template = provider.fetch_template().await?;
    println!("Got template at height {}", template.height);

    // Or subscribe to template updates
    let mut rx = provider.subscribe();
    tokio::spawn(async move { provider.run().await });

    while let Ok(template) = rx.recv().await {
        println!("New template: height={}", template.height);
    }

    Ok(())
}
```

## Requirements

- Rust 1.75+
- Running Zebra node with RPC enabled (port 8232)

## Testing with Zebra

1. Start Zebra with mining RPC enabled
2. Run: `cargo run --example fetch_template`
```

**Step 2: Create workspace README**

Create `README.md`:

```markdown
# Stratum V2 for Zcash

Implementation of Stratum V2 mining protocol for Zcash with support for decentralized block template construction.

## Project Status

Phase 1: Zcash Template Provider - **In Progress**

## Crates

| Crate | Description |
|-------|-------------|
| `zcash-template-provider` | Template Provider interfacing with Zebra |

## Building

```bash
cargo build --release
```

## Testing

```bash
cargo test
```

## Architecture

See [docs/stratum-v2-planning.md](docs/stratum-v2-planning.md) for the full implementation plan.

## License

MIT OR Apache-2.0
```

**Step 3: Commit**

```bash
git add README.md crates/zcash-template-provider/README.md
git commit -m "docs: add README files for workspace and template-provider crate"
```

---

## Summary

This Phase 1 implementation plan creates the foundation for Stratum V2 on Zcash:

1. **Task 1**: Initialize Rust workspace structure
2. **Task 2**: Define core types (Hash256, EquihashHeader, BlockTemplate)
3. **Task 3**: Implement Zebra RPC client
4. **Task 4**: Implement block commitments hash calculation (NU5+)
5. **Task 5**: Implement header assembly from getblocktemplate
6. **Task 6**: Implement Template Provider with polling/broadcast
7. **Task 7**: Add integration test binary
8. **Task 8**: Add nonce space partitioning
9. **Task 9**: Add documentation

**Dependencies**: Each task builds on previous tasks sequentially.

**Testing**: Each task includes unit tests. Integration testing requires a running Zebra node.

**Next Phase**: Phase 2 (Equihash Mining Protocol) will define `NewEquihashJob` and `SubmitEquihashShare` message types.
