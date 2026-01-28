# Zcash Stratum V2 Protocol Reference

Complete message format documentation for Zcash Stratum V2.

## Message Frame

All messages use this frame format:

```
+----------------+----------------+------------------+
|  Message Type  |  Payload Len   |     Payload      |
|    (1 byte)    |   (2 bytes)    |   (variable)     |
+----------------+----------------+------------------+
```

- All integers are **little-endian**
- Strings are length-prefixed (1-byte length + UTF-8 bytes)
- Arrays are count-prefixed (varies by type)

## Noise Encryption

When Noise is enabled, the frame above is encrypted after the handshake completes.

**Pattern:** `Noise_NK_25519_ChaChaPoly_BLAKE2s`

**Handshake:**
```
Client                              Server
  |                                    |
  |------- e, es (48 bytes) --------->|
  |<------ e, ee (48 bytes) ----------|
  |                                    |
  |  [Encrypted frames from here]      |
```

## Message Types

### Connection Setup

| Type | Name | Direction |
|------|------|-----------|
| 0x00 | SetupConnection | C → S |
| 0x01 | SetupConnection.Success | S → C |
| 0x02 | SetupConnection.Error | S → C |

### Mining Channel

| Type | Name | Direction |
|------|------|-----------|
| 0x10 | OpenStandardMiningChannel | C → S |
| 0x11 | OpenStandardMiningChannel.Success | S → C |
| 0x12 | OpenStandardMiningChannel.Error | S → C |
| 0x14 | UpdateChannel | C → S |
| 0x15 | UpdateChannel.Error | S → C |

### Mining Operations

| Type | Name | Direction |
|------|------|-----------|
| 0x1C | SubmitSharesStandard | C → S |
| 0x1D | SubmitShares.Success | S → C |
| 0x1E | SubmitShares.Error | S → C |
| 0x1F | NewMiningJob | S → C |
| 0x20 | NewPrevHash | S → C |
| 0x21 | SetTarget | S → C |

### Job Declaration (Coinbase-Only)

| Type | Name | Direction |
|------|------|-----------|
| 0x50 | AllocateMiningJobToken | C → S |
| 0x51 | AllocateMiningJobToken.Success | S → C |
| 0x52 | SetCustomMiningJob | C → S |
| 0x53 | SetCustomMiningJob.Success | S → C |
| 0x54 | SetCustomMiningJob.Error | S → C |
| 0x55 | PushSolution | C → S |

### Job Declaration (Full-Template)

| Type | Name | Direction |
|------|------|-----------|
| 0x56 | SetFullTemplateJob | C → S |
| 0x57 | SetFullTemplateJob.Success | S → C |
| 0x58 | SetFullTemplateJob.Error | S → C |
| 0x59 | GetMissingTransactions | S → C |
| 0x5A | ProvideMissingTransactions | C → S |

## Message Definitions

### SetupConnection (0x00)

```
+----------+----------+----------+----------+
| protocol | min_ver  | max_ver  |  flags   |
|  2 bytes |  2 bytes |  2 bytes | 4 bytes  |
+----------+----------+----------+----------+
| host_len |    endpoint_host (UTF-8)       |
|  1 byte  |    variable                    |
+----------+--------------------------------+
|   port   | vendor_len |  vendor (UTF-8)  |
|  2 bytes |   1 byte   |   variable       |
+----------+------------+------------------+
| hw_len   | hardware_version (UTF-8)       |
| 1 byte   |    variable                    |
+----------+--------------------------------+
| fw_len   |   firmware (UTF-8)             |
| 1 byte   |    variable                    |
+----------+--------------------------------+
| dev_len  |   device_id (UTF-8)            |
| 1 byte   |    variable                    |
+----------+--------------------------------+
```

**Fields:**
- `protocol`: 0 = Mining Protocol
- `min_version` / `max_version`: Protocol version range (currently 2)
- `flags`: Feature flags (0 for base features)

### SetupConnection.Success (0x01)

```
+----------+----------+
| used_ver |  flags   |
|  2 bytes | 4 bytes  |
+----------+----------+
```

### OpenStandardMiningChannel (0x10)

```
+------------+------------+--------------+-------------------+
| request_id |  user_len  |  user_ident  | nominal_hashrate  |
|  4 bytes   |   1 byte   |   variable   |     4 bytes       |
+------------+------------+--------------+-------------------+
| max_extranonce |
|     2 bytes    |
+----------------+
```

**Fields:**
- `request_id`: Client-generated request identifier
- `user_identity`: Worker identifier (e.g., "wallet.worker")
- `nominal_hashrate`: Expected hashrate as float32
- `max_extranonce`: Maximum extranonce size client supports

### OpenStandardMiningChannel.Success (0x11)

```
+------------+------------+---------------+------------------+
| request_id | channel_id | extranonce_len| extranonce       |
|  4 bytes   |  4 bytes   |    1 byte     | variable (≤32)   |
+------------+------------+---------------+------------------+
| group_channel_id |
|      4 bytes     |
+------------------+
```

### NewMiningJob (0x1F)

```
+------------+----------+------------+
| channel_id |  job_id  | future_job |
|  4 bytes   | 4 bytes  |   1 byte   |
+------------+----------+------------+
|  version   |     prev_hash (32 bytes)     |
|  4 bytes   |                              |
+------------+------------------------------+
|        merkle_root (32 bytes)             |
+-------------------------------------------+
|     block_commitments (32 bytes)          |
+-------------------------------------------+
|   nbits    |   ntime    |
|  4 bytes   |  4 bytes   |
+------------+------------+
```

**Fields:**
- `channel_id`: Mining channel this job belongs to
- `job_id`: Server-assigned job identifier
- `future_job`: If true, don't start mining until NewPrevHash
- `version`: Block version (5 for NU5+)
- `prev_hash`: Previous block hash (32 bytes)
- `merkle_root`: Merkle root of transactions (32 bytes)
- `block_commitments`: hashBlockCommitments (32 bytes, NU5+)
- `nbits`: Compact difficulty target
- `ntime`: Block timestamp

### SetTarget (0x21)

```
+------------+----------------------------------+
| channel_id |        max_target (32 bytes)     |
|  4 bytes   |                                  |
+------------+----------------------------------+
```

**Fields:**
- `max_target`: Maximum hash value for valid share (big-endian)

### NewPrevHash (0x20)

```
+------------+----------------------------------+
| channel_id |        prev_hash (32 bytes)      |
|  4 bytes   |                                  |
+------------+----------------------------------+
| min_ntime  |   nbits    |
|  4 bytes   |  4 bytes   |
+------------+------------+
```

**Semantics:** Invalidates all jobs with different prev_hash.

### SubmitSharesStandard (0x1C)

```
+------------+----------+----------+
| channel_id | seq_num  |  job_id  |
|  4 bytes   | 4 bytes  | 4 bytes  |
+------------+----------+----------+
|           nonce (32 bytes)       |
+----------------------------------+
|   ntime    | solution_len |
|  4 bytes   |   2 bytes    |
+------------+--------------+------+
|     solution (1344 bytes)        |
+----------------------------------+
```

**Fields:**
- `channel_id`: Mining channel
- `sequence_number`: Incrementing submission counter
- `job_id`: Job this solution is for
- `nonce`: Full 32-byte nonce (NONCE_1 || NONCE_2)
- `ntime`: Block timestamp (may be rolled)
- `solution`: Equihash solution (1344 bytes for 200,9)

### SubmitShares.Success (0x1D)

```
+------------+--------------+------------------+-----------------+
| channel_id | last_seq_num | new_submits_ok   | new_shares_sum  |
|  4 bytes   |   4 bytes    |     4 bytes      |     8 bytes     |
+------------+--------------+------------------+-----------------+
```

### SubmitShares.Error (0x1E)

```
+------------+----------+------------+
| channel_id | seq_num  | error_code |
|  4 bytes   | 4 bytes  |  4 bytes   |
+------------+----------+------------+
```

**Error Codes:**
| Code | Meaning |
|------|---------|
| 0x01 | Invalid channel |
| 0x02 | Stale share |
| 0x03 | Difficulty too low |
| 0x04 | Invalid solution |
| 0x05 | Duplicate share |

## Job Declaration Messages

### AllocateMiningJobToken (0x50)

```
+------------+----------+---------------------+---------------+
| request_id | user_len |  user_identifier    | requested_mode|
|  4 bytes   |  1 byte  |     variable        |    1 byte     |
+------------+----------+---------------------+---------------+
```

**Fields:**
- `requested_mode`: 0 = CoinbaseOnly, 1 = FullTemplate

### AllocateMiningJobToken.Success (0x51)

```
+------------+-----------+---------------------------+
| request_id | token_len |    mining_job_token       |
|  4 bytes   |  2 bytes  |       variable            |
+------------+-----------+---------------------------+
| coinbase_out_len | coinbase_output |
|     2 bytes      |    variable     |
+------------------+-----------------+
| coinbase_max_add | async_allowed | granted_mode |
|     4 bytes      |    1 byte     |    1 byte    |
+------------------+---------------+--------------+
```

### SetCustomMiningJob (0x52)

```
+------------+------------+-----------+---------------------------+
| channel_id | request_id | token_len |    mining_job_token       |
|  4 bytes   |  4 bytes   |  2 bytes  |       variable            |
+------------+------------+-----------+---------------------------+
|  version   |     prev_hash (32 bytes)     |
|  4 bytes   |                              |
+------------+------------------------------+
|        merkle_root (32 bytes)             |
+-------------------------------------------+
|     block_commitments (32 bytes)          |
+-------------------------------------------+
| coinbase_tx_len |    coinbase_tx          |
|     2 bytes     |      variable           |
+-----------------+-------------------------+
|   ntime    |   nbits    |
|  4 bytes   |  4 bytes   |
+------------+------------+
```

### SetFullTemplateJob (0x56)

```
+------------+------------+-----------+---------------------------+
| channel_id | request_id | token_len |    mining_job_token       |
|  4 bytes   |  4 bytes   |  2 bytes  |       variable            |
+------------+------------+-----------+---------------------------+
|  version   |     prev_hash (32 bytes)     |
|  4 bytes   |                              |
+------------+------------------------------+
|        merkle_root (32 bytes)             |
+-------------------------------------------+
|     block_commitments (32 bytes)          |
+-------------------------------------------+
| coinbase_tx_len |    coinbase_tx          |
|     2 bytes     |      variable           |
+-----------------+-------------------------+
|   ntime    |   nbits    |
|  4 bytes   |  4 bytes   |
+------------+------------+
| tx_count   |
|  2 bytes   |
+------------+-----------------------------+
|     tx_short_ids (32 bytes each × N)     |
+------------------------------------------+
| tx_data_count |
|    2 bytes    |
+---------------+
| For each tx_data:                        |
| tx_len (2 bytes) | tx_data (variable)    |
+------------------------------------------+
```

### GetMissingTransactions (0x59)

```
+------------+------------+----------+
| channel_id | request_id | tx_count |
|  4 bytes   |  4 bytes   | 2 bytes  |
+------------+------------+----------+
|   missing_tx_ids (32 bytes each × N)     |
+------------------------------------------+
```

### ProvideMissingTransactions (0x5A)

```
+------------+------------+----------+
| channel_id | request_id | tx_count |
|  4 bytes   |  4 bytes   | 2 bytes  |
+------------+------------+----------+
| For each transaction:                    |
| tx_len (2 bytes) | tx_data (variable)    |
+------------------------------------------+
```

## Zcash-Specific Details

### Block Header Structure

Zcash block headers are 140 bytes:

```
+-------------------------------------------+
|           version (4 bytes)               |
+-------------------------------------------+
|          prev_hash (32 bytes)             |
+-------------------------------------------+
|         merkle_root (32 bytes)            |
+-------------------------------------------+
|      block_commitments (32 bytes)         |
+-------------------------------------------+
|           ntime (4 bytes)                 |
+-------------------------------------------+
|           nbits (4 bytes)                 |
+-------------------------------------------+
|           nonce (32 bytes)                |
+-------------------------------------------+
```

### Equihash Parameters

Zcash uses Equihash with:
- **n** = 200 (bit width)
- **k** = 9 (number of collisions)
- **Solution size** = 1344 bytes

### Nonce Structure

```
Nonce (32 bytes):
+------------------+--------------------------------+
|     NONCE_1      |            NONCE_2             |
|  (pool assigns)  |      (miner iterates)          |
|    4 bytes       |           28 bytes             |
+------------------+--------------------------------+
```

### Difficulty Calculation

Convert compact nbits to target:
```rust
fn nbits_to_target(nbits: u32) -> [u8; 32] {
    let exponent = (nbits >> 24) as usize;
    let mantissa = nbits & 0x007fffff;

    let mut target = [0u8; 32];
    if exponent <= 3 {
        let shift = 8 * (3 - exponent);
        let value = mantissa >> shift;
        target[31] = (value & 0xff) as u8;
        target[30] = ((value >> 8) & 0xff) as u8;
        target[29] = ((value >> 16) & 0xff) as u8;
    } else {
        let byte_idx = 32 - exponent;
        target[byte_idx] = (mantissa & 0xff) as u8;
        target[byte_idx + 1] = ((mantissa >> 8) & 0xff) as u8;
        target[byte_idx + 2] = ((mantissa >> 16) & 0xff) as u8;
    }
    target
}
```

### Block Commitments (NU5+)

The `block_commitments` field contains `hashBlockCommitments`:
```
hashBlockCommitments = BLAKE2b-256("ZcashBlockCommit" || ...)
```

This includes:
- History tree root
- authDataRoot (for shielded transactions)
- Reserved future commitments

## Error Codes Reference

### Setup Errors

| Code | Meaning |
|------|---------|
| 0x00 | Unknown error |
| 0x01 | Unsupported feature flags |
| 0x02 | Unsupported protocol version |

### Channel Errors

| Code | Meaning |
|------|---------|
| 0x01 | Invalid user identity |
| 0x02 | Max channels exceeded |

### Share Errors

| Code | Meaning |
|------|---------|
| 0x01 | Invalid channel |
| 0x02 | Stale share |
| 0x03 | Difficulty too low |
| 0x04 | Invalid solution |
| 0x05 | Duplicate share |

### Job Declaration Errors

| Code | Meaning |
|------|---------|
| 0x01 | Invalid token |
| 0x02 | Token expired |
| 0x03 | Invalid coinbase |
| 0x04 | Coinbase constraint violation |
| 0x05 | Stale prev_hash |
| 0x06 | Invalid merkle root |
| 0x07 | Invalid version |
| 0x08 | Invalid bits |
| 0x09 | Server overloaded |
| 0x0A | Mode mismatch |
| 0x0B | Invalid transactions |
| 0x0C | Too many transactions |
| 0xFF | Other error |

## Implementation Notes

### Endianness

- All protocol integers: **little-endian**
- Block header fields: **little-endian** (except hash comparisons)
- Hash comparison: **big-endian** (for difficulty checks)

### String Encoding

All strings are length-prefixed UTF-8:
```
| length (1 byte) | UTF-8 bytes |
```

### Optional Noise

Noise encryption is negotiated out-of-band (client knows server's public key beforehand). The protocol itself doesn't negotiate encryption.

### Connection Lifecycle

```
1. TCP connect
2. [Optional] Noise handshake
3. SetupConnection
4. OpenStandardMiningChannel
5. [Receive jobs, submit shares]
6. [Optional] Job Declaration flow
7. Connection close
```
