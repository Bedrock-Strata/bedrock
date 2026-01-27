# Stratum V2 Zcash Phase 4: Job Declaration Protocol

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement the SV2 Job Declaration Protocol (Coinbase-Only mode) enabling miners to construct their own block templates while pools handle accounting and rewards.

**Architecture:** Two new crates - `zcash-jd-server` (embedded in pool) and `zcash-jd-client` (standalone binary). JD Client uses Phase 1's Template Provider to build templates from local Zebra, declares jobs to JD Server, and submits found blocks to both local Zebra and pool. Single-miner mode for MVP.

**Tech Stack:** Rust 1.75+, tokio, Phase 1-3 crates, no authentication (Phase 4)

---

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| JD Mode | Coinbase-Only | Simpler, better privacy, sufficient for decentralization |
| Template Source | Phase 1 Template Provider | DRY, already handles Zcash headers |
| JD Server Integration | Separate crate, embedded in pool | Clean separation, shared payout tracking |
| JD Client | Standalone binary | Easy miner adoption |
| Downstream | Single-miner mode | MVP simplicity |
| Authentication | None | Consistent with Phase 3, security in Phase 5 |
| Block Submission | Both Zebra + JD Server | Maximizes propagation |

---

## Protocol Messages (Coinbase-Only Mode)

| Message | Direction | Purpose |
|---------|-----------|---------|
| `AllocateMiningJobToken` | JDC → JDS | Request token for job declaration |
| `AllocateMiningJobToken.Success` | JDS → JDC | Return token + coinbase requirements |
| `SetCustomMiningJob` | JDC → Pool | Declare custom job using token |
| `SetCustomMiningJob.Success` | Pool → JDC | Acknowledge job acceptance |
| `SetCustomMiningJob.Error` | Pool → JDC | Reject job declaration |
| `PushSolution` | JDC → JDS | Submit found block |

---

## Crate Structure

```
crates/zcash-jd-server/
├── Cargo.toml
├── src/
│   ├── lib.rs              # Public API
│   ├── config.rs           # JD Server configuration
│   ├── error.rs            # Error types
│   ├── messages.rs         # JD protocol messages
│   ├── codec.rs            # Message encoding/decoding
│   ├── token.rs            # Token allocation and tracking
│   └── server.rs           # JD Server logic
└── tests/
    └── integration_tests.rs

crates/zcash-jd-client/
├── Cargo.toml
├── src/
│   ├── lib.rs              # Library API
│   ├── main.rs             # Binary entry point
│   ├── config.rs           # Client configuration
│   ├── error.rs            # Error types
│   ├── client.rs           # JD Client logic
│   ├── template_builder.rs # Coinbase construction
│   └── block_submitter.rs  # Block submission to Zebra
└── tests/
    └── integration_tests.rs
```

---

## Task 1: Define JD Protocol Messages

**Files:**
- Create: `crates/zcash-jd-server/Cargo.toml`
- Create: `crates/zcash-jd-server/src/lib.rs`
- Create: `crates/zcash-jd-server/src/messages.rs`
- Modify: `Cargo.toml` (workspace root)

**Step 1: Create jd-server crate Cargo.toml**

Create `crates/zcash-jd-server/Cargo.toml`:

```toml
[package]
name = "zcash-jd-server"
version.workspace = true
edition.workspace = true
license.workspace = true
description = "Job Declaration Server for Zcash Stratum V2"

[dependencies]
tokio = { workspace = true, features = ["full"] }
thiserror.workspace = true
tracing.workspace = true
serde.workspace = true
byteorder = "1.5"

# Local dependencies
zcash-mining-protocol = { path = "../zcash-mining-protocol" }
zcash-pool-server = { path = "../zcash-pool-server" }

[dev-dependencies]
tokio = { workspace = true, features = ["test-util", "macros", "rt-multi-thread"] }
```

**Step 2: Create initial lib.rs**

Create `crates/zcash-jd-server/src/lib.rs`:

```rust
//! Job Declaration Server for Zcash Stratum V2
//!
//! This crate provides the JD Server component that:
//! - Allocates mining job tokens to JD Clients
//! - Validates declared custom jobs (Coinbase-Only mode)
//! - Receives block solutions via PushSolution
//! - Integrates with the Pool Server for payout tracking

pub mod config;
pub mod error;
pub mod messages;
pub mod codec;
pub mod token;
pub mod server;

pub use config::JdServerConfig;
pub use error::JdServerError;
pub use messages::*;
pub use server::JdServer;
```

**Step 3: Create JD protocol messages**

Create `crates/zcash-jd-server/src/messages.rs`:

```rust
//! Job Declaration Protocol messages (Coinbase-Only mode)
//!
//! Reference: SV2 Spec Section 6 - Job Declaration Protocol

/// Message type identifiers for JD Protocol
pub mod message_types {
    /// AllocateMiningJobToken request
    pub const ALLOCATE_MINING_JOB_TOKEN: u8 = 0x50;
    /// AllocateMiningJobToken.Success response
    pub const ALLOCATE_MINING_JOB_TOKEN_SUCCESS: u8 = 0x51;
    /// SetCustomMiningJob request
    pub const SET_CUSTOM_MINING_JOB: u8 = 0x52;
    /// SetCustomMiningJob.Success response
    pub const SET_CUSTOM_MINING_JOB_SUCCESS: u8 = 0x53;
    /// SetCustomMiningJob.Error response
    pub const SET_CUSTOM_MINING_JOB_ERROR: u8 = 0x54;
    /// PushSolution (block found)
    pub const PUSH_SOLUTION: u8 = 0x55;
}

/// JD Client -> JD Server: Request a token for job declaration
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllocateMiningJobToken {
    /// Unique request ID for matching response
    pub request_id: u32,
    /// User-readable identifier for the mining device
    pub user_identifier: String,
}

/// JD Server -> JD Client: Token allocated successfully
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllocateMiningJobTokenSuccess {
    /// Matching request ID
    pub request_id: u32,
    /// Allocated token for job declaration
    pub mining_job_token: Vec<u8>,
    /// Maximum additional size for coinbase outputs (bytes)
    pub coinbase_output_max_additional_size: u32,
    /// Pool's required coinbase output (payout script)
    pub coinbase_output: Vec<u8>,
    /// Whether async job mining is allowed
    pub async_mining_allowed: bool,
}

/// JD Client -> Pool: Declare a custom mining job
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetCustomMiningJob {
    /// Channel ID for this job
    pub channel_id: u32,
    /// Request ID for response matching
    pub request_id: u32,
    /// Token from AllocateMiningJobToken.Success
    pub mining_job_token: Vec<u8>,
    /// Block version
    pub version: u32,
    /// Previous block hash
    pub prev_hash: [u8; 32],
    /// Merkle root of transactions (including coinbase)
    pub merkle_root: [u8; 32],
    /// hashBlockCommitments for NU5+
    pub block_commitments: [u8; 32],
    /// Block timestamp
    pub time: u32,
    /// Compact difficulty target (nBits)
    pub bits: u32,
    /// Serialized coinbase transaction
    pub coinbase_tx: Vec<u8>,
}

/// Pool -> JD Client: Custom job accepted
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetCustomMiningJobSuccess {
    /// Matching channel ID
    pub channel_id: u32,
    /// Matching request ID
    pub request_id: u32,
    /// Pool-assigned job ID for share submission
    pub job_id: u32,
}

/// Pool -> JD Client: Custom job rejected
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetCustomMiningJobError {
    /// Matching channel ID
    pub channel_id: u32,
    /// Matching request ID
    pub request_id: u32,
    /// Error code
    pub error_code: SetCustomMiningJobErrorCode,
    /// Human-readable error message
    pub error_message: String,
}

/// Error codes for SetCustomMiningJob.Error
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SetCustomMiningJobErrorCode {
    /// Invalid token
    InvalidToken = 1,
    /// Token expired
    TokenExpired = 2,
    /// Invalid coinbase
    InvalidCoinbase = 3,
    /// Invalid merkle root
    InvalidMerkleRoot = 4,
    /// Stale prevhash
    StalePrevHash = 5,
    /// Other error
    Other = 255,
}

/// JD Client -> JD Server: Submit a found block
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushSolution {
    /// Channel ID
    pub channel_id: u32,
    /// Job ID from SetCustomMiningJob.Success
    pub job_id: u32,
    /// Block version
    pub version: u32,
    /// Block timestamp used
    pub time: u32,
    /// Full 32-byte nonce
    pub nonce: [u8; 32],
    /// Equihash (200,9) solution
    pub solution: [u8; 1344],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allocate_token_creation() {
        let msg = AllocateMiningJobToken {
            request_id: 1,
            user_identifier: "miner-01".to_string(),
        };
        assert_eq!(msg.request_id, 1);
        assert_eq!(msg.user_identifier, "miner-01");
    }

    #[test]
    fn test_set_custom_job() {
        let msg = SetCustomMiningJob {
            channel_id: 1,
            request_id: 42,
            mining_job_token: vec![0x01, 0x02, 0x03],
            version: 5,
            prev_hash: [0xaa; 32],
            merkle_root: [0xbb; 32],
            block_commitments: [0xcc; 32],
            time: 1700000000,
            bits: 0x1d00ffff,
            coinbase_tx: vec![0x01; 100],
        };
        assert_eq!(msg.channel_id, 1);
        assert_eq!(msg.mining_job_token.len(), 3);
    }
}
```

**Step 4: Verify compilation fails (missing modules)**

Run: `cargo check -p zcash-jd-server`
Expected: FAIL (missing config, error, codec, token, server modules)

**Step 5: Commit**

```bash
git add Cargo.toml crates/zcash-jd-server/
git commit -m "feat(jd): define Job Declaration protocol messages"
```

---

## Task 2: Implement JD Server Error Types and Configuration

**Files:**
- Create: `crates/zcash-jd-server/src/error.rs`
- Create: `crates/zcash-jd-server/src/config.rs`

**Step 1: Create error types**

Create `crates/zcash-jd-server/src/error.rs`:

```rust
//! JD Server error types

use thiserror::Error;

#[derive(Error, Debug)]
pub enum JdServerError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid token")]
    InvalidToken,

    #[error("Token expired")]
    TokenExpired,

    #[error("Invalid coinbase: {0}")]
    InvalidCoinbase(String),

    #[error("Invalid merkle root")]
    InvalidMerkleRoot,

    #[error("Stale prev_hash")]
    StalePrevHash,

    #[error("Channel send error")]
    ChannelSend,

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("Pool error: {0}")]
    Pool(#[from] zcash_pool_server::PoolError),
}

pub type Result<T> = std::result::Result<T, JdServerError>;
```

**Step 2: Create configuration**

Create `crates/zcash-jd-server/src/config.rs`:

```rust
//! JD Server configuration

use std::time::Duration;

/// JD Server configuration
#[derive(Debug, Clone)]
pub struct JdServerConfig {
    /// Token validity duration
    pub token_lifetime: Duration,

    /// Maximum coinbase output size miners can add (bytes)
    pub coinbase_output_max_additional_size: u32,

    /// Pool's payout script (for coinbase output)
    pub pool_payout_script: Vec<u8>,

    /// Allow async mining (start mining before job acknowledged)
    pub async_mining_allowed: bool,

    /// Maximum active tokens per client
    pub max_tokens_per_client: usize,
}

impl Default for JdServerConfig {
    fn default() -> Self {
        Self {
            token_lifetime: Duration::from_secs(300), // 5 minutes
            coinbase_output_max_additional_size: 256,
            pool_payout_script: vec![], // Must be set by operator
            async_mining_allowed: true,
            max_tokens_per_client: 10,
        }
    }
}
```

**Step 3: Commit**

```bash
git add crates/zcash-jd-server/src/
git commit -m "feat(jd-server): add error types and configuration"
```

---

## Task 3: Implement Token Allocation

**Files:**
- Create: `crates/zcash-jd-server/src/token.rs`

**Step 1: Create token manager**

Create `crates/zcash-jd-server/src/token.rs`:

```rust
//! Mining job token allocation and tracking

use crate::config::JdServerConfig;
use crate::error::{JdServerError, Result};
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

    /// Allocate a new token for a client
    pub fn allocate_token(&self, client_id: &str) -> Result<MiningJobToken> {
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
    }

    #[test]
    fn test_token_validation() {
        let config = JdServerConfig::default();
        let manager = TokenManager::new(config);

        let token = manager.allocate_token("miner-01").unwrap();
        let validated = manager.validate_token(&token.token).unwrap();
        assert_eq!(validated.client_id, "miner-01");
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
```

**Step 2: Commit**

```bash
git add crates/zcash-jd-server/src/token.rs
git commit -m "feat(jd-server): implement token allocation"
```

---

## Task 4: Implement JD Message Codec

**Files:**
- Create: `crates/zcash-jd-server/src/codec.rs`

**Step 1: Create message codec**

Create `crates/zcash-jd-server/src/codec.rs`:

```rust
//! JD Protocol message encoding/decoding

use crate::error::{JdServerError, Result};
use crate::messages::*;
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io::{Cursor, Read, Write};
use zcash_mining_protocol::codec::MessageFrame;

/// Encode AllocateMiningJobToken message
pub fn encode_allocate_token(msg: &AllocateMiningJobToken) -> Result<Vec<u8>> {
    let mut payload = Vec::new();
    payload.write_u32::<LittleEndian>(msg.request_id)?;

    // Write string as length-prefixed
    let user_bytes = msg.user_identifier.as_bytes();
    payload.write_u16::<LittleEndian>(user_bytes.len() as u16)?;
    payload.write_all(user_bytes)?;

    let frame = MessageFrame {
        extension_type: 0,
        msg_type: message_types::ALLOCATE_MINING_JOB_TOKEN,
        length: payload.len() as u32,
    };

    let mut result = frame.encode();
    result.extend(payload);
    Ok(result)
}

/// Decode AllocateMiningJobToken message
pub fn decode_allocate_token(data: &[u8]) -> Result<AllocateMiningJobToken> {
    let mut cursor = Cursor::new(data);
    let request_id = cursor.read_u32::<LittleEndian>()?;

    let user_len = cursor.read_u16::<LittleEndian>()? as usize;
    let mut user_bytes = vec![0u8; user_len];
    cursor.read_exact(&mut user_bytes)?;

    let user_identifier = String::from_utf8(user_bytes)
        .map_err(|e| JdServerError::Protocol(format!("Invalid UTF-8: {}", e)))?;

    Ok(AllocateMiningJobToken {
        request_id,
        user_identifier,
    })
}

/// Encode AllocateMiningJobTokenSuccess message
pub fn encode_allocate_token_success(msg: &AllocateMiningJobTokenSuccess) -> Result<Vec<u8>> {
    let mut payload = Vec::new();
    payload.write_u32::<LittleEndian>(msg.request_id)?;

    // Token as length-prefixed bytes
    payload.write_u16::<LittleEndian>(msg.mining_job_token.len() as u16)?;
    payload.write_all(&msg.mining_job_token)?;

    payload.write_u32::<LittleEndian>(msg.coinbase_output_max_additional_size)?;

    // Coinbase output as length-prefixed
    payload.write_u16::<LittleEndian>(msg.coinbase_output.len() as u16)?;
    payload.write_all(&msg.coinbase_output)?;

    payload.write_u8(if msg.async_mining_allowed { 1 } else { 0 })?;

    let frame = MessageFrame {
        extension_type: 0,
        msg_type: message_types::ALLOCATE_MINING_JOB_TOKEN_SUCCESS,
        length: payload.len() as u32,
    };

    let mut result = frame.encode();
    result.extend(payload);
    Ok(result)
}

/// Decode AllocateMiningJobTokenSuccess message
pub fn decode_allocate_token_success(data: &[u8]) -> Result<AllocateMiningJobTokenSuccess> {
    let mut cursor = Cursor::new(data);
    let request_id = cursor.read_u32::<LittleEndian>()?;

    let token_len = cursor.read_u16::<LittleEndian>()? as usize;
    let mut token = vec![0u8; token_len];
    cursor.read_exact(&mut token)?;

    let max_size = cursor.read_u32::<LittleEndian>()?;

    let output_len = cursor.read_u16::<LittleEndian>()? as usize;
    let mut output = vec![0u8; output_len];
    cursor.read_exact(&mut output)?;

    let async_allowed = cursor.read_u8()? != 0;

    Ok(AllocateMiningJobTokenSuccess {
        request_id,
        mining_job_token: token,
        coinbase_output_max_additional_size: max_size,
        coinbase_output: output,
        async_mining_allowed: async_allowed,
    })
}

/// Encode SetCustomMiningJob message
pub fn encode_set_custom_job(msg: &SetCustomMiningJob) -> Result<Vec<u8>> {
    let mut payload = Vec::new();
    payload.write_u32::<LittleEndian>(msg.channel_id)?;
    payload.write_u32::<LittleEndian>(msg.request_id)?;

    // Token
    payload.write_u16::<LittleEndian>(msg.mining_job_token.len() as u16)?;
    payload.write_all(&msg.mining_job_token)?;

    payload.write_u32::<LittleEndian>(msg.version)?;
    payload.write_all(&msg.prev_hash)?;
    payload.write_all(&msg.merkle_root)?;
    payload.write_all(&msg.block_commitments)?;
    payload.write_u32::<LittleEndian>(msg.time)?;
    payload.write_u32::<LittleEndian>(msg.bits)?;

    // Coinbase tx
    payload.write_u32::<LittleEndian>(msg.coinbase_tx.len() as u32)?;
    payload.write_all(&msg.coinbase_tx)?;

    let frame = MessageFrame {
        extension_type: 0,
        msg_type: message_types::SET_CUSTOM_MINING_JOB,
        length: payload.len() as u32,
    };

    let mut result = frame.encode();
    result.extend(payload);
    Ok(result)
}

/// Decode SetCustomMiningJob message
pub fn decode_set_custom_job(data: &[u8]) -> Result<SetCustomMiningJob> {
    let mut cursor = Cursor::new(data);
    let channel_id = cursor.read_u32::<LittleEndian>()?;
    let request_id = cursor.read_u32::<LittleEndian>()?;

    let token_len = cursor.read_u16::<LittleEndian>()? as usize;
    let mut token = vec![0u8; token_len];
    cursor.read_exact(&mut token)?;

    let version = cursor.read_u32::<LittleEndian>()?;

    let mut prev_hash = [0u8; 32];
    cursor.read_exact(&mut prev_hash)?;

    let mut merkle_root = [0u8; 32];
    cursor.read_exact(&mut merkle_root)?;

    let mut block_commitments = [0u8; 32];
    cursor.read_exact(&mut block_commitments)?;

    let time = cursor.read_u32::<LittleEndian>()?;
    let bits = cursor.read_u32::<LittleEndian>()?;

    let coinbase_len = cursor.read_u32::<LittleEndian>()? as usize;
    let mut coinbase_tx = vec![0u8; coinbase_len];
    cursor.read_exact(&mut coinbase_tx)?;

    Ok(SetCustomMiningJob {
        channel_id,
        request_id,
        mining_job_token: token,
        version,
        prev_hash,
        merkle_root,
        block_commitments,
        time,
        bits,
        coinbase_tx,
    })
}

/// Encode SetCustomMiningJobSuccess message
pub fn encode_set_custom_job_success(msg: &SetCustomMiningJobSuccess) -> Result<Vec<u8>> {
    let mut payload = Vec::new();
    payload.write_u32::<LittleEndian>(msg.channel_id)?;
    payload.write_u32::<LittleEndian>(msg.request_id)?;
    payload.write_u32::<LittleEndian>(msg.job_id)?;

    let frame = MessageFrame {
        extension_type: 0,
        msg_type: message_types::SET_CUSTOM_MINING_JOB_SUCCESS,
        length: payload.len() as u32,
    };

    let mut result = frame.encode();
    result.extend(payload);
    Ok(result)
}

/// Encode SetCustomMiningJobError message
pub fn encode_set_custom_job_error(msg: &SetCustomMiningJobError) -> Result<Vec<u8>> {
    let mut payload = Vec::new();
    payload.write_u32::<LittleEndian>(msg.channel_id)?;
    payload.write_u32::<LittleEndian>(msg.request_id)?;
    payload.write_u8(msg.error_code as u8)?;

    let msg_bytes = msg.error_message.as_bytes();
    payload.write_u16::<LittleEndian>(msg_bytes.len() as u16)?;
    payload.write_all(msg_bytes)?;

    let frame = MessageFrame {
        extension_type: 0,
        msg_type: message_types::SET_CUSTOM_MINING_JOB_ERROR,
        length: payload.len() as u32,
    };

    let mut result = frame.encode();
    result.extend(payload);
    Ok(result)
}

/// Encode PushSolution message
pub fn encode_push_solution(msg: &PushSolution) -> Result<Vec<u8>> {
    let mut payload = Vec::new();
    payload.write_u32::<LittleEndian>(msg.channel_id)?;
    payload.write_u32::<LittleEndian>(msg.job_id)?;
    payload.write_u32::<LittleEndian>(msg.version)?;
    payload.write_u32::<LittleEndian>(msg.time)?;
    payload.write_all(&msg.nonce)?;
    payload.write_all(&msg.solution)?;

    let frame = MessageFrame {
        extension_type: 0,
        msg_type: message_types::PUSH_SOLUTION,
        length: payload.len() as u32,
    };

    let mut result = frame.encode();
    result.extend(payload);
    Ok(result)
}

/// Decode PushSolution message
pub fn decode_push_solution(data: &[u8]) -> Result<PushSolution> {
    let mut cursor = Cursor::new(data);
    let channel_id = cursor.read_u32::<LittleEndian>()?;
    let job_id = cursor.read_u32::<LittleEndian>()?;
    let version = cursor.read_u32::<LittleEndian>()?;
    let time = cursor.read_u32::<LittleEndian>()?;

    let mut nonce = [0u8; 32];
    cursor.read_exact(&mut nonce)?;

    let mut solution = [0u8; 1344];
    cursor.read_exact(&mut solution)?;

    Ok(PushSolution {
        channel_id,
        job_id,
        version,
        time,
        nonce,
        solution,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allocate_token_roundtrip() {
        let msg = AllocateMiningJobToken {
            request_id: 42,
            user_identifier: "test-miner".to_string(),
        };

        let encoded = encode_allocate_token(&msg).unwrap();
        // Skip frame header (6 bytes)
        let decoded = decode_allocate_token(&encoded[6..]).unwrap();

        assert_eq!(decoded.request_id, msg.request_id);
        assert_eq!(decoded.user_identifier, msg.user_identifier);
    }

    #[test]
    fn test_set_custom_job_roundtrip() {
        let msg = SetCustomMiningJob {
            channel_id: 1,
            request_id: 42,
            mining_job_token: vec![0x01, 0x02, 0x03],
            version: 5,
            prev_hash: [0xaa; 32],
            merkle_root: [0xbb; 32],
            block_commitments: [0xcc; 32],
            time: 1700000000,
            bits: 0x1d00ffff,
            coinbase_tx: vec![0x01; 100],
        };

        let encoded = encode_set_custom_job(&msg).unwrap();
        let decoded = decode_set_custom_job(&encoded[6..]).unwrap();

        assert_eq!(decoded.channel_id, msg.channel_id);
        assert_eq!(decoded.request_id, msg.request_id);
        assert_eq!(decoded.prev_hash, msg.prev_hash);
        assert_eq!(decoded.coinbase_tx, msg.coinbase_tx);
    }

    #[test]
    fn test_push_solution_roundtrip() {
        let msg = PushSolution {
            channel_id: 1,
            job_id: 42,
            version: 5,
            time: 1700000000,
            nonce: [0xff; 32],
            solution: [0xaa; 1344],
        };

        let encoded = encode_push_solution(&msg).unwrap();
        let decoded = decode_push_solution(&encoded[6..]).unwrap();

        assert_eq!(decoded.channel_id, msg.channel_id);
        assert_eq!(decoded.job_id, msg.job_id);
        assert_eq!(decoded.nonce, msg.nonce);
        assert_eq!(decoded.solution, msg.solution);
    }
}
```

**Step 2: Commit**

```bash
git add crates/zcash-jd-server/src/codec.rs
git commit -m "feat(jd-server): implement JD message codec"
```

---

## Task 5: Implement JD Server

**Files:**
- Create: `crates/zcash-jd-server/src/server.rs`

**Step 1: Create JD Server**

Create `crates/zcash-jd-server/src/server.rs`:

```rust
//! Job Declaration Server implementation

use crate::codec::*;
use crate::config::JdServerConfig;
use crate::error::{JdServerError, Result};
use crate::messages::*;
use crate::token::{DeclaredJobInfo, TokenManager};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use zcash_mining_protocol::codec::MessageFrame;
use zcash_pool_server::PayoutTracker;

/// Message from JD session to JD Server
#[derive(Debug)]
pub enum JdSessionMessage {
    /// Client requested a token
    AllocateToken {
        client_id: String,
        request_id: u32,
        response_tx: mpsc::Sender<AllocateMiningJobTokenSuccess>,
    },
    /// Client declared a custom job
    DeclareJob {
        request: SetCustomMiningJob,
        response_tx: mpsc::Sender<std::result::Result<SetCustomMiningJobSuccess, SetCustomMiningJobError>>,
    },
    /// Client submitted a block solution
    PushSolution(PushSolution),
    /// Client disconnected
    Disconnected { client_id: String },
}

/// JD Server embedded in pool
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
        Self {
            token_manager: Arc::new(TokenManager::new(config.clone())),
            config,
            next_job_id: AtomicU32::new(1),
            payout_tracker,
            current_prev_hash: Arc::new(tokio::sync::RwLock::new(None)),
        }
    }

    /// Update the current prev_hash (called when new block arrives)
    pub async fn set_current_prev_hash(&self, prev_hash: [u8; 32]) {
        let mut current = self.current_prev_hash.write().await;
        *current = Some(prev_hash);
    }

    /// Handle a token allocation request
    pub fn handle_allocate_token(&self, request_id: u32, user_id: &str) -> Result<AllocateMiningJobTokenSuccess> {
        let token = self.token_manager.allocate_token(user_id)?;

        Ok(AllocateMiningJobTokenSuccess {
            request_id,
            mining_job_token: token.token,
            coinbase_output_max_additional_size: self.token_manager.coinbase_output_max_additional_size(),
            coinbase_output: self.token_manager.pool_payout_script().to_vec(),
            async_mining_allowed: self.token_manager.async_mining_allowed(),
        })
    }

    /// Handle a custom job declaration
    pub async fn handle_declare_job(
        &self,
        request: SetCustomMiningJob,
    ) -> std::result::Result<SetCustomMiningJobSuccess, SetCustomMiningJobError> {
        // Validate token
        if let Err(e) = self.token_manager.validate_token(&request.mining_job_token) {
            return Err(SetCustomMiningJobError {
                channel_id: request.channel_id,
                request_id: request.request_id,
                error_code: match e {
                    JdServerError::InvalidToken => SetCustomMiningJobErrorCode::InvalidToken,
                    JdServerError::TokenExpired => SetCustomMiningJobErrorCode::TokenExpired,
                    _ => SetCustomMiningJobErrorCode::Other,
                },
                error_message: e.to_string(),
            });
        }

        // Check prev_hash is current
        {
            let current = self.current_prev_hash.read().await;
            if let Some(current_hash) = *current {
                if request.prev_hash != current_hash {
                    return Err(SetCustomMiningJobError {
                        channel_id: request.channel_id,
                        request_id: request.request_id,
                        error_code: SetCustomMiningJobErrorCode::StalePrevHash,
                        error_message: "Stale prev_hash".to_string(),
                    });
                }
            }
        }

        // Validate coinbase (basic checks)
        if request.coinbase_tx.is_empty() {
            return Err(SetCustomMiningJobError {
                channel_id: request.channel_id,
                request_id: request.request_id,
                error_code: SetCustomMiningJobErrorCode::InvalidCoinbase,
                error_message: "Empty coinbase".to_string(),
            });
        }

        // Allocate job ID
        let job_id = self.next_job_id.fetch_add(1, Ordering::SeqCst);

        // Store job info with token
        let job_info = DeclaredJobInfo {
            job_id,
            prev_hash: request.prev_hash,
            merkle_root: request.merkle_root,
            coinbase_tx: request.coinbase_tx,
        };

        if let Err(e) = self.token_manager.set_job_info(&request.mining_job_token, job_info) {
            return Err(SetCustomMiningJobError {
                channel_id: request.channel_id,
                request_id: request.request_id,
                error_code: SetCustomMiningJobErrorCode::Other,
                error_message: e.to_string(),
            });
        }

        info!(
            "Job {} declared by channel {} (prev_hash: {}...)",
            job_id,
            request.channel_id,
            hex::encode(&request.prev_hash[..4])
        );

        Ok(SetCustomMiningJobSuccess {
            channel_id: request.channel_id,
            request_id: request.request_id,
            job_id,
        })
    }

    /// Handle a block solution submission
    pub async fn handle_push_solution(&self, solution: PushSolution) -> Result<()> {
        info!(
            "Block solution received for job {} from channel {}",
            solution.job_id, solution.channel_id
        );

        // TODO: Validate solution and submit to Zebra
        // For now, just log it
        debug!(
            "Solution: nonce={}, time={}",
            hex::encode(&solution.nonce[..8]),
            solution.time
        );

        Ok(())
    }

    /// Get token manager (for testing)
    pub fn token_manager(&self) -> &TokenManager {
        &self.token_manager
    }
}

/// Handle a JD client connection
pub async fn handle_jd_client(
    mut stream: TcpStream,
    jd_server: Arc<JdServer>,
    client_id: String,
) -> Result<()> {
    info!("JD client connected: {}", client_id);

    let mut read_buf = vec![0u8; 8192];
    let mut buffer = Vec::new();

    loop {
        // Read data
        let n = match stream.read(&mut read_buf).await {
            Ok(0) => {
                info!("JD client disconnected: {}", client_id);
                break;
            }
            Ok(n) => n,
            Err(e) => {
                error!("Read error for {}: {}", client_id, e);
                break;
            }
        };

        buffer.extend_from_slice(&read_buf[..n]);

        // Try to parse messages
        while buffer.len() >= MessageFrame::HEADER_SIZE {
            let frame = match MessageFrame::decode(&buffer[..MessageFrame::HEADER_SIZE]) {
                Ok(f) => f,
                Err(e) => {
                    warn!("Frame decode error: {}", e);
                    break;
                }
            };

            let total_len = MessageFrame::HEADER_SIZE + frame.length as usize;
            if buffer.len() < total_len {
                break; // Need more data
            }

            let payload = &buffer[MessageFrame::HEADER_SIZE..total_len];

            // Handle message by type
            match frame.msg_type {
                message_types::ALLOCATE_MINING_JOB_TOKEN => {
                    let request = decode_allocate_token(payload)?;
                    debug!("AllocateMiningJobToken from {}: request_id={}", client_id, request.request_id);

                    match jd_server.handle_allocate_token(request.request_id, &request.user_identifier) {
                        Ok(response) => {
                            let encoded = encode_allocate_token_success(&response)?;
                            stream.write_all(&encoded).await?;
                        }
                        Err(e) => {
                            error!("Token allocation failed: {}", e);
                        }
                    }
                }
                message_types::SET_CUSTOM_MINING_JOB => {
                    let request = decode_set_custom_job(payload)?;
                    debug!("SetCustomMiningJob from {}: request_id={}", client_id, request.request_id);

                    match jd_server.handle_declare_job(request).await {
                        Ok(response) => {
                            let encoded = encode_set_custom_job_success(&response)?;
                            stream.write_all(&encoded).await?;
                        }
                        Err(error) => {
                            let encoded = encode_set_custom_job_error(&error)?;
                            stream.write_all(&encoded).await?;
                        }
                    }
                }
                message_types::PUSH_SOLUTION => {
                    let solution = decode_push_solution(payload)?;
                    debug!("PushSolution from {}: job_id={}", client_id, solution.job_id);

                    if let Err(e) = jd_server.handle_push_solution(solution).await {
                        error!("Push solution failed: {}", e);
                    }
                }
                _ => {
                    warn!("Unknown message type: 0x{:02x}", frame.msg_type);
                }
            }

            // Remove processed message
            buffer.drain(..total_len);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jd_server_creation() {
        let config = JdServerConfig::default();
        let payout = Arc::new(PayoutTracker::default());
        let server = JdServer::new(config, payout);

        // Allocate a token
        let response = server.handle_allocate_token(1, "test-miner").unwrap();
        assert_eq!(response.request_id, 1);
        assert!(!response.mining_job_token.is_empty());
    }

    #[tokio::test]
    async fn test_declare_job() {
        let mut config = JdServerConfig::default();
        config.pool_payout_script = vec![0x76, 0xa9]; // Example P2PKH prefix
        let payout = Arc::new(PayoutTracker::default());
        let server = JdServer::new(config, payout);

        // Set current prev_hash
        let prev_hash = [0xaa; 32];
        server.set_current_prev_hash(prev_hash).await;

        // Allocate token
        let token_response = server.handle_allocate_token(1, "test-miner").unwrap();

        // Declare job
        let request = SetCustomMiningJob {
            channel_id: 1,
            request_id: 2,
            mining_job_token: token_response.mining_job_token,
            version: 5,
            prev_hash,
            merkle_root: [0xbb; 32],
            block_commitments: [0xcc; 32],
            time: 1700000000,
            bits: 0x1d00ffff,
            coinbase_tx: vec![0x01; 100],
        };

        let result = server.handle_declare_job(request).await;
        assert!(result.is_ok());
        let success = result.unwrap();
        assert_eq!(success.channel_id, 1);
        assert_eq!(success.request_id, 2);
        assert!(success.job_id > 0);
    }

    #[tokio::test]
    async fn test_stale_prev_hash_rejected() {
        let config = JdServerConfig::default();
        let payout = Arc::new(PayoutTracker::default());
        let server = JdServer::new(config, payout);

        // Set current prev_hash
        server.set_current_prev_hash([0xaa; 32]).await;

        // Allocate token
        let token_response = server.handle_allocate_token(1, "test-miner").unwrap();

        // Declare job with wrong prev_hash
        let request = SetCustomMiningJob {
            channel_id: 1,
            request_id: 2,
            mining_job_token: token_response.mining_job_token,
            version: 5,
            prev_hash: [0xbb; 32], // Wrong!
            merkle_root: [0xcc; 32],
            block_commitments: [0xdd; 32],
            time: 1700000000,
            bits: 0x1d00ffff,
            coinbase_tx: vec![0x01; 100],
        };

        let result = server.handle_declare_job(request).await;
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.error_code, SetCustomMiningJobErrorCode::StalePrevHash);
    }
}
```

**Step 2: Verify compilation**

Run: `cargo check -p zcash-jd-server`
Expected: PASS

**Step 3: Commit**

```bash
git add crates/zcash-jd-server/src/server.rs
git commit -m "feat(jd-server): implement JD Server logic"
```

---

## Task 6: Initialize JD Client Crate

**Files:**
- Create: `crates/zcash-jd-client/Cargo.toml`
- Create: `crates/zcash-jd-client/src/lib.rs`
- Create: `crates/zcash-jd-client/src/main.rs`
- Create: `crates/zcash-jd-client/src/config.rs`
- Create: `crates/zcash-jd-client/src/error.rs`
- Modify: `Cargo.toml` (workspace root)

**Step 1: Create jd-client crate Cargo.toml**

Create `crates/zcash-jd-client/Cargo.toml`:

```toml
[package]
name = "zcash-jd-client"
version.workspace = true
edition.workspace = true
license.workspace = true
description = "Job Declaration Client for Zcash Stratum V2"

[[bin]]
name = "zcash-jd-client"
path = "src/main.rs"

[dependencies]
tokio = { workspace = true, features = ["full"] }
thiserror.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
serde.workspace = true
byteorder = "1.5"
clap = { version = "4.4", features = ["derive"] }

# Local dependencies
zcash-template-provider = { path = "../zcash-template-provider" }
zcash-jd-server = { path = "../zcash-jd-server" }
zcash-mining-protocol = { path = "../zcash-mining-protocol" }
zcash-equihash-validator = { path = "../zcash-equihash-validator" }

[dev-dependencies]
tokio = { workspace = true, features = ["test-util", "macros", "rt-multi-thread"] }
```

**Step 2: Create lib.rs**

Create `crates/zcash-jd-client/src/lib.rs`:

```rust
//! Job Declaration Client for Zcash Stratum V2
//!
//! This crate provides the JD Client that:
//! - Connects to a local Zebra node via Template Provider
//! - Builds custom block templates
//! - Declares jobs to a pool's JD Server
//! - Submits found blocks to both Zebra and the pool

pub mod client;
pub mod config;
pub mod error;
pub mod template_builder;
pub mod block_submitter;

pub use client::JdClient;
pub use config::JdClientConfig;
pub use error::JdClientError;
```

**Step 3: Create config.rs**

Create `crates/zcash-jd-client/src/config.rs`:

```rust
//! JD Client configuration

use std::net::SocketAddr;

/// JD Client configuration
#[derive(Debug, Clone)]
pub struct JdClientConfig {
    /// Zebra RPC URL for template provider
    pub zebra_url: String,

    /// Pool JD Server address
    pub pool_jd_addr: SocketAddr,

    /// User identifier for the miner
    pub user_identifier: String,

    /// Template polling interval in milliseconds
    pub template_poll_ms: u64,

    /// Miner payout address (for extra coinbase output)
    pub miner_payout_address: Option<String>,
}

impl Default for JdClientConfig {
    fn default() -> Self {
        Self {
            zebra_url: "http://127.0.0.1:8232".to_string(),
            pool_jd_addr: "127.0.0.1:3334".parse().unwrap(),
            user_identifier: "zcash-jd-client".to_string(),
            template_poll_ms: 1000,
            miner_payout_address: None,
        }
    }
}
```

**Step 4: Create error.rs**

Create `crates/zcash-jd-client/src/error.rs`:

```rust
//! JD Client error types

use thiserror::Error;

#[derive(Error, Debug)]
pub enum JdClientError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Template provider error: {0}")]
    TemplateProvider(#[from] zcash_template_provider::Error),

    #[error("JD Server error: {0}")]
    JdServer(#[from] zcash_jd_server::JdServerError),

    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Token allocation failed: {0}")]
    TokenAllocationFailed(String),

    #[error("Job declaration rejected: {0}")]
    JobRejected(String),

    #[error("Block submission failed: {0}")]
    BlockSubmissionFailed(String),

    #[error("Protocol error: {0}")]
    Protocol(String),
}

pub type Result<T> = std::result::Result<T, JdClientError>;
```

**Step 5: Create main.rs**

Create `crates/zcash-jd-client/src/main.rs`:

```rust
//! Zcash JD Client binary
//!
//! Usage: zcash-jd-client --zebra-url http://127.0.0.1:8232 --pool-jd-addr 127.0.0.1:3334

use clap::Parser;
use zcash_jd_client::{JdClient, JdClientConfig};

#[derive(Parser, Debug)]
#[command(name = "zcash-jd-client")]
#[command(about = "Zcash Job Declaration Client for Stratum V2")]
struct Args {
    /// Zebra RPC URL
    #[arg(long, default_value = "http://127.0.0.1:8232")]
    zebra_url: String,

    /// Pool JD Server address
    #[arg(long, default_value = "127.0.0.1:3334")]
    pool_jd_addr: String,

    /// User identifier
    #[arg(long, default_value = "zcash-jd-client")]
    user_id: String,

    /// Template polling interval (ms)
    #[arg(long, default_value = "1000")]
    poll_interval: u64,

    /// Miner payout address (optional)
    #[arg(long)]
    payout_address: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    let config = JdClientConfig {
        zebra_url: args.zebra_url,
        pool_jd_addr: args.pool_jd_addr.parse()?,
        user_identifier: args.user_id,
        template_poll_ms: args.poll_interval,
        miner_payout_address: args.payout_address,
    };

    println!("=== Zcash JD Client ===");
    println!("Zebra RPC: {}", config.zebra_url);
    println!("Pool JD Server: {}", config.pool_jd_addr);
    println!("User ID: {}", config.user_identifier);
    println!();

    let client = JdClient::new(config)?;
    client.run().await?;

    Ok(())
}
```

**Step 6: Verify compilation fails (missing modules)**

Run: `cargo check -p zcash-jd-client`
Expected: FAIL (missing client, template_builder, block_submitter modules)

**Step 7: Commit**

```bash
git add Cargo.toml crates/zcash-jd-client/
git commit -m "feat(jd-client): initialize JD Client crate"
```

---

## Task 7: Implement Template Builder

**Files:**
- Create: `crates/zcash-jd-client/src/template_builder.rs`

**Step 1: Create template builder**

Create `crates/zcash-jd-client/src/template_builder.rs`:

```rust
//! Template builder for JD Client
//!
//! Constructs custom block templates using the Template Provider
//! and adds the pool's required coinbase output.

use crate::error::{JdClientError, Result};
use zcash_template_provider::types::BlockTemplate;

/// Template builder that adds pool coinbase requirements
pub struct TemplateBuilder {
    /// Pool's required coinbase output script
    pool_coinbase_output: Vec<u8>,
    /// Maximum additional coinbase size allowed
    max_additional_size: u32,
    /// Optional miner payout address
    miner_payout_address: Option<String>,
}

impl TemplateBuilder {
    /// Create a new template builder
    pub fn new(
        pool_coinbase_output: Vec<u8>,
        max_additional_size: u32,
        miner_payout_address: Option<String>,
    ) -> Self {
        Self {
            pool_coinbase_output,
            max_additional_size,
            miner_payout_address,
        }
    }

    /// Build a custom coinbase transaction from a template
    ///
    /// The coinbase must include the pool's required output.
    /// For Phase 4 MVP, we use the template's coinbase directly
    /// and prepend the pool's output.
    pub fn build_coinbase(&self, template: &BlockTemplate) -> Result<Vec<u8>> {
        // For MVP, use the template's coinbase as-is
        // In production, we'd construct a proper coinbase with:
        // 1. Pool's required output (for payout)
        // 2. Miner's optional output
        // 3. Funding stream outputs (handled by Zebra)

        // Validate size
        let additional_size = self.pool_coinbase_output.len() as u32;
        if additional_size > self.max_additional_size {
            return Err(JdClientError::Protocol(format!(
                "Pool output too large: {} > {}",
                additional_size, self.max_additional_size
            )));
        }

        // For now, return the template's coinbase
        // TODO: Proper coinbase construction with pool output
        Ok(template.coinbase_tx.clone())
    }

    /// Calculate merkle root for a modified coinbase
    ///
    /// For MVP, we use the template's merkle root directly
    /// since we're not modifying the coinbase.
    pub fn calculate_merkle_root(&self, template: &BlockTemplate, _coinbase: &[u8]) -> [u8; 32] {
        // For MVP, use template's merkle root
        // In production, we'd recalculate based on modified coinbase
        template.header.merkle_root.0
    }

    /// Get the block commitments hash
    pub fn block_commitments(&self, template: &BlockTemplate) -> [u8; 32] {
        template.header.block_commitments.0
    }

    /// Update pool coinbase output (when receiving new token)
    pub fn set_pool_output(&mut self, output: Vec<u8>, max_size: u32) {
        self.pool_coinbase_output = output;
        self.max_additional_size = max_size;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_builder_creation() {
        let builder = TemplateBuilder::new(
            vec![0x76, 0xa9], // P2PKH prefix
            256,
            None,
        );

        assert_eq!(builder.max_additional_size, 256);
    }
}
```

**Step 2: Commit**

```bash
git add crates/zcash-jd-client/src/template_builder.rs
git commit -m "feat(jd-client): implement template builder"
```

---

## Task 8: Implement Block Submitter

**Files:**
- Create: `crates/zcash-jd-client/src/block_submitter.rs`

**Step 1: Create block submitter**

Create `crates/zcash-jd-client/src/block_submitter.rs`:

```rust
//! Block submission to Zebra
//!
//! Submits found blocks to the local Zebra node via RPC.

use crate::error::{JdClientError, Result};
use tracing::{error, info};

/// Block submitter for Zebra RPC
pub struct BlockSubmitter {
    /// Zebra RPC URL
    zebra_url: String,
    /// HTTP client
    client: reqwest::Client,
}

impl BlockSubmitter {
    /// Create a new block submitter
    pub fn new(zebra_url: String) -> Self {
        Self {
            zebra_url,
            client: reqwest::Client::new(),
        }
    }

    /// Submit a block to Zebra
    ///
    /// Returns Ok(()) if the block was accepted, Err otherwise.
    pub async fn submit_block(&self, block_hex: &str) -> Result<()> {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": "1",
            "method": "submitblock",
            "params": [block_hex]
        });

        let response = self
            .client
            .post(&self.zebra_url)
            .json(&request)
            .send()
            .await
            .map_err(|e| JdClientError::BlockSubmissionFailed(e.to_string()))?;

        let result: serde_json::Value = response
            .json()
            .await
            .map_err(|e| JdClientError::BlockSubmissionFailed(e.to_string()))?;

        // Check for errors
        if let Some(error) = result.get("error") {
            if !error.is_null() {
                let error_msg = error.to_string();
                error!("Block submission failed: {}", error_msg);
                return Err(JdClientError::BlockSubmissionFailed(error_msg));
            }
        }

        info!("Block submitted successfully to Zebra");
        Ok(())
    }

    /// Build block hex from components
    pub fn build_block_hex(
        header: &[u8; 140],
        solution: &[u8; 1344],
        coinbase_tx: &[u8],
        transactions: &[Vec<u8>],
    ) -> String {
        let mut block = Vec::new();

        // Header (140 bytes)
        block.extend_from_slice(header);

        // Equihash solution length (compactSize) + solution
        // For 1344 bytes, compactSize is 0xfd followed by 2-byte length
        block.push(0xfd);
        block.extend_from_slice(&(1344u16).to_le_bytes());
        block.extend_from_slice(solution);

        // Transaction count (compactSize)
        let tx_count = 1 + transactions.len();
        if tx_count < 0xfd {
            block.push(tx_count as u8);
        } else {
            block.push(0xfd);
            block.extend_from_slice(&(tx_count as u16).to_le_bytes());
        }

        // Coinbase transaction
        block.extend_from_slice(coinbase_tx);

        // Other transactions
        for tx in transactions {
            block.extend_from_slice(tx);
        }

        hex::encode(block)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_submitter_creation() {
        let submitter = BlockSubmitter::new("http://127.0.0.1:8232".to_string());
        assert_eq!(submitter.zebra_url, "http://127.0.0.1:8232");
    }

    #[test]
    fn test_build_block_hex() {
        let header = [0xaa; 140];
        let solution = [0xbb; 1344];
        let coinbase_tx = vec![0x01; 100];
        let transactions: Vec<Vec<u8>> = vec![];

        let hex = BlockSubmitter::build_block_hex(&header, &solution, &coinbase_tx, &transactions);

        // Should be: header(140) + fd + len(2) + solution(1344) + tx_count(1) + coinbase(100)
        // = 140 + 1 + 2 + 1344 + 1 + 100 = 1588 bytes = 3176 hex chars
        assert_eq!(hex.len(), 3176);
    }
}
```

**Step 2: Add reqwest dependency**

Update `crates/zcash-jd-client/Cargo.toml` to add:
```toml
reqwest = { version = "0.11", features = ["json"] }
hex.workspace = true
```

**Step 3: Commit**

```bash
git add crates/zcash-jd-client/
git commit -m "feat(jd-client): implement block submitter"
```

---

## Task 9: Implement JD Client

**Files:**
- Create: `crates/zcash-jd-client/src/client.rs`

**Step 1: Create JD Client**

Create `crates/zcash-jd-client/src/client.rs`:

```rust
//! JD Client implementation

use crate::block_submitter::BlockSubmitter;
use crate::config::JdClientConfig;
use crate::error::{JdClientError, Result};
use crate::template_builder::TemplateBuilder;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use zcash_jd_server::codec::*;
use zcash_jd_server::messages::*;
use zcash_mining_protocol::codec::MessageFrame;
use zcash_template_provider::{TemplateProvider, TemplateProviderConfig};
use zcash_template_provider::types::BlockTemplate;

/// JD Client state
pub struct JdClient {
    /// Configuration
    config: JdClientConfig,
    /// Template provider
    template_provider: Arc<TemplateProvider>,
    /// Template builder
    template_builder: Arc<RwLock<TemplateBuilder>>,
    /// Block submitter
    block_submitter: BlockSubmitter,
    /// Current token
    current_token: Arc<RwLock<Option<Vec<u8>>>>,
    /// Current job ID
    current_job_id: Arc<RwLock<Option<u32>>>,
}

impl JdClient {
    /// Create a new JD Client
    pub fn new(config: JdClientConfig) -> Result<Self> {
        let template_config = TemplateProviderConfig {
            zebra_url: config.zebra_url.clone(),
            poll_interval_ms: config.template_poll_ms,
        };

        let template_provider = TemplateProvider::new(template_config)?;

        Ok(Self {
            template_builder: Arc::new(RwLock::new(TemplateBuilder::new(
                vec![], // Will be set after token allocation
                0,
                config.miner_payout_address.clone(),
            ))),
            block_submitter: BlockSubmitter::new(config.zebra_url.clone()),
            config,
            template_provider: Arc::new(template_provider),
            current_token: Arc::new(RwLock::new(None)),
            current_job_id: Arc::new(RwLock::new(None)),
        })
    }

    /// Run the JD Client
    pub async fn run(self) -> Result<()> {
        info!("Starting JD Client");

        // Connect to pool JD Server
        let mut stream = TcpStream::connect(self.config.pool_jd_addr)
            .await
            .map_err(|e| JdClientError::ConnectionFailed(e.to_string()))?;

        info!("Connected to pool JD Server at {}", self.config.pool_jd_addr);

        // Allocate initial token
        self.allocate_token(&mut stream).await?;

        // Subscribe to template updates
        let mut template_rx = self.template_provider.subscribe();

        // Spawn template provider
        let provider = self.template_provider.clone();
        tokio::spawn(async move {
            if let Err(e) = provider.run().await {
                error!("Template provider error: {}", e);
            }
        });

        info!("JD Client running");

        // Main loop
        loop {
            tokio::select! {
                // Receive template updates
                template_result = template_rx.recv() => {
                    match template_result {
                        Ok(template) => {
                            if let Err(e) = self.handle_new_template(&mut stream, template).await {
                                error!("Template handling error: {}", e);
                            }
                        }
                        Err(e) => {
                            warn!("Template channel error: {}", e);
                        }
                    }
                }
            }
        }
    }

    /// Allocate a mining job token from the pool
    async fn allocate_token(&self, stream: &mut TcpStream) -> Result<()> {
        let request = AllocateMiningJobToken {
            request_id: 1,
            user_identifier: self.config.user_identifier.clone(),
        };

        let encoded = encode_allocate_token(&request)?;
        stream.write_all(&encoded).await?;

        // Read response
        let mut header_buf = [0u8; MessageFrame::HEADER_SIZE];
        stream.read_exact(&mut header_buf).await?;

        let frame = MessageFrame::decode(&header_buf)
            .map_err(|e| JdClientError::Protocol(e.to_string()))?;

        let mut payload = vec![0u8; frame.length as usize];
        stream.read_exact(&mut payload).await?;

        if frame.msg_type != message_types::ALLOCATE_MINING_JOB_TOKEN_SUCCESS {
            return Err(JdClientError::TokenAllocationFailed(
                "Unexpected response type".to_string(),
            ));
        }

        let response = decode_allocate_token_success(&payload)?;

        info!(
            "Token allocated: {} bytes, max_additional_size={}",
            response.mining_job_token.len(),
            response.coinbase_output_max_additional_size
        );

        // Update template builder with pool requirements
        {
            let mut builder = self.template_builder.write().await;
            builder.set_pool_output(
                response.coinbase_output.clone(),
                response.coinbase_output_max_additional_size,
            );
        }

        // Store token
        {
            let mut token = self.current_token.write().await;
            *token = Some(response.mining_job_token);
        }

        Ok(())
    }

    /// Handle a new template from Zebra
    async fn handle_new_template(
        &self,
        stream: &mut TcpStream,
        template: BlockTemplate,
    ) -> Result<()> {
        let token = {
            let guard = self.current_token.read().await;
            guard.clone().ok_or(JdClientError::Protocol("No token".to_string()))?
        };

        let builder = self.template_builder.read().await;

        // Build coinbase
        let coinbase = builder.build_coinbase(&template)?;
        let merkle_root = builder.calculate_merkle_root(&template, &coinbase);
        let block_commitments = builder.block_commitments(&template);

        // Declare job to pool
        let request = SetCustomMiningJob {
            channel_id: 1,
            request_id: template.height as u32,
            mining_job_token: token,
            version: template.header.version,
            prev_hash: template.header.prev_hash.0,
            merkle_root,
            block_commitments,
            time: template.header.time,
            bits: template.header.bits,
            coinbase_tx: coinbase,
        };

        let encoded = encode_set_custom_job(&request)?;
        stream.write_all(&encoded).await?;

        // Read response
        let mut header_buf = [0u8; MessageFrame::HEADER_SIZE];
        stream.read_exact(&mut header_buf).await?;

        let frame = MessageFrame::decode(&header_buf)
            .map_err(|e| JdClientError::Protocol(e.to_string()))?;

        let mut payload = vec![0u8; frame.length as usize];
        stream.read_exact(&mut payload).await?;

        match frame.msg_type {
            message_types::SET_CUSTOM_MINING_JOB_SUCCESS => {
                let response = decode_set_custom_job_success(&payload)?;
                info!(
                    "Job declared: job_id={}, height={}",
                    response.job_id, template.height
                );

                // Store job ID
                {
                    let mut job_id = self.current_job_id.write().await;
                    *job_id = Some(response.job_id);
                }
            }
            message_types::SET_CUSTOM_MINING_JOB_ERROR => {
                let error = decode_set_custom_job_error(&payload)?;
                warn!("Job rejected: {:?} - {}", error.error_code, error.error_message);

                // Request new token if expired
                if error.error_code == SetCustomMiningJobErrorCode::TokenExpired {
                    self.allocate_token(stream).await?;
                }

                return Err(JdClientError::JobRejected(error.error_message));
            }
            _ => {
                return Err(JdClientError::Protocol(format!(
                    "Unexpected message type: 0x{:02x}",
                    frame.msg_type
                )));
            }
        }

        Ok(())
    }
}

// Add missing decode functions
fn decode_set_custom_job_success(data: &[u8]) -> Result<SetCustomMiningJobSuccess> {
    use byteorder::{LittleEndian, ReadBytesExt};
    use std::io::Cursor;

    let mut cursor = Cursor::new(data);
    let channel_id = cursor.read_u32::<LittleEndian>()?;
    let request_id = cursor.read_u32::<LittleEndian>()?;
    let job_id = cursor.read_u32::<LittleEndian>()?;

    Ok(SetCustomMiningJobSuccess {
        channel_id,
        request_id,
        job_id,
    })
}

fn decode_set_custom_job_error(data: &[u8]) -> Result<SetCustomMiningJobError> {
    use byteorder::{LittleEndian, ReadBytesExt};
    use std::io::{Cursor, Read};

    let mut cursor = Cursor::new(data);
    let channel_id = cursor.read_u32::<LittleEndian>()?;
    let request_id = cursor.read_u32::<LittleEndian>()?;
    let error_code_byte = cursor.read_u8()?;

    let error_code = match error_code_byte {
        1 => SetCustomMiningJobErrorCode::InvalidToken,
        2 => SetCustomMiningJobErrorCode::TokenExpired,
        3 => SetCustomMiningJobErrorCode::InvalidCoinbase,
        4 => SetCustomMiningJobErrorCode::InvalidMerkleRoot,
        5 => SetCustomMiningJobErrorCode::StalePrevHash,
        _ => SetCustomMiningJobErrorCode::Other,
    };

    let msg_len = cursor.read_u16::<LittleEndian>()? as usize;
    let mut msg_bytes = vec![0u8; msg_len];
    cursor.read_exact(&mut msg_bytes)?;
    let error_message = String::from_utf8_lossy(&msg_bytes).to_string();

    Ok(SetCustomMiningJobError {
        channel_id,
        request_id,
        error_code,
        error_message,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jd_client_config() {
        let config = JdClientConfig::default();
        assert_eq!(config.zebra_url, "http://127.0.0.1:8232");
    }
}
```

**Step 2: Verify compilation**

Run: `cargo check -p zcash-jd-client`
Expected: PASS

**Step 3: Commit**

```bash
git add crates/zcash-jd-client/src/client.rs
git commit -m "feat(jd-client): implement JD Client"
```

---

## Task 10: Integrate JD Server with Pool Server

**Files:**
- Modify: `crates/zcash-pool-server/src/server.rs`
- Modify: `crates/zcash-pool-server/src/lib.rs`
- Modify: `crates/zcash-pool-server/Cargo.toml`

**Step 1: Add JD Server dependency to pool server**

Update `crates/zcash-pool-server/Cargo.toml` to add:
```toml
zcash-jd-server = { path = "../zcash-jd-server" }
```

**Step 2: Integrate JD Server into PoolServer**

Add to `crates/zcash-pool-server/src/server.rs`:

1. Add import: `use zcash_jd_server::{JdServer, JdServerConfig, handle_jd_client};`
2. Add field to PoolServer: `jd_server: Arc<JdServer>,`
3. In `new()`: Create JD Server with shared payout tracker
4. In `run()`: Add TCP listener for JD clients (port 3334)
5. Spawn JD client handler tasks

**Step 3: Commit**

```bash
git add crates/zcash-pool-server/
git commit -m "feat(pool): integrate JD Server"
```

---

## Task 11: Add Integration Tests

**Files:**
- Create: `crates/zcash-jd-server/tests/integration_tests.rs`
- Create: `crates/zcash-jd-client/tests/integration_tests.rs`

**Step 1: Create JD Server integration tests**

Create `crates/zcash-jd-server/tests/integration_tests.rs`:

```rust
//! JD Server integration tests

use zcash_jd_server::*;
use zcash_pool_server::PayoutTracker;
use std::sync::Arc;

#[test]
fn test_token_flow() {
    let config = JdServerConfig::default();
    let payout = Arc::new(PayoutTracker::default());
    let server = JdServer::new(config, payout);

    // Allocate token
    let response = server.handle_allocate_token(1, "test-miner").unwrap();
    assert_eq!(response.request_id, 1);
    assert!(!response.mining_job_token.is_empty());
    assert!(response.async_mining_allowed);
}

#[tokio::test]
async fn test_job_declaration_flow() {
    let mut config = JdServerConfig::default();
    config.pool_payout_script = vec![0x76, 0xa9];
    let payout = Arc::new(PayoutTracker::default());
    let server = JdServer::new(config, payout);

    // Set current block
    let prev_hash = [0xaa; 32];
    server.set_current_prev_hash(prev_hash).await;

    // Allocate token
    let token_response = server.handle_allocate_token(1, "test-miner").unwrap();

    // Declare job
    let request = SetCustomMiningJob {
        channel_id: 1,
        request_id: 2,
        mining_job_token: token_response.mining_job_token,
        version: 5,
        prev_hash,
        merkle_root: [0xbb; 32],
        block_commitments: [0xcc; 32],
        time: 1700000000,
        bits: 0x1d00ffff,
        coinbase_tx: vec![0x01; 100],
    };

    let result = server.handle_declare_job(request).await;
    assert!(result.is_ok());
}

#[test]
fn test_message_codec_roundtrips() {
    use zcash_jd_server::codec::*;

    // AllocateMiningJobToken
    let msg = AllocateMiningJobToken {
        request_id: 42,
        user_identifier: "test".to_string(),
    };
    let encoded = encode_allocate_token(&msg).unwrap();
    let decoded = decode_allocate_token(&encoded[6..]).unwrap();
    assert_eq!(decoded.request_id, msg.request_id);

    // PushSolution
    let solution_msg = PushSolution {
        channel_id: 1,
        job_id: 42,
        version: 5,
        time: 1700000000,
        nonce: [0xff; 32],
        solution: [0xaa; 1344],
    };
    let encoded = encode_push_solution(&solution_msg).unwrap();
    let decoded = decode_push_solution(&encoded[6..]).unwrap();
    assert_eq!(decoded.job_id, solution_msg.job_id);
}
```

**Step 2: Commit**

```bash
git add crates/zcash-jd-server/tests/
git commit -m "test(jd): add integration tests"
```

---

## Task 12: Documentation and Final Verification

**Files:**
- Create: `crates/zcash-jd-server/README.md`
- Create: `crates/zcash-jd-client/README.md`
- Update: `README.md` (workspace root)

**Step 1: Create JD Server README**

Create `crates/zcash-jd-server/README.md`:

```markdown
# zcash-jd-server

Job Declaration Server for Zcash Stratum V2.

## Overview

Implements the SV2 Job Declaration Protocol (Coinbase-Only mode), allowing miners to:
- Request job declaration tokens
- Declare custom mining jobs built from their own templates
- Submit found blocks

## Integration

The JD Server is embedded in the Pool Server:

```rust
use zcash_jd_server::{JdServer, JdServerConfig};
use zcash_pool_server::PayoutTracker;

let config = JdServerConfig::default();
let payout_tracker = Arc::new(PayoutTracker::default());
let jd_server = JdServer::new(config, payout_tracker);
```

## Protocol Messages

| Message | Direction | Purpose |
|---------|-----------|---------|
| AllocateMiningJobToken | Client → Server | Request token |
| AllocateMiningJobToken.Success | Server → Client | Return token |
| SetCustomMiningJob | Client → Server | Declare job |
| SetCustomMiningJob.Success/Error | Server → Client | Acknowledge/reject |
| PushSolution | Client → Server | Submit block |

## Configuration

| Option | Default | Description |
|--------|---------|-------------|
| `token_lifetime` | 5 min | Token validity duration |
| `coinbase_output_max_additional_size` | 256 | Max miner coinbase addition |
| `async_mining_allowed` | true | Allow mining before ack |
```

**Step 2: Create JD Client README**

Create `crates/zcash-jd-client/README.md`:

```markdown
# zcash-jd-client

Job Declaration Client for Zcash Stratum V2.

## Overview

Standalone binary that enables miners to:
- Build custom block templates from local Zebra node
- Declare jobs to a pool's JD Server
- Submit found blocks to both Zebra and the pool

## Usage

```bash
zcash-jd-client \
  --zebra-url http://127.0.0.1:8232 \
  --pool-jd-addr 192.168.1.100:3334 \
  --user-id my-miner
```

## Options

| Option | Default | Description |
|--------|---------|-------------|
| `--zebra-url` | `http://127.0.0.1:8232` | Local Zebra RPC |
| `--pool-jd-addr` | `127.0.0.1:3334` | Pool JD Server |
| `--user-id` | `zcash-jd-client` | Miner identifier |
| `--poll-interval` | 1000 | Template poll ms |
| `--payout-address` | None | Optional extra output |

## Requirements

- Running Zebra node with RPC enabled
- Pool with JD Server support
```

**Step 3: Update workspace README**

Add Phase 4 to the project status and crates table.

**Step 4: Run all tests**

Run: `cargo test`
Expected: All tests pass

**Step 5: Commit**

```bash
git add README.md crates/zcash-jd-server/README.md crates/zcash-jd-client/README.md
git commit -m "docs: add Phase 4 documentation"
```

---

## Summary

Phase 4 creates two new crates:

1. **`zcash-jd-server`** - Embedded in pool, handles:
   - Token allocation for miners
   - Custom job declaration validation
   - Block solution reception

2. **`zcash-jd-client`** - Standalone binary for miners:
   - Connects to local Zebra for templates
   - Declares jobs to pool
   - Submits blocks to Zebra and pool

**Protocol: Coinbase-Only mode** - Miners declare jobs by coinbase, keeping transaction selection private.

**Not included (future phases):**
- Full-Template mode (DeclareMiningJob, ProvideMissingTransactions)
- Multi-miner downstream support
- TLS/Noise authentication
- Optimistic mining (mine before ack)
