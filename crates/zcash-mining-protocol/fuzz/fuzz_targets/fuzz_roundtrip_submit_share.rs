#![no_main]
use libfuzzer_sys::fuzz_target;
use arbitrary::Arbitrary;
use zcash_mining_protocol::codec::{encode_submit_share, decode_submit_share};
use zcash_mining_protocol::messages::SubmitEquihashShare;

fuzz_target!(|data: &[u8]| {
    let mut u = arbitrary::Unstructured::new(data);
    let Ok(mut share) = SubmitEquihashShare::arbitrary(&mut u) else { return };

    // Constrain nonce_2 to valid range
    if share.nonce_2.len() > 32 {
        share.nonce_2.truncate(32);
    }

    let encoded = match encode_submit_share(&share) {
        Ok(e) => e,
        Err(_) => return,
    };
    let decoded = decode_submit_share(&encoded)
        .expect("decode must succeed for encoder output");

    assert_eq!(share, decoded, "roundtrip mismatch");
});
