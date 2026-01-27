//! Zcash JD Client binary

use clap::Parser;
use zcash_jd_client::JdClientConfig;

#[derive(Parser, Debug)]
#[command(name = "zcash-jd-client")]
#[command(about = "Zcash Job Declaration Client for Stratum V2")]
struct Args {
    #[arg(long, default_value = "http://127.0.0.1:8232")]
    zebra_url: String,

    #[arg(long, default_value = "127.0.0.1:3334")]
    pool_jd_addr: String,

    #[arg(long, default_value = "zcash-jd-client")]
    user_id: String,

    #[arg(long, default_value = "1000")]
    poll_interval: u64,

    #[arg(long)]
    payout_address: Option<String>,
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
    };

    println!("=== Zcash JD Client ===");
    println!("Zebra RPC: {}", config.zebra_url);
    println!("Pool JD Server: {}", config.pool_jd_addr);
    println!("User ID: {}", config.user_identifier);
    println!();

    // TODO: Start client in Tasks 7-9
    println!("JD Client stub - implementation coming in Tasks 7-9");

    Ok(())
}
