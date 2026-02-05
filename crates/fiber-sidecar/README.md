# Fiber Sidecar

A standalone sidecar binary that enables Stratum V1 mining pools to use fiber-zcash for low-latency block relay.

## Overview

The fiber sidecar:
- Polls Zebra for new block templates
- Builds compact blocks when templates change
- Announces compact blocks to the fiber relay network

This allows any V1 pool (NOMP, etc.) to benefit from compact block relay without modification.

## Usage

### Command Line

```bash
fiber-sidecar \
    --zebra-url http://127.0.0.1:8232 \
    --relay-peer fiber-relay.example.com:8333 \
    --auth-key 0123456789abcdef... \
    --poll-interval-ms 100
```

### Configuration File

```bash
fiber-sidecar --config config.toml
```

See `config.example.toml` for all options.

## Architecture

```
STRATUM V1 POOL (unmodified)
        │
        ▼ getblocktemplate/submitblock
    ZEBRA NODE ◄──────────────────────┐
        │                             │
        │ poll templates              │ (future: submitblock)
        ▼                             │
   FIBER SIDECAR ─────────────────────┘
        │
        ▼ UDP/FEC
   FIBER RELAY NETWORK
```

## Requirements

- Zebra node with JSON-RPC enabled
- Network connectivity to fiber relay nodes

## Building

```bash
cargo build --release -p fiber-sidecar
```

Binary will be at `target/release/fiber-sidecar`.
