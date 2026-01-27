//! Example: Validate an Equihash share
//!
//! This demonstrates the full flow from job creation to share validation.
//!
//! Usage: cargo run --example validate_share -p zcash-equihash-validator

use zcash_equihash_validator::{EquihashValidator, VardiffConfig, VardiffController};
use zcash_mining_protocol::messages::{NewEquihashJob, SubmitEquihashShare};

fn main() {
    tracing_subscriber::fmt::init();

    println!("=== Zcash Equihash Share Validation Demo ===\n");

    // Create validator
    let validator = EquihashValidator::new();
    println!(
        "Validator initialized with Equihash({}, {})",
        validator.n(),
        validator.k()
    );

    // Create vardiff controller
    let config = VardiffConfig::default();
    let mut vardiff = VardiffController::new(config);
    println!(
        "Vardiff initialized: target {:.1} shares/min",
        vardiff.stats().target_rate
    );
    println!("Current difficulty: {:.2}", vardiff.current_difficulty());

    // Create a mining job
    let job = NewEquihashJob {
        channel_id: 1,
        job_id: 42,
        future_job: false,
        version: 5,
        prev_hash: [0xab; 32],
        merkle_root: [0xcd; 32],
        block_commitments: [0xef; 32],
        nonce_1: vec![0x00, 0x00, 0x00, 0x01], // Pool prefix
        nonce_2_len: 28,
        time: 1700000000,
        bits: 0x1d00ffff,
        target: vardiff.current_target().to_le_bytes(),
        clean_jobs: true,
    };

    println!("\n=== Mining Job ===");
    println!("Job ID: {}", job.job_id);
    println!("Height implied by prev_hash");
    println!("Nonce_1 length: {} bytes", job.nonce_1.len());
    println!("Nonce_2 length: {} bytes", job.nonce_2_len);

    // Simulate a share submission (with dummy solution)
    let share = SubmitEquihashShare {
        channel_id: 1,
        sequence_number: 1,
        job_id: 42,
        nonce_2: vec![0xff; 28],
        time: 1700000001,
        solution: [0x00; 1344], // Invalid solution for demo
    };

    println!("\n=== Share Submission ===");
    println!("Sequence: {}", share.sequence_number);
    println!("Solution size: {} bytes", share.solution.len());

    // Build full nonce and header
    let nonce = job
        .build_nonce(&share.nonce_2)
        .expect("Invalid nonce_2 length");
    let header = job.build_header(&nonce);

    println!("\n=== Validation ===");
    println!("Header size: {} bytes", header.len());
    println!("Nonce: {}", hex::encode(&nonce[..8])); // First 8 bytes

    // Validate the solution
    match validator.verify_solution(&header, &share.solution) {
        Ok(()) => {
            println!("Solution: VALID");

            // Check target
            let target = vardiff.current_target();
            match validator.verify_share(&header, &share.solution, &target.to_le_bytes()) {
                Ok(hash) => {
                    println!("Share: ACCEPTED");
                    println!("Hash: {}", hex::encode(&hash[..8]));
                    vardiff.record_share();
                }
                Err(e) => {
                    println!("Share: REJECTED ({})", e);
                }
            }
        }
        Err(e) => {
            println!("Solution: INVALID ({})", e);
            println!("Share: REJECTED");
        }
    }

    // Show vardiff stats
    let stats = vardiff.stats();
    println!("\n=== Vardiff Stats ===");
    println!("Difficulty: {:.2}", stats.current_difficulty);
    println!("Shares in window: {}", stats.shares_in_window);
    println!("Current rate: {:.2}/min", stats.current_rate);
    println!("Target rate: {:.2}/min", stats.target_rate);

    println!("\n=== Demo Complete ===");
    println!("Note: This demo uses an invalid solution for illustration.");
    println!("Real shares require valid Equihash solutions from mining hardware.");
}
