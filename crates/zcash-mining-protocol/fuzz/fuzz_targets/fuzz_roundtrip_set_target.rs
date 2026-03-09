#![no_main]
use libfuzzer_sys::fuzz_target;
use arbitrary::Arbitrary;
use zcash_mining_protocol::codec::{encode_set_target, decode_set_target};
use zcash_mining_protocol::messages::SetTarget;

fuzz_target!(|data: &[u8]| {
    let mut u = arbitrary::Unstructured::new(data);
    let Ok(msg) = SetTarget::arbitrary(&mut u) else { return };

    let encoded = match encode_set_target(&msg) {
        Ok(e) => e,
        Err(_) => return,
    };
    let decoded = decode_set_target(&encoded)
        .expect("decode must succeed for encoder output");

    assert_eq!(msg, decoded, "roundtrip mismatch");
});
