//! Run a basic pool server
//!
//! Usage: cargo run --example run_pool -p zcash-pool-server

use zcash_pool_server::{PoolConfig, PoolServer};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let config = PoolConfig {
        listen_addr: "127.0.0.1:3333".parse()?,
        zebra_url: "http://127.0.0.1:8232".to_string(),
        ..Default::default()
    };

    println!("=== Zcash Pool Server ===");
    println!("Listening on: {}", config.listen_addr);
    println!("Zebra RPC: {}", config.zebra_url);
    println!("Nonce_1 length: {} bytes", config.nonce_1_len);
    println!("Initial difficulty: {}", config.initial_difficulty);
    println!();

    let server = PoolServer::new(config)?;
    server.run().await?;

    Ok(())
}
