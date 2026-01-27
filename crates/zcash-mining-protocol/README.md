# zcash-mining-protocol

Zcash Mining Protocol messages for Stratum V2.

## Overview

This crate defines the binary message types for Equihash mining:

- `NewEquihashJob` - Pool -> Miner job distribution
- `SubmitEquihashShare` - Miner -> Pool share submission
- `SubmitSharesResponse` - Pool -> Miner share acknowledgment
- `SetTarget` - Pool -> Miner difficulty adjustment

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
    future_job: false,
    version: 5,
    prev_hash: [0; 32],
    merkle_root: [0; 32],
    block_commitments: [0; 32],
    nonce_1: vec![0; 8],
    nonce_2_len: 24,
    time: 1234567890,
    bits: 0x1d00ffff,
    target: [0xff; 32],
    clean_jobs: true,
};

// Encode for transmission
let bytes = encode_message(&job)?;

// Decode received message
let decoded: NewEquihashJob = decode_message(&bytes)?;
```

## Wire Format

### Frame Header (6 bytes)

| Field | Type | Size | Description |
|-------|------|------|-------------|
| extension_type | u16 | 2 | Extension type (0 for mining) |
| msg_type | u8 | 1 | Message type identifier |
| length | u24 | 3 | Payload length |

### NewEquihashJob (0x20)

| Field | Type | Size | Description |
|-------|------|------|-------------|
| channel_id | u32 | 4 | Channel identifier |
| job_id | u32 | 4 | Unique job identifier |
| future_job | bool | 1 | Queue for future use |
| version | u32 | 4 | Block version |
| prev_hash | [u8; 32] | 32 | Previous block hash |
| merkle_root | [u8; 32] | 32 | Transaction merkle root |
| block_commitments | [u8; 32] | 32 | NU5+ hashBlockCommitments |
| nonce_1_len | u8 | 1 | Length of pool nonce |
| nonce_1 | [u8] | var | Pool-assigned nonce prefix |
| nonce_2_len | u8 | 1 | Miner nonce portion length |
| time | u32 | 4 | Block timestamp |
| bits | u32 | 4 | Compact difficulty (nbits) |
| target | [u8; 32] | 32 | Share difficulty target |
| clean_jobs | bool | 1 | Discard previous jobs |

### SubmitEquihashShare (0x21)

| Field | Type | Size | Description |
|-------|------|------|-------------|
| channel_id | u32 | 4 | Channel identifier |
| sequence_number | u32 | 4 | Response matching |
| job_id | u32 | 4 | Job this share is for |
| nonce_2_len | u8 | 1 | Length of miner nonce |
| nonce_2 | [u8] | var | Miner-controlled nonce |
| time | u32 | 4 | Block timestamp |
| solution | [u8; 1344] | 1344 | Equihash (200,9) solution |

### SubmitSharesResponse (0x22)

| Field | Type | Size | Description |
|-------|------|------|-------------|
| channel_id | u32 | 4 | Channel identifier |
| sequence_number | u32 | 4 | Matching request sequence |
| result | u8 | 1 | 0 = Accepted, 1+ = Rejected |

### SetTarget (0x23)

| Field | Type | Size | Description |
|-------|------|------|-------------|
| channel_id | u32 | 4 | Channel identifier |
| target | [u8; 32] | 32 | New difficulty target |

## Nonce Structure

The 32-byte nonce is split between pool and miner:
- `nonce_1`: Pool-assigned prefix (prevents work overlap)
- `nonce_2`: Miner-controlled suffix (for solution search)

The `nonce_1.len() + nonce_2_len` must equal 32.

## License

MIT OR Apache-2.0
