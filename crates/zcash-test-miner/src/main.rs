mod protocol;
mod transport;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "zcash-test-miner")]
#[command(about = "CPU Equihash test miner for Bedrock pool testing")]
struct Args {
    /// Pool SV2 endpoint
    #[arg(long, default_value = "127.0.0.1:3333")]
    pool_addr: String,

    /// Number of simulated worker connections
    #[arg(long, default_value = "1")]
    workers: u32,

    /// Worker name prefix (names: {prefix}-1, {prefix}-2, ...)
    #[arg(long, default_value = "worker")]
    worker_prefix: String,

    /// CPU threads per worker for Equihash solving
    #[arg(long, default_value = "1")]
    solver_threads: u32,

    /// Pool's Noise public key (hex). If omitted, connects without encryption.
    #[arg(long)]
    pool_public_key: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();
    tracing::info!(
        pool_addr = %args.pool_addr,
        workers = args.workers,
        prefix = %args.worker_prefix,
        solver_threads = args.solver_threads,
        noise = args.pool_public_key.is_some(),
        "Starting zcash-test-miner"
    );

    Ok(())
}
