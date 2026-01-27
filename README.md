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
cargo run --example fetch_template -p zcash-template-provider

# Demonstrate share validation
cargo run --example validate_share -p zcash-equihash-validator
```

## Architecture

See [docs/plans/](docs/plans/) for the full implementation plans.

## License

MIT OR Apache-2.0
