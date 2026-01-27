//! Block commitments hash calculation for NU5+
//!
//! hashBlockCommitments = BLAKE2b-256("ZcashBlockCommit" || historyRoot || authDataRoot || 0x00...00)
//!
//! The personalization is "ZcashBlockCommit" (16 bytes)
//! historyRoot is the chain history tree root (32 bytes)
//! authDataRoot is the auth data merkle root (32 bytes)
//! The terminator is 32 zero bytes

use crate::types::Hash256;
use blake2b_simd::Params;

const BLOCK_COMMIT_PERSONALIZATION: &[u8; 16] = b"ZcashBlockCommit";

/// Calculate hashBlockCommitments for NU5+ blocks
///
/// # Arguments
/// * `history_root` - The chain history tree root
/// * `auth_data_root` - The auth data merkle root
///
/// # Returns
/// The 32-byte block commitments hash
pub fn calculate_block_commitments_hash(
    history_root: &Hash256,
    auth_data_root: &Hash256,
) -> Hash256 {
    let mut params = Params::new();
    params.hash_length(32);
    params.personal(BLOCK_COMMIT_PERSONALIZATION);

    let mut state = params.to_state();
    state.update(history_root.as_bytes());
    state.update(auth_data_root.as_bytes());
    state.update(&[0u8; 32]); // terminator

    let hash = state.finalize();
    let mut result = [0u8; 32];
    result.copy_from_slice(hash.as_bytes());

    Hash256(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_commitments_deterministic() {
        let h1 = Hash256([1u8; 32]);
        let a1 = Hash256([2u8; 32]);

        let result1 = calculate_block_commitments_hash(&h1, &a1);
        let result2 = calculate_block_commitments_hash(&h1, &a1);

        assert_eq!(result1, result2);
    }

    #[test]
    fn test_block_commitments_changes_with_input() {
        let h1 = Hash256([1u8; 32]);
        let h2 = Hash256([2u8; 32]);
        let auth = Hash256([0u8; 32]);

        let result1 = calculate_block_commitments_hash(&h1, &auth);
        let result2 = calculate_block_commitments_hash(&h2, &auth);

        assert_ne!(result1, result2);
    }
}
