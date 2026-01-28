//! Test vectors from Zcash mainnet/testnet blocks

/// A test vector containing a valid Equihash solution
#[allow(dead_code)]
pub struct TestVector {
    pub name: &'static str,
    pub header_hex: &'static str,
    pub solution_hex: &'static str,
    pub height: u64,
}

/// Zcash mainnet block test vectors
/// These are real blocks from the Zcash blockchain
#[allow(dead_code)]
pub const TEST_VECTORS: &[TestVector] = &[
    // Genesis block (simplified - real genesis has different structure)
    // For actual testing, use blocks from after NU5 activation
];

/// Helper to decode hex to bytes
pub fn hex_to_bytes(hex: &str) -> Vec<u8> {
    hex::decode(hex).expect("Invalid hex string")
}

/// Helper to decode hex to fixed array
pub fn hex_to_array<const N: usize>(hex: &str) -> [u8; N] {
    let bytes = hex_to_bytes(hex);
    assert_eq!(bytes.len(), N, "Hex string has wrong length");
    let mut arr = [0u8; N];
    arr.copy_from_slice(&bytes);
    arr
}
