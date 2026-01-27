# zcash-template-provider

Zcash Template Provider for Stratum V2 mining.

## Overview

This crate provides a Template Provider that:
- Interfaces with Zebra nodes via JSON-RPC
- Produces SV2-compatible block templates
- Assembles 140-byte Equihash input headers
- Handles 32-byte nonce space partitioning
- Broadcasts templates to subscribers on new blocks

## Usage

```rust
use zcash_template_provider::{TemplateProvider, TemplateProviderConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = TemplateProviderConfig {
        zebra_url: "http://127.0.0.1:8232".to_string(),
        poll_interval_ms: 1000,
    };

    let provider = TemplateProvider::new(config)?;

    // Fetch a single template
    let template = provider.fetch_template().await?;
    println!("Got template at height {}", template.height);

    // Or subscribe to template updates
    let mut rx = provider.subscribe();
    tokio::spawn(async move { provider.run().await });

    while let Ok(template) = rx.recv().await {
        println!("New template: height={}", template.height);
    }

    Ok(())
}
```

## Requirements

- Rust 1.75+
- Running Zebra node with RPC enabled (port 8232)

## Testing with Zebra

1. Start Zebra with mining RPC enabled
2. Run: `cargo run --example fetch_template`
