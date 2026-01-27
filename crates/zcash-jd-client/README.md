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

## Architecture

The JD Client consists of three main components:

- **Template Builder**: Polls Zebra for new block templates and constructs coinbase transactions
- **JD Client Core**: Manages connection to pool's JD Server, handles token allocation and job declaration
- **Block Submitter**: Submits found blocks to both Zebra (for network propagation) and the pool

## License

MIT OR Apache-2.0
