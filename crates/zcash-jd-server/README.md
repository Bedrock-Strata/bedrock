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

## Protocol Flow

1. Client requests a token via `AllocateMiningJobToken`
2. Server responds with `AllocateMiningJobTokenSuccess` containing the token
3. Client declares a job via `SetCustomMiningJob` with the token
4. Server validates and responds with `SetCustomMiningJobSuccess` or error
5. Client can submit solutions via `PushSolution`

## Configuration

| Option | Default | Description |
|--------|---------|-------------|
| `token_lifetime` | 5 min | Token validity duration |
| `coinbase_output_max_additional_size` | 256 | Max miner coinbase addition (bytes) |
| `async_mining_allowed` | true | Allow mining before ack |
| `pool_payout_script` | empty | Pool's payout output script |
| `max_tokens_per_client` | 10 | Max active tokens per client |

## License

MIT OR Apache-2.0
