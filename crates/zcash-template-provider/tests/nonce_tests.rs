use zcash_template_provider::nonce::NoncePartitioner;

#[test]
fn test_nonce_partition_basic() {
    let partitioner = NoncePartitioner::new(8).unwrap(); // 8-byte nonce_1
    let range = partitioner.get_range(0);

    assert_eq!(range.nonce_1.len(), 8);
    assert_eq!(range.nonce_2_len, 24); // 32 - 8 = 24
}

#[test]
fn test_nonce_partitions_unique() {
    let partitioner = NoncePartitioner::new(8).unwrap();
    let range1 = partitioner.get_range(0);
    let range2 = partitioner.get_range(1);

    assert_ne!(range1.nonce_1, range2.nonce_1);
}

#[test]
fn test_nonce_1_length_validation() {
    // nonce_1 must be <= 32 bytes - should return None instead of panic
    assert!(NoncePartitioner::new(33).is_none());

    // Valid lengths should succeed
    assert!(NoncePartitioner::new(32).is_some());
    assert!(NoncePartitioner::new(0).is_some());
}
