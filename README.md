# Stratum V2 for Zcash

Implementation of Stratum V2 mining protocol for Zcash with support for decentralized block template construction.

## Project Status

- Phase 1: Zcash Template Provider - **Complete**
- Phase 2: Equihash Mining Protocol - **Complete**
- Phase 3: Pool Server MVP - **Complete**
- Phase 4: Job Declaration Protocol - **Complete**
- Phase 5: Security & Observability - **Complete**
- Phase 6: Full-Template Mode - **Complete**

## Crates

| Crate | Description |
|-------|-------------|
| `zcash-template-provider` | Template Provider interfacing with Zebra |
| `zcash-mining-protocol` | SV2 message types for Equihash mining |
| `zcash-equihash-validator` | Share validation and vardiff |
| `zcash-pool-server` | Pool server accepting miner connections |
| `zcash-pool-common` | Shared types for pool components (PPS tracking) |
| `zcash-jd-server` | Job Declaration Server for custom mining jobs |
| `zcash-jd-client` | Job Declaration Client for decentralized templates |
| `zcash-stratum-noise` | Noise Protocol encryption |
| `zcash-stratum-observability` | Metrics, logging, tracing |

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
cargo run --example fetch_template -p zcash-template-provider

# Demonstrate share validation
cargo run --example validate_share -p zcash-equihash-validator

# Run the pool server (requires Zebra node)
cargo run --example run_pool -p zcash-pool-server
```

## Architecture

See [docs/plans/](docs/plans/) for the full implementation plans.

## License

MIT OR Apache-2.0
