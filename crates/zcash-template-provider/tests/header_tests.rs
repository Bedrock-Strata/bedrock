use zcash_template_provider::types::Hash256;

#[test]
fn test_header_serialization_length() {
    // Header without nonce/solution should be 140 bytes
    let header = zcash_template_provider::types::EquihashHeader {
        version: 5,
        prev_hash: Hash256([0u8; 32]),
        merkle_root: Hash256([0u8; 32]),
        hash_block_commitments: Hash256([0u8; 32]),
        time: 1700000000,
        bits: 0x1d00ffff,
        nonce: [0u8; 32],
    };

    let serialized = header.serialize();
    assert_eq!(serialized.len(), 140);
}

#[test]
fn test_header_field_positions() {
    let header = zcash_template_provider::types::EquihashHeader {
        version: 0x05000000,
        prev_hash: Hash256([0xaa; 32]),
        merkle_root: Hash256([0xbb; 32]),
        hash_block_commitments: Hash256([0xcc; 32]),
        time: 0x12345678,
        bits: 0xaabbccdd,
        nonce: [0xff; 32],
    };

    let serialized = header.serialize();

    // Version at offset 0 (little-endian)
    assert_eq!(&serialized[0..4], &[0x00, 0x00, 0x00, 0x05]);

    // prev_hash at offset 4
    assert_eq!(&serialized[4..36], &[0xaa; 32]);

    // merkle_root at offset 36
    assert_eq!(&serialized[36..68], &[0xbb; 32]);

    // hash_block_commitments at offset 68
    assert_eq!(&serialized[68..100], &[0xcc; 32]);

    // time at offset 100 (little-endian)
    assert_eq!(&serialized[100..104], &[0x78, 0x56, 0x34, 0x12]);

    // bits at offset 104 (little-endian)
    assert_eq!(&serialized[104..108], &[0xdd, 0xcc, 0xbb, 0xaa]);

    // nonce at offset 108
    assert_eq!(&serialized[108..140], &[0xff; 32]);
}
