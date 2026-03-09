#![no_main]
use libfuzzer_sys::fuzz_target;
use arbitrary::Arbitrary;
use zcash_mining_protocol::codec::{encode_new_equihash_job, decode_new_equihash_job};
use zcash_mining_protocol::messages::NewEquihashJob;

fuzz_target!(|data: &[u8]| {
    let mut u = arbitrary::Unstructured::new(data);
    let Ok(mut job) = NewEquihashJob::arbitrary(&mut u) else { return };

    // Constrain: nonce_1.len() + nonce_2_len must == 32
    if job.nonce_1.len() > 32 {
        job.nonce_1.truncate(32);
    }
    job.nonce_2_len = (32 - job.nonce_1.len()) as u8;

    let encoded = match encode_new_equihash_job(&job) {
        Ok(e) => e,
        Err(_) => return,
    };
    let decoded = decode_new_equihash_job(&encoded)
        .expect("decode must succeed for encoder output");

    assert_eq!(job, decoded, "roundtrip mismatch");
});
