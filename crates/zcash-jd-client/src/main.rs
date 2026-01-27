//! Zcash JD Client binary

use clap::Parser;
use tracing::info;
use zcash_jd_client::{JdClient, JdClientConfig};

#[derive(Parser, Debug)]
#[command(name = "zcash-jd-client")]
#[command(about = "Zcash Job Declaration Client for Stratum V2")]
struct Args {
    /// Zebra RPC URL
    #[arg(long, default_value = "http://127.0.0.1:8232")]
    zebra_url: String,

    /// Pool JD Server address
    #[arg(long, default_value = "127.0.0.1:3334")]
    pool_jd_addr: String,

    /// User identifier for job allocation
    #[arg(long, default_value = "zcash-jd-client")]
    user_id: String,

    /// Template polling interval in milliseconds
    #[arg(long, default_value = "1000")]
    poll_interval: u64,

    /// Optional miner payout address
    #[arg(long)]
    payout_address: Option<String>,

    /// Enable Noise encryption
    #[arg(long)]
    noise: bool,

    /// Pool's Noise public key (hex-encoded)
    #[arg(long)]
    pool_public_key: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    let config = JdClientConfig {
        zebra_url: args.zebra_url,
        pool_jd_addr: args.pool_jd_addr.parse()?,
        user_identifier: args.user_id,
        template_poll_ms: args.poll_interval,
        miner_payout_address: args.payout_address,
        noise_enabled: args.noise,
        pool_public_key: args.pool_public_key,
    };

    info!("=== Zcash JD Client ===");
    info!("Zebra RPC: {}", config.zebra_url);
    info!("Pool JD Server: {}", config.pool_jd_addr);
    info!("User ID: {}", config.user_identifier);
    info!("Poll interval: {}ms", config.template_poll_ms);
    info!("Noise encryption: {}", if config.noise_enabled { "enabled" } else { "disabled" });

    let client = JdClient::new(config)?;
    client.run().await?;

    Ok(())
}
