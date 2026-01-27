# Stratum V2 Zcash Phase 2: Equihash Mining Protocol

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement the Equihash Mining Protocol with message types, solution validation, and adaptive difficulty for Stratum V2 on Zcash.

**Architecture:** Two new crates developed in parallel. `zcash-mining-protocol` defines binary message types (`NewEquihashJob`, `SubmitEquihashShare`) using SRI-compatible encoding. `zcash-equihash-validator` wraps the `equihash` crate for share validation with adaptive vardiff. Both integrate with the Phase 1 `zcash-template-provider`.

**Tech Stack:** Rust 1.75+, equihash (zcash-hackworks), tokio, serde, blake2b_simd

---

## Crate 1: zcash-mining-protocol

### Task 1: Initialize Mining Protocol Crate

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Create: `crates/zcash-mining-protocol/Cargo.toml`
- Create: `crates/zcash-mining-protocol/src/lib.rs`

**Step 1: Update workspace Cargo.toml**

Add to workspace dependencies in `Cargo.toml`:

```toml
[workspace.dependencies]
# ... existing deps ...
byteorder = "1.5"
```

**Step 2: Create mining-protocol crate Cargo.toml**

Create `crates/zcash-mining-protocol/Cargo.toml`:

```toml
[package]
name = "zcash-mining-protocol"
version.workspace = true
edition.workspace = true
license.workspace = true
description = "Zcash Mining Protocol messages for Stratum V2"

[dependencies]
serde.workspace = true
thiserror.workspace = true
byteorder.workspace = true

[dev-dependencies]
tokio = { workspace = true, features = ["test-util", "macros"] }
```

**Step 3: Create initial lib.rs**

Create `crates/zcash-mining-protocol/src/lib.rs`:

```rust
//! Zcash Mining Protocol for Stratum V2
//!
//! This crate defines the message types for Equihash mining:
//! - NewEquihashJob: Pool → Miner job distribution
//! - SubmitEquihashShare: Miner → Pool share submission
//! - Channel management messages

pub mod error;
pub mod messages;
pub mod codec;

pub use error::ProtocolError;
pub use messages::{NewEquihashJob, SubmitEquihashShare, SubmitSharesResponse, ShareResult};
```

**Step 4: Verify compilation**

Run: `cargo check -p zcash-mining-protocol`
Expected: FAIL (missing modules - expected at this stage)

**Step 5: Commit**

```bash
git add Cargo.toml crates/zcash-mining-protocol/
git commit -m "chore: initialize zcash-mining-protocol crate"
```

---

### Task 2: Define Protocol Error Types

**Files:**
- Create: `crates/zcash-mining-protocol/src/error.rs`

**Step 1: Write error module**

Create `crates/zcash-mining-protocol/src/error.rs`:

```rust
//! Protocol error types

use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    #[error("Invalid message type: {0}")]
    InvalidMessageType(u8),

    #[error("Message too short: expected {expected}, got {actual}")]
    MessageTooShort { expected: usize, actual: usize },

    #[error("Invalid nonce length: expected {expected}, got {actual}")]
    InvalidNonceLength { expected: usize, actual: usize },

    #[error("Invalid solution length: expected 1344, got {0}")]
    InvalidSolutionLength(usize),

    #[error("Unknown channel: {0}")]
    UnknownChannel(u32),

    #[error("Unknown job: {0}")]
    UnknownJob(u32),

    #[error("Stale share: job {job_id} superseded")]
    StaleShare { job_id: u32 },

    #[error("Duplicate share")]
    DuplicateShare,

    #[error("Invalid solution: {0}")]
    InvalidSolution(String),

    #[error("Target not met: share difficulty below threshold")]
    TargetNotMet,

    #[error("Encoding error: {0}")]
    EncodingError(String),
}

pub type Result<T> = std::result::Result<T, ProtocolError>;
```

**Step 2: Verify compilation**

Run: `cargo check -p zcash-mining-protocol`
Expected: FAIL (still missing modules)

**Step 3: Commit**

```bash
git add crates/zcash-mining-protocol/src/error.rs
git commit -m "feat(mining-protocol): add protocol error types"
```

---

### Task 3: Define Core Message Types

**Files:**
- Create: `crates/zcash-mining-protocol/src/messages.rs`

**Step 1: Write failing test**

Create `crates/zcash-mining-protocol/tests/message_tests.rs`:

```rust
use zcash_mining_protocol::messages::{NewEquihashJob, SubmitEquihashShare};

#[test]
fn test_new_equihash_job_creation() {
    let job = NewEquihashJob {
        channel_id: 1,
        job_id: 42,
        future_job: false,
        version: 5,
        prev_hash: [0xaa; 32],
        merkle_root: [0xbb; 32],
        block_commitments: [0xcc; 32],
        nonce_1: vec![0x01, 0x02, 0x03, 0x04],
        nonce_2_len: 28,
        time: 1700000000,
        bits: 0x1d00ffff,
        target: [0x00; 32],
        clean_jobs: true,
    };

    assert_eq!(job.channel_id, 1);
    assert_eq!(job.job_id, 42);
    assert_eq!(job.nonce_1.len() + job.nonce_2_len as usize, 32);
}

#[test]
fn test_submit_equihash_share_creation() {
    let share = SubmitEquihashShare {
        channel_id: 1,
        sequence_number: 100,
        job_id: 42,
        nonce_2: vec![0xff; 28],
        time: 1700000001,
        solution: [0x00; 1344],
    };

    assert_eq!(share.solution.len(), 1344);
    assert_eq!(share.nonce_2.len(), 28);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p zcash-mining-protocol --test message_tests`
Expected: FAIL with "cannot find value"

**Step 3: Write messages module**

Create `crates/zcash-mining-protocol/src/messages.rs`:

```rust
//! Zcash Mining Protocol message types
//!
//! Message types for Equihash mining over Stratum V2:
//! - NewEquihashJob: Sent by pool to distribute mining work
//! - SubmitEquihashShare: Sent by miner to submit solutions
//! - SubmitSharesResponse: Pool's response to share submission

use serde::{Deserialize, Serialize};

/// Message type identifiers (SRI-compatible range for extension)
pub mod message_types {
    /// NewEquihashJob message type
    pub const NEW_EQUIHASH_JOB: u8 = 0x20;
    /// SubmitEquihashShare message type
    pub const SUBMIT_EQUIHASH_SHARE: u8 = 0x21;
    /// SubmitSharesResponse message type
    pub const SUBMIT_SHARES_RESPONSE: u8 = 0x22;
    /// SetTarget message type (difficulty adjustment)
    pub const SET_TARGET: u8 = 0x23;
}

/// Pool → Miner: New mining job
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewEquihashJob {
    /// Channel this job belongs to
    pub channel_id: u32,
    /// Unique job identifier
    pub job_id: u32,
    /// If true, this job should be queued for future use
    pub future_job: bool,
    /// Block version
    pub version: u32,
    /// Previous block hash (32 bytes)
    pub prev_hash: [u8; 32],
    /// Merkle root of transactions (32 bytes)
    pub merkle_root: [u8; 32],
    /// hashBlockCommitments for NU5+ (32 bytes)
    pub block_commitments: [u8; 32],
    /// Pool-assigned nonce prefix (variable length)
    pub nonce_1: Vec<u8>,
    /// Length of miner-controlled nonce portion
    pub nonce_2_len: u8,
    /// Block timestamp
    pub time: u32,
    /// Compact difficulty target (nbits)
    pub bits: u32,
    /// Share difficulty target (256-bit, pool-set)
    pub target: [u8; 32],
    /// If true, discard all previous jobs
    pub clean_jobs: bool,
}

impl NewEquihashJob {
    /// Total nonce length (must equal 32)
    pub fn total_nonce_len(&self) -> usize {
        self.nonce_1.len() + self.nonce_2_len as usize
    }

    /// Validate that nonce lengths sum to 32
    pub fn validate_nonce_len(&self) -> bool {
        self.total_nonce_len() == 32
    }

    /// Construct the 140-byte header for Equihash input
    /// (version || prev_hash || merkle_root || block_commitments || time || bits || nonce)
    pub fn build_header(&self, nonce: &[u8; 32]) -> [u8; 140] {
        let mut header = [0u8; 140];
        header[0..4].copy_from_slice(&self.version.to_le_bytes());
        header[4..36].copy_from_slice(&self.prev_hash);
        header[36..68].copy_from_slice(&self.merkle_root);
        header[68..100].copy_from_slice(&self.block_commitments);
        header[100..104].copy_from_slice(&self.time.to_le_bytes());
        header[104..108].copy_from_slice(&self.bits.to_le_bytes());
        header[108..140].copy_from_slice(nonce);
        header
    }

    /// Combine nonce_1 and nonce_2 into full 32-byte nonce
    pub fn build_nonce(&self, nonce_2: &[u8]) -> Option<[u8; 32]> {
        if nonce_2.len() != self.nonce_2_len as usize {
            return None;
        }
        let mut nonce = [0u8; 32];
        nonce[..self.nonce_1.len()].copy_from_slice(&self.nonce_1);
        nonce[self.nonce_1.len()..].copy_from_slice(nonce_2);
        Some(nonce)
    }
}

/// Miner → Pool: Submit Equihash share
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmitEquihashShare {
    /// Channel this share belongs to
    pub channel_id: u32,
    /// Sequence number for response matching
    pub sequence_number: u32,
    /// Job this share is for
    pub job_id: u32,
    /// Miner-controlled nonce portion
    pub nonce_2: Vec<u8>,
    /// Block timestamp (may differ from job time)
    pub time: u32,
    /// Equihash (200,9) solution (1344 bytes)
    pub solution: [u8; 1344],
}

impl SubmitEquihashShare {
    /// Equihash (200,9) solution size
    pub const SOLUTION_SIZE: usize = 1344;

    /// Validate solution length
    pub fn validate_solution_len(&self) -> bool {
        self.solution.len() == Self::SOLUTION_SIZE
    }
}

/// Pool → Miner: Response to share submission
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmitSharesResponse {
    /// Channel ID
    pub channel_id: u32,
    /// Sequence number matching the submission
    pub sequence_number: u32,
    /// Result of share validation
    pub result: ShareResult,
}

/// Result of share validation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShareResult {
    /// Share accepted
    Accepted,
    /// Share rejected with reason
    Rejected(RejectReason),
}

/// Reasons for share rejection
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RejectReason {
    /// Job ID not found or expired
    StaleJob,
    /// Duplicate share already submitted
    Duplicate,
    /// Solution does not verify
    InvalidSolution,
    /// Share difficulty below target
    LowDifficulty,
    /// Other error
    Other(String),
}

/// Pool → Miner: Update share difficulty target
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetTarget {
    /// Channel ID
    pub channel_id: u32,
    /// New target (256-bit, little-endian)
    pub target: [u8; 32],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nonce_validation() {
        let job = NewEquihashJob {
            channel_id: 1,
            job_id: 1,
            future_job: false,
            version: 5,
            prev_hash: [0; 32],
            merkle_root: [0; 32],
            block_commitments: [0; 32],
            nonce_1: vec![0; 8],
            nonce_2_len: 24,
            time: 0,
            bits: 0,
            target: [0; 32],
            clean_jobs: false,
        };
        assert!(job.validate_nonce_len());

        let bad_job = NewEquihashJob {
            nonce_1: vec![0; 8],
            nonce_2_len: 20, // 8 + 20 = 28, not 32
            ..job.clone()
        };
        assert!(!bad_job.validate_nonce_len());
    }

    #[test]
    fn test_build_nonce() {
        let job = NewEquihashJob {
            channel_id: 1,
            job_id: 1,
            future_job: false,
            version: 5,
            prev_hash: [0; 32],
            merkle_root: [0; 32],
            block_commitments: [0; 32],
            nonce_1: vec![0x01, 0x02, 0x03, 0x04],
            nonce_2_len: 28,
            time: 0,
            bits: 0,
            target: [0; 32],
            clean_jobs: false,
        };

        let nonce_2 = vec![0xaa; 28];
        let full_nonce = job.build_nonce(&nonce_2).unwrap();

        assert_eq!(&full_nonce[0..4], &[0x01, 0x02, 0x03, 0x04]);
        assert_eq!(&full_nonce[4..32], &[0xaa; 28]);
    }

    #[test]
    fn test_build_header() {
        let job = NewEquihashJob {
            channel_id: 1,
            job_id: 1,
            future_job: false,
            version: 5,
            prev_hash: [0xaa; 32],
            merkle_root: [0xbb; 32],
            block_commitments: [0xcc; 32],
            nonce_1: vec![0; 4],
            nonce_2_len: 28,
            time: 0x12345678,
            bits: 0xaabbccdd,
            target: [0; 32],
            clean_jobs: false,
        };

        let nonce = [0xff; 32];
        let header = job.build_header(&nonce);

        assert_eq!(header.len(), 140);
        // Version at offset 0 (little-endian)
        assert_eq!(&header[0..4], &[0x05, 0x00, 0x00, 0x00]);
        // prev_hash at offset 4
        assert_eq!(&header[4..36], &[0xaa; 32]);
        // merkle_root at offset 36
        assert_eq!(&header[36..68], &[0xbb; 32]);
        // block_commitments at offset 68
        assert_eq!(&header[68..100], &[0xcc; 32]);
        // time at offset 100 (little-endian)
        assert_eq!(&header[100..104], &[0x78, 0x56, 0x34, 0x12]);
        // bits at offset 104 (little-endian)
        assert_eq!(&header[104..108], &[0xdd, 0xcc, 0xbb, 0xaa]);
        // nonce at offset 108
        assert_eq!(&header[108..140], &[0xff; 32]);
    }
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test -p zcash-mining-protocol`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/zcash-mining-protocol/
git commit -m "feat(mining-protocol): add core message types"
```

---

### Task 4: Implement Binary Codec

**Files:**
- Create: `crates/zcash-mining-protocol/src/codec.rs`
- Create: `crates/zcash-mining-protocol/tests/codec_tests.rs`

**Step 1: Write failing test**

Create `crates/zcash-mining-protocol/tests/codec_tests.rs`:

```rust
use zcash_mining_protocol::codec::{encode_message, decode_message, MessageFrame};
use zcash_mining_protocol::messages::{NewEquihashJob, SubmitEquihashShare};

#[test]
fn test_new_equihash_job_roundtrip() {
    let job = NewEquihashJob {
        channel_id: 1,
        job_id: 42,
        future_job: false,
        version: 5,
        prev_hash: [0xaa; 32],
        merkle_root: [0xbb; 32],
        block_commitments: [0xcc; 32],
        nonce_1: vec![0x01, 0x02, 0x03, 0x04],
        nonce_2_len: 28,
        time: 1700000000,
        bits: 0x1d00ffff,
        target: [0x00; 32],
        clean_jobs: true,
    };

    let encoded = encode_message(&job).unwrap();
    let decoded: NewEquihashJob = decode_message(&encoded).unwrap();

    assert_eq!(job, decoded);
}

#[test]
fn test_submit_share_roundtrip() {
    let share = SubmitEquihashShare {
        channel_id: 1,
        sequence_number: 100,
        job_id: 42,
        nonce_2: vec![0xff; 28],
        time: 1700000001,
        solution: [0x12; 1344],
    };

    let encoded = encode_message(&share).unwrap();
    let decoded: SubmitEquihashShare = decode_message(&encoded).unwrap();

    assert_eq!(share, decoded);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p zcash-mining-protocol --test codec_tests`
Expected: FAIL

**Step 3: Implement codec module**

Create `crates/zcash-mining-protocol/src/codec.rs`:

```rust
//! Binary codec for Zcash Mining Protocol messages
//!
//! Wire format follows SRI conventions:
//! - Little-endian integers
//! - Variable-length fields prefixed with length byte
//! - Fixed arrays without length prefix

use crate::error::{ProtocolError, Result};
use crate::messages::{
    message_types, NewEquihashJob, RejectReason, SetTarget, ShareResult,
    SubmitEquihashShare, SubmitSharesResponse,
};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io::{Cursor, Read, Write};

/// Message frame header
#[derive(Debug, Clone)]
pub struct MessageFrame {
    /// Extension type (0 for mining protocol)
    pub extension_type: u16,
    /// Message type identifier
    pub msg_type: u8,
    /// Payload length
    pub length: u32,
}

impl MessageFrame {
    /// Header size in bytes
    pub const HEADER_SIZE: usize = 6;

    /// Encode frame header
    pub fn encode(&self) -> [u8; 6] {
        let mut buf = [0u8; 6];
        buf[0..2].copy_from_slice(&self.extension_type.to_le_bytes());
        buf[2] = self.msg_type;
        // Length is 3 bytes (24-bit)
        buf[3..6].copy_from_slice(&self.length.to_le_bytes()[0..3]);
        buf
    }

    /// Decode frame header
    pub fn decode(data: &[u8]) -> Result<Self> {
        if data.len() < Self::HEADER_SIZE {
            return Err(ProtocolError::MessageTooShort {
                expected: Self::HEADER_SIZE,
                actual: data.len(),
            });
        }
        let extension_type = u16::from_le_bytes([data[0], data[1]]);
        let msg_type = data[2];
        let mut len_bytes = [0u8; 4];
        len_bytes[0..3].copy_from_slice(&data[3..6]);
        let length = u32::from_le_bytes(len_bytes);

        Ok(Self {
            extension_type,
            msg_type,
            length,
        })
    }
}

/// Encode a NewEquihashJob message
pub fn encode_new_equihash_job(job: &NewEquihashJob) -> Result<Vec<u8>> {
    let mut payload = Vec::new();

    payload.write_u32::<LittleEndian>(job.channel_id).unwrap();
    payload.write_u32::<LittleEndian>(job.job_id).unwrap();
    payload.write_u8(if job.future_job { 1 } else { 0 }).unwrap();
    payload.write_u32::<LittleEndian>(job.version).unwrap();
    payload.write_all(&job.prev_hash).unwrap();
    payload.write_all(&job.merkle_root).unwrap();
    payload.write_all(&job.block_commitments).unwrap();
    // Variable-length nonce_1
    payload.write_u8(job.nonce_1.len() as u8).unwrap();
    payload.write_all(&job.nonce_1).unwrap();
    payload.write_u8(job.nonce_2_len).unwrap();
    payload.write_u32::<LittleEndian>(job.time).unwrap();
    payload.write_u32::<LittleEndian>(job.bits).unwrap();
    payload.write_all(&job.target).unwrap();
    payload.write_u8(if job.clean_jobs { 1 } else { 0 }).unwrap();

    let frame = MessageFrame {
        extension_type: 0,
        msg_type: message_types::NEW_EQUIHASH_JOB,
        length: payload.len() as u32,
    };

    let mut result = frame.encode().to_vec();
    result.extend(payload);
    Ok(result)
}

/// Decode a NewEquihashJob message
pub fn decode_new_equihash_job(data: &[u8]) -> Result<NewEquihashJob> {
    let frame = MessageFrame::decode(data)?;
    if frame.msg_type != message_types::NEW_EQUIHASH_JOB {
        return Err(ProtocolError::InvalidMessageType(frame.msg_type));
    }

    let payload = &data[MessageFrame::HEADER_SIZE..];
    let mut cursor = Cursor::new(payload);

    let channel_id = cursor.read_u32::<LittleEndian>().map_err(|e| {
        ProtocolError::EncodingError(e.to_string())
    })?;
    let job_id = cursor.read_u32::<LittleEndian>().map_err(|e| {
        ProtocolError::EncodingError(e.to_string())
    })?;
    let future_job = cursor.read_u8().map_err(|e| {
        ProtocolError::EncodingError(e.to_string())
    })? != 0;
    let version = cursor.read_u32::<LittleEndian>().map_err(|e| {
        ProtocolError::EncodingError(e.to_string())
    })?;

    let mut prev_hash = [0u8; 32];
    cursor.read_exact(&mut prev_hash).map_err(|e| {
        ProtocolError::EncodingError(e.to_string())
    })?;

    let mut merkle_root = [0u8; 32];
    cursor.read_exact(&mut merkle_root).map_err(|e| {
        ProtocolError::EncodingError(e.to_string())
    })?;

    let mut block_commitments = [0u8; 32];
    cursor.read_exact(&mut block_commitments).map_err(|e| {
        ProtocolError::EncodingError(e.to_string())
    })?;

    let nonce_1_len = cursor.read_u8().map_err(|e| {
        ProtocolError::EncodingError(e.to_string())
    })? as usize;
    let mut nonce_1 = vec![0u8; nonce_1_len];
    cursor.read_exact(&mut nonce_1).map_err(|e| {
        ProtocolError::EncodingError(e.to_string())
    })?;

    let nonce_2_len = cursor.read_u8().map_err(|e| {
        ProtocolError::EncodingError(e.to_string())
    })?;

    let time = cursor.read_u32::<LittleEndian>().map_err(|e| {
        ProtocolError::EncodingError(e.to_string())
    })?;
    let bits = cursor.read_u32::<LittleEndian>().map_err(|e| {
        ProtocolError::EncodingError(e.to_string())
    })?;

    let mut target = [0u8; 32];
    cursor.read_exact(&mut target).map_err(|e| {
        ProtocolError::EncodingError(e.to_string())
    })?;

    let clean_jobs = cursor.read_u8().map_err(|e| {
        ProtocolError::EncodingError(e.to_string())
    })? != 0;

    Ok(NewEquihashJob {
        channel_id,
        job_id,
        future_job,
        version,
        prev_hash,
        merkle_root,
        block_commitments,
        nonce_1,
        nonce_2_len,
        time,
        bits,
        target,
        clean_jobs,
    })
}

/// Encode a SubmitEquihashShare message
pub fn encode_submit_share(share: &SubmitEquihashShare) -> Result<Vec<u8>> {
    let mut payload = Vec::new();

    payload.write_u32::<LittleEndian>(share.channel_id).unwrap();
    payload.write_u32::<LittleEndian>(share.sequence_number).unwrap();
    payload.write_u32::<LittleEndian>(share.job_id).unwrap();
    // Variable-length nonce_2
    payload.write_u8(share.nonce_2.len() as u8).unwrap();
    payload.write_all(&share.nonce_2).unwrap();
    payload.write_u32::<LittleEndian>(share.time).unwrap();
    payload.write_all(&share.solution).unwrap();

    let frame = MessageFrame {
        extension_type: 0,
        msg_type: message_types::SUBMIT_EQUIHASH_SHARE,
        length: payload.len() as u32,
    };

    let mut result = frame.encode().to_vec();
    result.extend(payload);
    Ok(result)
}

/// Decode a SubmitEquihashShare message
pub fn decode_submit_share(data: &[u8]) -> Result<SubmitEquihashShare> {
    let frame = MessageFrame::decode(data)?;
    if frame.msg_type != message_types::SUBMIT_EQUIHASH_SHARE {
        return Err(ProtocolError::InvalidMessageType(frame.msg_type));
    }

    let payload = &data[MessageFrame::HEADER_SIZE..];
    let mut cursor = Cursor::new(payload);

    let channel_id = cursor.read_u32::<LittleEndian>().map_err(|e| {
        ProtocolError::EncodingError(e.to_string())
    })?;
    let sequence_number = cursor.read_u32::<LittleEndian>().map_err(|e| {
        ProtocolError::EncodingError(e.to_string())
    })?;
    let job_id = cursor.read_u32::<LittleEndian>().map_err(|e| {
        ProtocolError::EncodingError(e.to_string())
    })?;

    let nonce_2_len = cursor.read_u8().map_err(|e| {
        ProtocolError::EncodingError(e.to_string())
    })? as usize;
    let mut nonce_2 = vec![0u8; nonce_2_len];
    cursor.read_exact(&mut nonce_2).map_err(|e| {
        ProtocolError::EncodingError(e.to_string())
    })?;

    let time = cursor.read_u32::<LittleEndian>().map_err(|e| {
        ProtocolError::EncodingError(e.to_string())
    })?;

    let mut solution = [0u8; 1344];
    cursor.read_exact(&mut solution).map_err(|e| {
        ProtocolError::EncodingError(e.to_string())
    })?;

    Ok(SubmitEquihashShare {
        channel_id,
        sequence_number,
        job_id,
        nonce_2,
        time,
        solution,
    })
}

/// Generic encode trait
pub trait Encodable {
    fn encode(&self) -> Result<Vec<u8>>;
}

/// Generic decode trait
pub trait Decodable: Sized {
    fn decode(data: &[u8]) -> Result<Self>;
}

impl Encodable for NewEquihashJob {
    fn encode(&self) -> Result<Vec<u8>> {
        encode_new_equihash_job(self)
    }
}

impl Decodable for NewEquihashJob {
    fn decode(data: &[u8]) -> Result<Self> {
        decode_new_equihash_job(data)
    }
}

impl Encodable for SubmitEquihashShare {
    fn encode(&self) -> Result<Vec<u8>> {
        encode_submit_share(self)
    }
}

impl Decodable for SubmitEquihashShare {
    fn decode(data: &[u8]) -> Result<Self> {
        decode_submit_share(data)
    }
}

/// Convenience functions for generic encode/decode
pub fn encode_message<T: Encodable>(msg: &T) -> Result<Vec<u8>> {
    msg.encode()
}

pub fn decode_message<T: Decodable>(data: &[u8]) -> Result<T> {
    T::decode(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_roundtrip() {
        let frame = MessageFrame {
            extension_type: 0x1234,
            msg_type: 0x20,
            length: 0x123456,
        };

        let encoded = frame.encode();
        let decoded = MessageFrame::decode(&encoded).unwrap();

        assert_eq!(frame.extension_type, decoded.extension_type);
        assert_eq!(frame.msg_type, decoded.msg_type);
        assert_eq!(frame.length, decoded.length);
    }
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test -p zcash-mining-protocol`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/zcash-mining-protocol/
git commit -m "feat(mining-protocol): implement binary codec for messages"
```

---

## Crate 2: zcash-equihash-validator

### Task 5: Initialize Validator Crate

**Files:**
- Modify: `Cargo.toml` (workspace root)
- Create: `crates/zcash-equihash-validator/Cargo.toml`
- Create: `crates/zcash-equihash-validator/src/lib.rs`

**Step 1: Update workspace Cargo.toml**

Add to workspace dependencies:

```toml
[workspace.dependencies]
# ... existing deps ...
equihash = "0.2"
```

**Step 2: Create validator crate Cargo.toml**

Create `crates/zcash-equihash-validator/Cargo.toml`:

```toml
[package]
name = "zcash-equihash-validator"
version.workspace = true
edition.workspace = true
license.workspace = true
description = "Equihash solution validation and difficulty management for Zcash mining"

[dependencies]
equihash = { workspace = true }
thiserror.workspace = true
tracing.workspace = true
tokio.workspace = true

# Local dependencies
zcash-mining-protocol = { path = "../zcash-mining-protocol" }
zcash-template-provider = { path = "../zcash-template-provider" }

[dev-dependencies]
tokio = { workspace = true, features = ["test-util", "macros", "rt-multi-thread"] }
hex.workspace = true
```

**Step 3: Create initial lib.rs**

Create `crates/zcash-equihash-validator/src/lib.rs`:

```rust
//! Equihash solution validation for Zcash Stratum V2
//!
//! This crate provides:
//! - Equihash (200,9) solution verification
//! - Share difficulty validation
//! - Adaptive variable difficulty (vardiff) algorithm

pub mod error;
pub mod validator;
pub mod difficulty;
pub mod vardiff;

pub use error::ValidationError;
pub use validator::EquihashValidator;
pub use difficulty::{Target, compact_to_target, target_to_difficulty};
pub use vardiff::VardiffController;
```

**Step 4: Verify compilation**

Run: `cargo check -p zcash-equihash-validator`
Expected: FAIL (missing modules)

**Step 5: Commit**

```bash
git add Cargo.toml crates/zcash-equihash-validator/
git commit -m "chore: initialize zcash-equihash-validator crate"
```

---

### Task 6: Implement Validation Error Types

**Files:**
- Create: `crates/zcash-equihash-validator/src/error.rs`

**Step 1: Write error module**

Create `crates/zcash-equihash-validator/src/error.rs`:

```rust
//! Validation error types

use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum ValidationError {
    #[error("Invalid Equihash solution: {0}")]
    InvalidSolution(String),

    #[error("Solution does not meet target difficulty")]
    TargetNotMet,

    #[error("Invalid header length: expected 140, got {0}")]
    InvalidHeaderLength(usize),

    #[error("Invalid solution length: expected 1344, got {0}")]
    InvalidSolutionLength(usize),

    #[error("Invalid nonce length: expected 32, got {0}")]
    InvalidNonceLength(usize),

    #[error("Hash computation failed: {0}")]
    HashError(String),
}

pub type Result<T> = std::result::Result<T, ValidationError>;
```

**Step 2: Commit**

```bash
git add crates/zcash-equihash-validator/src/error.rs
git commit -m "feat(validator): add validation error types"
```

---

### Task 7: Implement Equihash Validator

**Files:**
- Create: `crates/zcash-equihash-validator/src/validator.rs`
- Create: `crates/zcash-equihash-validator/tests/validator_tests.rs`

**Step 1: Write failing test**

Create `crates/zcash-equihash-validator/tests/validator_tests.rs`:

```rust
use zcash_equihash_validator::{EquihashValidator, ValidationError};

#[test]
fn test_validator_creation() {
    let validator = EquihashValidator::new();
    assert_eq!(validator.n(), 200);
    assert_eq!(validator.k(), 9);
}

#[test]
fn test_invalid_solution_rejected() {
    let validator = EquihashValidator::new();

    // All-zero header and solution should fail verification
    let header = [0u8; 140];
    let solution = [0u8; 1344];

    let result = validator.verify_solution(&header, &solution);
    assert!(result.is_err());
}

#[test]
fn test_wrong_solution_length_rejected() {
    let validator = EquihashValidator::new();

    let header = [0u8; 140];
    let bad_solution = [0u8; 100]; // Wrong length

    let result = validator.verify_solution(&header, &bad_solution);
    assert!(matches!(result, Err(ValidationError::InvalidSolutionLength(_))));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p zcash-equihash-validator --test validator_tests`
Expected: FAIL

**Step 3: Implement validator module**

Create `crates/zcash-equihash-validator/src/validator.rs`:

```rust
//! Equihash solution validator
//!
//! Wraps the `equihash` crate to provide verification for Zcash's (200,9) parameters.

use crate::error::{Result, ValidationError};
use tracing::{debug, trace};

/// Zcash Equihash parameters
pub const EQUIHASH_N: u32 = 200;
pub const EQUIHASH_K: u32 = 9;

/// Expected solution size for (200,9): 512 * 21 bits / 8 = 1344 bytes
pub const SOLUTION_SIZE: usize = 1344;

/// Equihash solution validator for Zcash
#[derive(Debug, Clone)]
pub struct EquihashValidator {
    n: u32,
    k: u32,
}

impl Default for EquihashValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl EquihashValidator {
    /// Create a new validator with Zcash parameters (200,9)
    pub fn new() -> Self {
        Self {
            n: EQUIHASH_N,
            k: EQUIHASH_K,
        }
    }

    /// Get the n parameter
    pub fn n(&self) -> u32 {
        self.n
    }

    /// Get the k parameter
    pub fn k(&self) -> u32 {
        self.k
    }

    /// Verify an Equihash solution
    ///
    /// # Arguments
    /// * `header` - The 140-byte block header (including nonce, excluding solution)
    /// * `solution` - The 1344-byte Equihash solution
    ///
    /// # Returns
    /// * `Ok(())` if the solution is valid
    /// * `Err(ValidationError)` if verification fails
    pub fn verify_solution(&self, header: &[u8], solution: &[u8]) -> Result<()> {
        // Validate input lengths
        if header.len() != 140 {
            return Err(ValidationError::InvalidHeaderLength(header.len()));
        }
        if solution.len() != SOLUTION_SIZE {
            return Err(ValidationError::InvalidSolutionLength(solution.len()));
        }

        trace!(
            "Verifying Equihash solution: header_len={}, solution_len={}",
            header.len(),
            solution.len()
        );

        // The equihash crate expects:
        // - input: the 108-byte header prefix (before nonce)
        // - nonce: the 32-byte nonce
        // - solution: the 1344-byte solution
        let input = &header[..108];
        let nonce = &header[108..140];

        equihash::is_valid_solution(self.n, self.k, input, nonce, solution)
            .map_err(|e| ValidationError::InvalidSolution(format!("{:?}", e)))?;

        debug!("Equihash solution verified successfully");
        Ok(())
    }

    /// Verify a solution and check if it meets the target difficulty
    ///
    /// # Arguments
    /// * `header` - The 140-byte block header
    /// * `solution` - The 1344-byte Equihash solution
    /// * `target` - The 256-bit target (solution hash must be <= target)
    ///
    /// # Returns
    /// * `Ok(hash)` if valid and meets target, returns the solution hash
    /// * `Err(ValidationError)` if verification fails or target not met
    pub fn verify_share(
        &self,
        header: &[u8],
        solution: &[u8],
        target: &[u8; 32],
    ) -> Result<[u8; 32]> {
        // First verify the solution is valid
        self.verify_solution(header, solution)?;

        // Compute the hash of header + solution
        let hash = self.compute_solution_hash(header, solution)?;

        // Check if hash meets target (hash <= target, little-endian comparison)
        if !self.meets_target(&hash, target) {
            return Err(ValidationError::TargetNotMet);
        }

        Ok(hash)
    }

    /// Compute the double SHA-256 hash of the block header + solution
    /// (This is what gets compared against the target)
    fn compute_solution_hash(&self, header: &[u8], solution: &[u8]) -> Result<[u8; 32]> {
        use blake2b_simd::Params;

        // Zcash uses BLAKE2b for block hashing
        // The block hash is BLAKE2b-256 of the full header including solution
        let mut data = Vec::with_capacity(header.len() + 3 + solution.len());
        data.extend_from_slice(header);
        // CompactSize encoding for solution length (1344 = 0xfd 0x40 0x05)
        data.push(0xfd);
        data.push(0x40);
        data.push(0x05);
        data.extend_from_slice(solution);

        let hash = Params::new()
            .hash_length(32)
            .personal(b"ZcashBlockHash\0\0")
            .hash(&data);

        let mut result = [0u8; 32];
        result.copy_from_slice(hash.as_bytes());
        Ok(result)
    }

    /// Check if a hash meets the target (hash <= target, little-endian)
    fn meets_target(&self, hash: &[u8; 32], target: &[u8; 32]) -> bool {
        // Compare as little-endian 256-bit integers
        // Start from the most significant byte (index 31) and work down
        for i in (0..32).rev() {
            if hash[i] < target[i] {
                return true;
            }
            if hash[i] > target[i] {
                return false;
            }
        }
        true // Equal is also valid
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_meets_target() {
        let validator = EquihashValidator::new();

        // Hash is less than target
        let hash = [0x00; 32];
        let target = [0xff; 32];
        assert!(validator.meets_target(&hash, &target));

        // Hash equals target
        let same = [0x42; 32];
        assert!(validator.meets_target(&same, &same));

        // Hash is greater than target
        let high_hash = [0xff; 32];
        let low_target = [0x00; 32];
        assert!(!validator.meets_target(&high_hash, &low_target));
    }

    #[test]
    fn test_parameter_values() {
        let validator = EquihashValidator::new();
        assert_eq!(validator.n(), 200);
        assert_eq!(validator.k(), 9);
    }
}
```

**Step 4: Add blake2b_simd to validator crate**

Update `crates/zcash-equihash-validator/Cargo.toml`:

```toml
[dependencies]
equihash.workspace = true
thiserror.workspace = true
tracing.workspace = true
tokio.workspace = true
blake2b_simd.workspace = true

# Local dependencies
zcash-mining-protocol = { path = "../zcash-mining-protocol" }
zcash-template-provider = { path = "../zcash-template-provider" }
```

**Step 5: Run tests**

Run: `cargo test -p zcash-equihash-validator`
Expected: PASS

**Step 6: Commit**

```bash
git add crates/zcash-equihash-validator/
git commit -m "feat(validator): implement Equihash solution verification"
```

---

### Task 8: Implement Difficulty Utilities

**Files:**
- Create: `crates/zcash-equihash-validator/src/difficulty.rs`

**Step 1: Write failing test**

Add to `crates/zcash-equihash-validator/tests/validator_tests.rs`:

```rust
use zcash_equihash_validator::difficulty::{Target, compact_to_target, difficulty_to_target};

#[test]
fn test_compact_to_target() {
    // Standard testnet difficulty
    let compact = 0x1d00ffff_u32;
    let target = compact_to_target(compact);

    // Should produce a target with leading zeros
    assert!(target.0[31] == 0x00);
}

#[test]
fn test_difficulty_to_target() {
    // Difficulty 1 should give max target
    let target = difficulty_to_target(1.0);
    assert!(target.0[31] > 0);

    // Higher difficulty = lower target
    let harder = difficulty_to_target(2.0);
    assert!(harder < target);
}
```

**Step 2: Implement difficulty module**

Create `crates/zcash-equihash-validator/src/difficulty.rs`:

```rust
//! Difficulty and target calculations for Zcash mining
//!
//! Zcash uses a 256-bit target. A valid share must have a hash <= target.
//! Difficulty is inversely proportional to target.

use std::cmp::Ordering;

/// 256-bit target value
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target(pub [u8; 32]);

impl Target {
    /// Create a target from bytes (little-endian)
    pub fn from_le_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Get bytes as little-endian
    pub fn to_le_bytes(&self) -> [u8; 32] {
        self.0
    }

    /// Maximum target (difficulty 1)
    pub fn max() -> Self {
        // Zcash's powLimit for mainnet
        // 0007ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
        let mut target = [0xff; 32];
        target[28] = 0x07;
        target[29] = 0x00;
        target[30] = 0x00;
        target[31] = 0x00;
        Self(target)
    }

    /// Check if a hash meets this target (hash <= target)
    pub fn is_met_by(&self, hash: &[u8; 32]) -> bool {
        // Compare as little-endian 256-bit integers
        for i in (0..32).rev() {
            match hash[i].cmp(&self.0[i]) {
                Ordering::Less => return true,
                Ordering::Greater => return false,
                Ordering::Equal => continue,
            }
        }
        true // Equal is valid
    }
}

impl PartialOrd for Target {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Target {
    fn cmp(&self, other: &Self) -> Ordering {
        // Compare as little-endian 256-bit integers
        for i in (0..32).rev() {
            match self.0[i].cmp(&other.0[i]) {
                Ordering::Equal => continue,
                other => return other,
            }
        }
        Ordering::Equal
    }
}

/// Convert compact "bits" representation to full target
///
/// The compact format is: mantissa * 256^(exponent-3)
/// where exponent is the first byte and mantissa is the next 3 bytes
pub fn compact_to_target(compact: u32) -> Target {
    let bytes = compact.to_be_bytes();
    let exponent = bytes[0] as usize;
    let mantissa = ((bytes[1] as u32) << 16) | ((bytes[2] as u32) << 8) | (bytes[3] as u32);

    let mut target = [0u8; 32];

    if exponent <= 3 {
        // Mantissa fits in lower bytes
        let shift = 3 - exponent;
        let value = mantissa >> (8 * shift);
        target[0] = (value & 0xff) as u8;
        if exponent >= 2 {
            target[1] = ((value >> 8) & 0xff) as u8;
        }
        if exponent >= 3 {
            target[2] = ((value >> 16) & 0xff) as u8;
        }
    } else {
        // Place mantissa at exponent-3 position
        let pos = exponent - 3;
        if pos < 32 {
            target[pos] = (mantissa & 0xff) as u8;
        }
        if pos + 1 < 32 {
            target[pos + 1] = ((mantissa >> 8) & 0xff) as u8;
        }
        if pos + 2 < 32 {
            target[pos + 2] = ((mantissa >> 16) & 0xff) as u8;
        }
    }

    Target(target)
}

/// Convert target to difficulty
///
/// Difficulty = max_target / target
pub fn target_to_difficulty(target: &Target) -> f64 {
    let max = Target::max();

    // Convert to f64 for division (approximate but sufficient for display)
    let max_val = target_to_f64(&max);
    let target_val = target_to_f64(target);

    if target_val == 0.0 {
        return f64::INFINITY;
    }

    max_val / target_val
}

/// Convert difficulty to target
///
/// Target = max_target / difficulty
pub fn difficulty_to_target(difficulty: f64) -> Target {
    if difficulty <= 0.0 {
        return Target::max();
    }

    let max = Target::max();
    let max_val = target_to_f64(&max);
    let target_val = max_val / difficulty;

    f64_to_target(target_val)
}

/// Convert target to approximate f64 (loses precision for very large values)
fn target_to_f64(target: &Target) -> f64 {
    let mut result = 0.0f64;
    for i in (0..32).rev() {
        result = result * 256.0 + (target.0[i] as f64);
    }
    result
}

/// Convert f64 to target (approximate)
fn f64_to_target(mut value: f64) -> Target {
    let mut target = [0u8; 32];
    for i in 0..32 {
        let byte = (value % 256.0) as u8;
        target[i] = byte;
        value = (value - byte as f64) / 256.0;
    }
    Target(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_target_comparison() {
        let low = Target([0x01; 32]);
        let high = Target([0xff; 32]);

        assert!(low < high);
        assert!(high > low);
    }

    #[test]
    fn test_is_met_by() {
        let target = Target([0x10; 32]);
        let good_hash = [0x0f; 32];
        let bad_hash = [0x11; 32];

        assert!(target.is_met_by(&good_hash));
        assert!(!target.is_met_by(&bad_hash));
    }

    #[test]
    fn test_difficulty_roundtrip() {
        let difficulties = [1.0, 2.0, 100.0, 1000.0, 1_000_000.0];

        for &diff in &difficulties {
            let target = difficulty_to_target(diff);
            let recovered = target_to_difficulty(&target);
            // Allow 1% error due to floating point
            let ratio = recovered / diff;
            assert!(ratio > 0.99 && ratio < 1.01, "diff={}, recovered={}", diff, recovered);
        }
    }
}
```

**Step 3: Run tests**

Run: `cargo test -p zcash-equihash-validator`
Expected: PASS

**Step 4: Commit**

```bash
git add crates/zcash-equihash-validator/src/difficulty.rs
git commit -m "feat(validator): implement difficulty and target calculations"
```

---

### Task 9: Implement Adaptive Vardiff Controller

**Files:**
- Create: `crates/zcash-equihash-validator/src/vardiff.rs`

**Step 1: Write failing test**

Create `crates/zcash-equihash-validator/tests/vardiff_tests.rs`:

```rust
use zcash_equihash_validator::vardiff::{VardiffController, VardiffConfig};
use std::time::Duration;

#[test]
fn test_vardiff_creation() {
    let config = VardiffConfig::default();
    let controller = VardiffController::new(config);

    assert!(controller.current_difficulty() > 0.0);
}

#[test]
fn test_vardiff_adjusts_up_on_fast_shares() {
    let config = VardiffConfig {
        target_shares_per_minute: 6.0,
        min_difficulty: 1.0,
        max_difficulty: 1_000_000.0,
        retarget_interval: Duration::from_secs(30),
        variance_tolerance: 0.25,
    };
    let mut controller = VardiffController::new(config);

    // Simulate very fast share submission (10 shares in 30 seconds)
    let initial_diff = controller.current_difficulty();
    for _ in 0..10 {
        controller.record_share();
    }
    std::thread::sleep(Duration::from_millis(100)); // Small delay to trigger retarget

    controller.maybe_retarget();
    let new_diff = controller.current_difficulty();

    // Difficulty should increase
    assert!(new_diff > initial_diff);
}

#[test]
fn test_vardiff_adjusts_down_on_slow_shares() {
    let config = VardiffConfig {
        target_shares_per_minute: 6.0,
        min_difficulty: 1.0,
        max_difficulty: 1_000_000.0,
        retarget_interval: Duration::from_secs(1), // Short interval for testing
        variance_tolerance: 0.25,
    };
    let mut controller = VardiffController::new(config);

    // Start at higher difficulty
    controller.set_difficulty(100.0);

    // Simulate slow share submission (1 share in 1 second when expecting 0.1)
    controller.record_share();
    std::thread::sleep(Duration::from_secs(1));

    controller.maybe_retarget();
    let new_diff = controller.current_difficulty();

    // Difficulty should decrease
    assert!(new_diff < 100.0);
}
```

**Step 2: Implement vardiff module**

Create `crates/zcash-equihash-validator/src/vardiff.rs`:

```rust
//! Adaptive Variable Difficulty (Vardiff) Controller
//!
//! Adjusts share difficulty per-miner to maintain a target share rate.
//! Designed for Equihash's ~15-30 second solve times on ASICs.

use crate::difficulty::{difficulty_to_target, Target};
use std::time::{Duration, Instant};
use tracing::{debug, info};

/// Configuration for the vardiff algorithm
#[derive(Debug, Clone)]
pub struct VardiffConfig {
    /// Target shares per minute from each miner
    pub target_shares_per_minute: f64,
    /// Minimum allowed difficulty
    pub min_difficulty: f64,
    /// Maximum allowed difficulty
    pub max_difficulty: f64,
    /// How often to recalculate difficulty
    pub retarget_interval: Duration,
    /// Tolerance for share rate variance (0.25 = 25%)
    pub variance_tolerance: f64,
}

impl Default for VardiffConfig {
    fn default() -> Self {
        Self {
            // For Equihash ASICs (~420 KSol/s), target 4-6 shares/min
            target_shares_per_minute: 5.0,
            min_difficulty: 1.0,
            max_difficulty: 1_000_000_000.0,
            retarget_interval: Duration::from_secs(60),
            variance_tolerance: 0.25,
        }
    }
}

/// Per-miner vardiff state
#[derive(Debug)]
pub struct VardiffController {
    config: VardiffConfig,
    current_difficulty: f64,
    shares_since_retarget: u32,
    last_retarget: Instant,
    window_start: Instant,
}

impl VardiffController {
    /// Create a new vardiff controller
    pub fn new(config: VardiffConfig) -> Self {
        let now = Instant::now();
        Self {
            current_difficulty: config.min_difficulty,
            config,
            shares_since_retarget: 0,
            last_retarget: now,
            window_start: now,
        }
    }

    /// Get current difficulty
    pub fn current_difficulty(&self) -> f64 {
        self.current_difficulty
    }

    /// Get current target as 256-bit value
    pub fn current_target(&self) -> Target {
        difficulty_to_target(self.current_difficulty)
    }

    /// Set difficulty directly (for initial connection setup)
    pub fn set_difficulty(&mut self, difficulty: f64) {
        self.current_difficulty = difficulty.clamp(
            self.config.min_difficulty,
            self.config.max_difficulty,
        );
        self.reset_window();
        info!("Difficulty set to {:.2}", self.current_difficulty);
    }

    /// Record a submitted share
    pub fn record_share(&mut self) {
        self.shares_since_retarget += 1;
    }

    /// Check if retargeting is needed and adjust difficulty
    ///
    /// Returns `Some(new_difficulty)` if difficulty changed, `None` otherwise
    pub fn maybe_retarget(&mut self) -> Option<f64> {
        let elapsed = self.last_retarget.elapsed();

        if elapsed < self.config.retarget_interval {
            return None;
        }

        let minutes = elapsed.as_secs_f64() / 60.0;
        let actual_rate = self.shares_since_retarget as f64 / minutes;
        let target_rate = self.config.target_shares_per_minute;

        debug!(
            "Vardiff check: {} shares in {:.1}s = {:.2}/min (target: {:.2}/min)",
            self.shares_since_retarget,
            elapsed.as_secs_f64(),
            actual_rate,
            target_rate
        );

        // Check if we're within tolerance
        let ratio = actual_rate / target_rate;
        let lower_bound = 1.0 - self.config.variance_tolerance;
        let upper_bound = 1.0 + self.config.variance_tolerance;

        if ratio >= lower_bound && ratio <= upper_bound {
            // Within tolerance, no change needed
            self.reset_window();
            return None;
        }

        // Calculate new difficulty
        // If shares are coming too fast (ratio > 1), increase difficulty
        // If shares are coming too slow (ratio < 1), decrease difficulty
        let adjustment = if ratio > 0.0 { ratio } else { 0.5 };
        let new_difficulty = (self.current_difficulty * adjustment).clamp(
            self.config.min_difficulty,
            self.config.max_difficulty,
        );

        // Apply smoothing to avoid large jumps
        let smoothed = self.current_difficulty * 0.5 + new_difficulty * 0.5;
        let final_difficulty = smoothed.clamp(
            self.config.min_difficulty,
            self.config.max_difficulty,
        );

        if (final_difficulty - self.current_difficulty).abs() > 0.01 {
            info!(
                "Vardiff adjustment: {:.2} -> {:.2} (share rate: {:.2}/min)",
                self.current_difficulty, final_difficulty, actual_rate
            );
            self.current_difficulty = final_difficulty;
            self.reset_window();
            return Some(final_difficulty);
        }

        self.reset_window();
        None
    }

    /// Reset the measurement window
    fn reset_window(&mut self) {
        self.shares_since_retarget = 0;
        self.last_retarget = Instant::now();
        self.window_start = Instant::now();
    }

    /// Get statistics about current window
    pub fn stats(&self) -> VardiffStats {
        let elapsed = self.window_start.elapsed();
        let minutes = elapsed.as_secs_f64() / 60.0;
        let rate = if minutes > 0.0 {
            self.shares_since_retarget as f64 / minutes
        } else {
            0.0
        };

        VardiffStats {
            current_difficulty: self.current_difficulty,
            shares_in_window: self.shares_since_retarget,
            window_duration: elapsed,
            current_rate: rate,
            target_rate: self.config.target_shares_per_minute,
        }
    }
}

/// Statistics from vardiff controller
#[derive(Debug, Clone)]
pub struct VardiffStats {
    pub current_difficulty: f64,
    pub shares_in_window: u32,
    pub window_duration: Duration,
    pub current_rate: f64,
    pub target_rate: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = VardiffConfig::default();
        assert!(config.target_shares_per_minute > 0.0);
        assert!(config.min_difficulty > 0.0);
        assert!(config.max_difficulty > config.min_difficulty);
    }

    #[test]
    fn test_difficulty_clamping() {
        let config = VardiffConfig {
            min_difficulty: 10.0,
            max_difficulty: 100.0,
            ..Default::default()
        };
        let mut controller = VardiffController::new(config);

        controller.set_difficulty(5.0);
        assert_eq!(controller.current_difficulty(), 10.0);

        controller.set_difficulty(500.0);
        assert_eq!(controller.current_difficulty(), 100.0);
    }

    #[test]
    fn test_target_generation() {
        let config = VardiffConfig::default();
        let controller = VardiffController::new(config);

        let target = controller.current_target();
        // Target should be non-zero
        assert!(target.0.iter().any(|&b| b != 0));
    }
}
```

**Step 3: Run tests**

Run: `cargo test -p zcash-equihash-validator`
Expected: PASS

**Step 4: Commit**

```bash
git add crates/zcash-equihash-validator/
git commit -m "feat(validator): implement adaptive vardiff controller"
```

---

## Integration

### Task 10: Create Integration Test with Test Vectors

**Files:**
- Create: `crates/zcash-equihash-validator/tests/integration_tests.rs`
- Create: `crates/zcash-equihash-validator/tests/test_vectors.rs`

**Step 1: Add test vectors module**

Create `crates/zcash-equihash-validator/tests/test_vectors.rs`:

```rust
//! Test vectors from Zcash mainnet/testnet blocks

/// A test vector containing a valid Equihash solution
pub struct TestVector {
    pub name: &'static str,
    pub header_hex: &'static str,
    pub solution_hex: &'static str,
    pub height: u64,
}

/// Zcash mainnet block test vectors
/// These are real blocks from the Zcash blockchain
pub const TEST_VECTORS: &[TestVector] = &[
    // Genesis block (simplified - real genesis has different structure)
    // For actual testing, use blocks from after NU5 activation
];

/// Helper to decode hex to bytes
pub fn hex_to_bytes(hex: &str) -> Vec<u8> {
    hex::decode(hex).expect("Invalid hex string")
}

/// Helper to decode hex to fixed array
pub fn hex_to_array<const N: usize>(hex: &str) -> [u8; N] {
    let bytes = hex_to_bytes(hex);
    assert_eq!(bytes.len(), N, "Hex string has wrong length");
    let mut arr = [0u8; N];
    arr.copy_from_slice(&bytes);
    arr
}
```

**Step 2: Create integration test**

Create `crates/zcash-equihash-validator/tests/integration_tests.rs`:

```rust
//! Integration tests for the Equihash validator

mod test_vectors;

use zcash_equihash_validator::{EquihashValidator, VardiffController, VardiffConfig};
use zcash_equihash_validator::difficulty::{difficulty_to_target, target_to_difficulty};
use zcash_mining_protocol::messages::{NewEquihashJob, SubmitEquihashShare};

#[test]
fn test_full_share_validation_flow() {
    let validator = EquihashValidator::new();

    // Create a job
    let job = NewEquihashJob {
        channel_id: 1,
        job_id: 1,
        future_job: false,
        version: 5,
        prev_hash: [0xaa; 32],
        merkle_root: [0xbb; 32],
        block_commitments: [0xcc; 32],
        nonce_1: vec![0x01, 0x02, 0x03, 0x04],
        nonce_2_len: 28,
        time: 1700000000,
        bits: 0x1d00ffff,
        target: [0xff; 32], // Easy target for testing
        clean_jobs: false,
    };

    // Create a share (with invalid solution - just testing the flow)
    let share = SubmitEquihashShare {
        channel_id: 1,
        sequence_number: 1,
        job_id: 1,
        nonce_2: vec![0xff; 28],
        time: 1700000000,
        solution: [0x00; 1344],
    };

    // Build full nonce
    let nonce = job.build_nonce(&share.nonce_2).unwrap();
    assert_eq!(nonce.len(), 32);

    // Build header
    let header = job.build_header(&nonce);
    assert_eq!(header.len(), 140);

    // Verification should fail (invalid solution)
    let result = validator.verify_solution(&header, &share.solution);
    assert!(result.is_err());
}

#[test]
fn test_vardiff_integration_with_protocol() {
    let config = VardiffConfig {
        target_shares_per_minute: 5.0,
        min_difficulty: 1.0,
        max_difficulty: 1_000_000.0,
        retarget_interval: std::time::Duration::from_secs(60),
        variance_tolerance: 0.25,
    };
    let controller = VardiffController::new(config);

    // Get target for job
    let target = controller.current_target();

    // Create job with this target
    let job = NewEquihashJob {
        channel_id: 1,
        job_id: 1,
        future_job: false,
        version: 5,
        prev_hash: [0; 32],
        merkle_root: [0; 32],
        block_commitments: [0; 32],
        nonce_1: vec![0; 8],
        nonce_2_len: 24,
        time: 0,
        bits: 0x1d00ffff,
        target: target.to_le_bytes(),
        clean_jobs: false,
    };

    assert_eq!(job.target, target.to_le_bytes());
}

#[test]
fn test_difficulty_to_target_integration() {
    // Test that difficulty values produce sensible targets
    let difficulties = [1.0, 10.0, 100.0, 1000.0];

    for diff in difficulties {
        let target = difficulty_to_target(diff);
        let recovered = target_to_difficulty(&target);

        // Should be within 1% due to floating point
        let ratio = recovered / diff;
        assert!(
            ratio > 0.99 && ratio < 1.01,
            "Difficulty {} recovered as {}", diff, recovered
        );
    }
}
```

**Step 3: Run integration tests**

Run: `cargo test -p zcash-equihash-validator --test integration_tests`
Expected: PASS

**Step 4: Commit**

```bash
git add crates/zcash-equihash-validator/tests/
git commit -m "test(validator): add integration tests"
```

---

### Task 11: Add End-to-End Example

**Files:**
- Create: `crates/zcash-equihash-validator/examples/validate_share.rs`

**Step 1: Create example binary**

Create `crates/zcash-equihash-validator/examples/validate_share.rs`:

```rust
//! Example: Validate an Equihash share
//!
//! This demonstrates the full flow from job creation to share validation.
//!
//! Usage: cargo run --example validate_share

use zcash_equihash_validator::{EquihashValidator, VardiffController, VardiffConfig};
use zcash_mining_protocol::messages::{NewEquihashJob, SubmitEquihashShare, ShareResult, RejectReason};

fn main() {
    tracing_subscriber::fmt::init();

    println!("=== Zcash Equihash Share Validation Demo ===\n");

    // Create validator
    let validator = EquihashValidator::new();
    println!("Validator initialized with Equihash({}, {})", validator.n(), validator.k());

    // Create vardiff controller
    let config = VardiffConfig::default();
    let mut vardiff = VardiffController::new(config);
    println!("Vardiff initialized: target {:.1} shares/min", vardiff.stats().target_rate);
    println!("Current difficulty: {:.2}", vardiff.current_difficulty());

    // Create a mining job
    let job = NewEquihashJob {
        channel_id: 1,
        job_id: 42,
        future_job: false,
        version: 5,
        prev_hash: [0xab; 32],
        merkle_root: [0xcd; 32],
        block_commitments: [0xef; 32],
        nonce_1: vec![0x00, 0x00, 0x00, 0x01], // Pool prefix
        nonce_2_len: 28,
        time: 1700000000,
        bits: 0x1d00ffff,
        target: vardiff.current_target().to_le_bytes(),
        clean_jobs: true,
    };

    println!("\n=== Mining Job ===");
    println!("Job ID: {}", job.job_id);
    println!("Height implied by prev_hash");
    println!("Nonce_1 length: {} bytes", job.nonce_1.len());
    println!("Nonce_2 length: {} bytes", job.nonce_2_len);

    // Simulate a share submission (with dummy solution)
    let share = SubmitEquihashShare {
        channel_id: 1,
        sequence_number: 1,
        job_id: 42,
        nonce_2: vec![0xff; 28],
        time: 1700000001,
        solution: [0x00; 1344], // Invalid solution for demo
    };

    println!("\n=== Share Submission ===");
    println!("Sequence: {}", share.sequence_number);
    println!("Solution size: {} bytes", share.solution.len());

    // Build full nonce and header
    let nonce = job.build_nonce(&share.nonce_2).expect("Invalid nonce_2 length");
    let header = job.build_header(&nonce);

    println!("\n=== Validation ===");
    println!("Header size: {} bytes", header.len());
    println!("Nonce: {}", hex::encode(&nonce[..8])); // First 8 bytes

    // Validate the solution
    match validator.verify_solution(&header, &share.solution) {
        Ok(()) => {
            println!("Solution: VALID");

            // Check target
            let target = vardiff.current_target();
            match validator.verify_share(&header, &share.solution, &target.to_le_bytes()) {
                Ok(hash) => {
                    println!("Share: ACCEPTED");
                    println!("Hash: {}", hex::encode(&hash[..8]));
                    vardiff.record_share();
                }
                Err(e) => {
                    println!("Share: REJECTED ({})", e);
                }
            }
        }
        Err(e) => {
            println!("Solution: INVALID ({})", e);
            println!("Share: REJECTED");
        }
    }

    // Show vardiff stats
    let stats = vardiff.stats();
    println!("\n=== Vardiff Stats ===");
    println!("Difficulty: {:.2}", stats.current_difficulty);
    println!("Shares in window: {}", stats.shares_in_window);
    println!("Current rate: {:.2}/min", stats.current_rate);
    println!("Target rate: {:.2}/min", stats.target_rate);

    println!("\n=== Demo Complete ===");
    println!("Note: This demo uses an invalid solution for illustration.");
    println!("Real shares require valid Equihash solutions from mining hardware.");
}
```

**Step 2: Update Cargo.toml for example**

Ensure `crates/zcash-equihash-validator/Cargo.toml` has:

```toml
[dev-dependencies]
tokio = { workspace = true, features = ["test-util", "macros", "rt-multi-thread"] }
hex.workspace = true
tracing-subscriber.workspace = true
```

**Step 3: Verify compilation**

Run: `cargo build --example validate_share`
Expected: PASS

**Step 4: Commit**

```bash
git add crates/zcash-equihash-validator/
git commit -m "feat(validator): add validate_share example"
```

---

### Task 12: Documentation and Final Verification

**Files:**
- Create: `crates/zcash-mining-protocol/README.md`
- Create: `crates/zcash-equihash-validator/README.md`
- Update: `README.md` (workspace root)

**Step 1: Create mining-protocol README**

Create `crates/zcash-mining-protocol/README.md`:

```markdown
# zcash-mining-protocol

Zcash Mining Protocol messages for Stratum V2.

## Overview

This crate defines the binary message types for Equihash mining:

- `NewEquihashJob` - Pool → Miner job distribution
- `SubmitEquihashShare` - Miner → Pool share submission
- `SubmitSharesResponse` - Pool → Miner share acknowledgment
- `SetTarget` - Pool → Miner difficulty adjustment

## Message Format

Messages use SRI-compatible binary encoding:
- 6-byte frame header (extension_type, msg_type, length)
- Little-endian integers
- Variable-length fields prefixed with length byte

## Usage

```rust
use zcash_mining_protocol::messages::{NewEquihashJob, SubmitEquihashShare};
use zcash_mining_protocol::codec::{encode_message, decode_message};

// Create a job
let job = NewEquihashJob {
    channel_id: 1,
    job_id: 42,
    // ... other fields
};

// Encode for transmission
let bytes = encode_message(&job)?;

// Decode received message
let decoded: NewEquihashJob = decode_message(&bytes)?;
```

## Wire Format

### NewEquihashJob (0x20)

| Field | Type | Size |
|-------|------|------|
| channel_id | u32 | 4 |
| job_id | u32 | 4 |
| future_job | bool | 1 |
| version | u32 | 4 |
| prev_hash | [u8; 32] | 32 |
| merkle_root | [u8; 32] | 32 |
| block_commitments | [u8; 32] | 32 |
| nonce_1_len | u8 | 1 |
| nonce_1 | [u8] | var |
| nonce_2_len | u8 | 1 |
| time | u32 | 4 |
| bits | u32 | 4 |
| target | [u8; 32] | 32 |
| clean_jobs | bool | 1 |

### SubmitEquihashShare (0x21)

| Field | Type | Size |
|-------|------|------|
| channel_id | u32 | 4 |
| sequence_number | u32 | 4 |
| job_id | u32 | 4 |
| nonce_2_len | u8 | 1 |
| nonce_2 | [u8] | var |
| time | u32 | 4 |
| solution | [u8; 1344] | 1344 |
```

**Step 2: Create validator README**

Create `crates/zcash-equihash-validator/README.md`:

```markdown
# zcash-equihash-validator

Equihash solution validation and difficulty management for Zcash mining.

## Overview

This crate provides:

- **EquihashValidator** - Verifies Equihash (200,9) solutions
- **VardiffController** - Adaptive difficulty adjustment per-miner
- **Difficulty utilities** - Target/difficulty conversion functions

## Usage

```rust
use zcash_equihash_validator::{EquihashValidator, VardiffController, VardiffConfig};

// Create validator
let validator = EquihashValidator::new();

// Verify a solution
let header: [u8; 140] = /* 140-byte header including nonce */;
let solution: [u8; 1344] = /* 1344-byte Equihash solution */;

validator.verify_solution(&header, &solution)?;

// With difficulty check
let target: [u8; 32] = /* 256-bit target */;
let hash = validator.verify_share(&header, &solution, &target)?;
```

### Vardiff

```rust
let config = VardiffConfig {
    target_shares_per_minute: 5.0,
    min_difficulty: 1.0,
    max_difficulty: 1_000_000_000.0,
    retarget_interval: Duration::from_secs(60),
    variance_tolerance: 0.25,
};

let mut vardiff = VardiffController::new(config);

// On share received
vardiff.record_share();

// Periodically check for retarget
if let Some(new_diff) = vardiff.maybe_retarget() {
    // Send SetTarget message to miner
}
```

## Equihash Parameters

Zcash uses Equihash (200, 9):
- n = 200, k = 9
- Solution size: 1344 bytes (512 × 21-bit indices)
- Memory requirement: ~144 MB for verification
- Solve time: ~15-30 seconds on ASIC hardware

## Dependencies

- `equihash` crate (zcash-hackworks) for core verification
- `blake2b_simd` for block hashing
```

**Step 3: Update workspace README**

Update `README.md`:

```markdown
# Stratum V2 for Zcash

Implementation of Stratum V2 mining protocol for Zcash with support for decentralized block template construction.

## Project Status

- Phase 1: Zcash Template Provider - **Complete**
- Phase 2: Equihash Mining Protocol - **Complete**

## Crates

| Crate | Description |
|-------|-------------|
| `zcash-template-provider` | Template Provider interfacing with Zebra |
| `zcash-mining-protocol` | SV2 message types for Equihash mining |
| `zcash-equihash-validator` | Share validation and vardiff |

## Building

```bash
cargo build --release
```

## Testing

```bash
cargo test
```

## Examples

```bash
# Fetch a template from Zebra
cargo run --example fetch_template

# Demonstrate share validation
cargo run --example validate_share
```

## Architecture

See [docs/stratum-v2-planning.md](docs/stratum-v2-planning.md) for the full implementation plan.

## License

MIT OR Apache-2.0
```

**Step 4: Run all tests**

Run: `cargo test`
Expected: All tests pass

**Step 5: Commit**

```bash
git add README.md crates/zcash-mining-protocol/README.md crates/zcash-equihash-validator/README.md
git commit -m "docs: add README files for Phase 2 crates"
```

---

## Summary

Phase 2 creates two new crates:

1. **zcash-mining-protocol** (Tasks 1-4)
   - Protocol error types
   - Message types: `NewEquihashJob`, `SubmitEquihashShare`, `SubmitSharesResponse`, `SetTarget`
   - Binary codec with SRI-compatible framing

2. **zcash-equihash-validator** (Tasks 5-11)
   - Equihash (200,9) solution verification via `equihash` crate
   - Difficulty/target utilities
   - Adaptive vardiff controller for ~15-30s solve times
   - Integration tests and examples

**Testing:** Unit tests throughout, integration tests with protocol messages, example binaries for manual testing. Full end-to-end testing with Zebra regtest deferred to Phase 3 (Pool Server) when we have the infrastructure to generate real shares.

**Dependencies:**
- Phase 1 types (Hash256, EquihashHeader) reused
- `equihash` crate for verification
- SRI binary encoding format for wire compatibility
