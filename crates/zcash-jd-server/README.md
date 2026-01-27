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
use zcash_pool_common::PayoutTracker;
use std::sync::Arc;

let config = JdServerConfig::default();
let payout_tracker = Arc::new(PayoutTracker::default());
let jd_server = JdServer::new(config, payout_tracker);
```

## Protocol Messages

| Message | Direction | Purpose |
|---------|-----------|---------|
| AllocateMiningJobToken | Client -> Server | Request token |
| AllocateMiningJobToken.Success | Server -> Client | Return token |
| SetCustomMiningJob | Client -> Server | Declare job |
| SetCustomMiningJob.Success/Error | Server -> Client | Acknowledge/reject |
| PushSolution | Client -> Server | Submit block |

## Protocol Flow (Coinbase-Only Mode)

1. Client requests a token via `AllocateMiningJobToken`
2. Server responds with `AllocateMiningJobTokenSuccess` containing the token
3. Client declares a job via `SetCustomMiningJob` with the token
4. Server validates and responds with `SetCustomMiningJobSuccess` or error
5. Client can submit solutions via `PushSolution`

## Full-Template Mode

In addition to Coinbase-Only mode, the JD Server supports Full-Template mode where miners can select which transactions to include in their blocks.

### Enabling Full-Template Mode

```rust
let config = JdServerConfig {
    full_template_enabled: true,
    full_template_validation: ValidationLevel::Standard,
    min_pool_payout: 0,
    ..Default::default()
};
```

### Validation Levels

| Level | Description |
|-------|-------------|
| `Minimal` | Only verify pool payout output exists |
| `Standard` | Verify pool payout + request missing transactions |
| `Strict` | Full validation of all transactions |

### Protocol Flow (Full-Template)

1. Client requests token with `JobDeclarationMode::FullTemplate`
2. Server grants FullTemplate mode if enabled (falls back to CoinbaseOnly otherwise)
3. Client sends `SetFullTemplateJob` with transactions
4. If server needs missing transactions, it sends `GetMissingTransactions`
5. Client responds with `ProvideMissingTransactions`
6. Server validates and responds with success or error

## Configuration

| Option | Default | Description |
|--------|---------|-------------|
| `token_lifetime` | 5 min | Token validity duration |
| `coinbase_output_max_additional_size` | 256 | Max miner coinbase addition (bytes) |
| `async_mining_allowed` | true | Allow mining before ack |
| `pool_payout_script` | empty | Pool's payout output script |
| `max_tokens_per_client` | 10 | Max active tokens per client |
| `full_template_enabled` | false | Enable Full-Template mode |
| `full_template_validation` | Standard | Validation level for Full-Template |
| `min_pool_payout` | 0 | Minimum pool payout (zatoshis) |

## License

MIT OR Apache-2.0
