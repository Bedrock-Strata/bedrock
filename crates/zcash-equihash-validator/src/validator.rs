//! Equihash solution validator
//!
//! Wraps the `equihash` crate to provide verification for Zcash's (200,9) parameters.

use crate::error::{Result, ValidationError};
use tracing::{debug, trace};

/// Zcash Equihash parameters
pub const EQUIHASH_N: u32 = 200;
pub const EQUIHASH_K: u32 = 9;

/// Expected solution size for (200,9): 512 * 21 bits / 8 = 1344 bytes
pub const SOLUTION_SIZE: usize = 1344;

/// Equihash solution validator for Zcash
#[derive(Debug, Clone)]
pub struct EquihashValidator {
    n: u32,
    k: u32,
}

impl Default for EquihashValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl EquihashValidator {
    /// Create a new validator with Zcash parameters (200,9)
    pub fn new() -> Self {
        Self {
            n: EQUIHASH_N,
            k: EQUIHASH_K,
        }
    }

    /// Get the n parameter
    pub fn n(&self) -> u32 {
        self.n
    }

    /// Get the k parameter
    pub fn k(&self) -> u32 {
        self.k
    }

    /// Verify an Equihash solution
    ///
    /// # Arguments
    /// * `header` - The 140-byte block header (including nonce, excluding solution)
    /// * `solution` - The 1344-byte Equihash solution
    ///
    /// # Returns
    /// * `Ok(())` if the solution is valid
    /// * `Err(ValidationError)` if verification fails
    pub fn verify_solution(&self, header: &[u8], solution: &[u8]) -> Result<()> {
        // Validate input lengths
        if header.len() != 140 {
            return Err(ValidationError::InvalidHeaderLength(header.len()));
        }
        if solution.len() != SOLUTION_SIZE {
            return Err(ValidationError::InvalidSolutionLength(solution.len()));
        }

        trace!(
            "Verifying Equihash solution: header_len={}, solution_len={}",
            header.len(),
            solution.len()
        );

        // The equihash crate expects:
        // - input: the 108-byte header prefix (before nonce)
        // - nonce: the 32-byte nonce
        // - solution: the 1344-byte solution
        let input = &header[..108];
        let nonce = &header[108..140];

        equihash::is_valid_solution(self.n, self.k, input, nonce, solution)
            .map_err(|e| ValidationError::InvalidSolution(format!("{:?}", e)))?;

        debug!("Equihash solution verified successfully");
        Ok(())
    }

    /// Verify a solution and check if it meets the target difficulty
    ///
    /// # Arguments
    /// * `header` - The 140-byte block header
    /// * `solution` - The 1344-byte Equihash solution
    /// * `target` - The 256-bit target (solution hash must be <= target)
    ///
    /// # Returns
    /// * `Ok(hash)` if valid and meets target, returns the solution hash
    /// * `Err(ValidationError)` if verification fails or target not met
    pub fn verify_share(
        &self,
        header: &[u8],
        solution: &[u8],
        target: &[u8; 32],
    ) -> Result<[u8; 32]> {
        // First verify the solution is valid
        self.verify_solution(header, solution)?;

        // Compute the hash of header + solution
        let hash = self.compute_solution_hash(header, solution)?;

        // Check if hash meets target (hash <= target, little-endian comparison)
        if !self.meets_target(&hash, target) {
            return Err(ValidationError::TargetNotMet);
        }

        Ok(hash)
    }

    /// Compute Zcash's proof-of-work hash: the double-SHA256 of the full
    /// 1487-byte serialized header, in internal byte order.
    ///
    /// This is the value the target must be compared against. It is the block
    /// id Zebra reports and every explorer displays (as its byte reversal).
    ///
    /// It is deliberately NOT the BLAKE2b-256 digest personalised
    /// `"ZcashBlockHash"`. Despite that personalization string, that digest is
    /// the relay's INTERNAL object id, used for raw-segment dedup and
    /// reassembly (see `sovright_relay::hash`, which documents the same
    /// distinction). The two functions produce unrelated values over the same
    /// bytes, so comparing that digest to an nBits-derived target accepts a
    /// genuine block only by coincidence. Mainnet block 3470793, the fixture in
    /// `tests/consensus_pow_hash.rs`, is one it rejects.
    fn compute_solution_hash(&self, header: &[u8], solution: &[u8]) -> Result<[u8; 32]> {
        use sha2::{Digest, Sha256};

        // The serialized header is header(140) || compactSize(solution len) ||
        // solution(1344), which is the same 1487 bytes the P2P network and
        // `submitblock` carry.
        let mut data = Vec::with_capacity(header.len() + 3 + solution.len());
        data.extend_from_slice(header);
        zcash_pool_common::write_compact_size(solution.len() as u64, &mut data);
        data.extend_from_slice(solution);

        let first = Sha256::digest(&data);
        let second = Sha256::digest(first);

        let mut result = [0u8; 32];
        result.copy_from_slice(&second);
        Ok(result)
    }

    /// Check if a hash meets the target (hash <= target, little-endian)
    fn meets_target(&self, hash: &[u8; 32], target: &[u8; 32]) -> bool {
        // Compare as little-endian 256-bit integers
        // Start from the most significant byte (index 31) and work down
        for i in (0..32).rev() {
            if hash[i] < target[i] {
                return true;
            }
            if hash[i] > target[i] {
                return false;
            }
        }
        true // Equal is also valid
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_meets_target() {
        let validator = EquihashValidator::new();

        // Hash is less than target
        let hash = [0x00; 32];
        let target = [0xff; 32];
        assert!(validator.meets_target(&hash, &target));

        // Hash equals target
        let same = [0x42; 32];
        assert!(validator.meets_target(&same, &same));

        // Hash is greater than target
        let high_hash = [0xff; 32];
        let low_target = [0x00; 32];
        assert!(!validator.meets_target(&high_hash, &low_target));
    }

    #[test]
    fn test_parameter_values() {
        let validator = EquihashValidator::new();
        assert_eq!(validator.n(), 200);
        assert_eq!(validator.k(), 9);
    }
}
