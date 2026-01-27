//! Nonce space partitioning for Zcash mining
//!
//! Zcash uses a 32-byte nonce split as:
//! - NONCE_1: Pool-assigned prefix (assigned to each miner)
//! - NONCE_2: Miner-controlled suffix (incremented during mining)
//!
//! len(NONCE_1) + len(NONCE_2) = 32

use std::sync::atomic::{AtomicU64, Ordering};

/// A partitioned nonce range for a miner
#[derive(Debug, Clone)]
pub struct NonceRange {
    /// Pool-assigned nonce prefix
    pub nonce_1: Vec<u8>,
    /// Length of nonce_2 (miner-controlled portion)
    pub nonce_2_len: usize,
}

impl NonceRange {
    /// Construct a full 32-byte nonce from nonce_1 and nonce_2
    pub fn make_nonce(&self, nonce_2: &[u8]) -> [u8; 32] {
        assert_eq!(nonce_2.len(), self.nonce_2_len, "nonce_2 length mismatch");

        let mut nonce = [0u8; 32];
        nonce[..self.nonce_1.len()].copy_from_slice(&self.nonce_1);
        nonce[self.nonce_1.len()..].copy_from_slice(nonce_2);
        nonce
    }
}

/// Partitions the 32-byte nonce space for multiple miners
pub struct NoncePartitioner {
    nonce_1_len: usize,
    next_id: AtomicU64,
}

impl NoncePartitioner {
    /// Create a new partitioner with the given nonce_1 length
    ///
    /// # Panics
    /// Panics if nonce_1_len > 32
    pub fn new(nonce_1_len: usize) -> Self {
        assert!(nonce_1_len <= 32, "nonce_1 cannot exceed 32 bytes");
        Self {
            nonce_1_len,
            next_id: AtomicU64::new(0),
        }
    }

    /// Get a unique nonce range for a new miner connection
    pub fn allocate_range(&self) -> NonceRange {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        self.get_range(id)
    }

    /// Get a nonce range for a specific ID (useful for deterministic testing)
    pub fn get_range(&self, id: u64) -> NonceRange {
        let mut nonce_1 = vec![0u8; self.nonce_1_len];

        // Encode the ID into nonce_1 (big-endian, truncated to fit)
        let id_bytes = id.to_be_bytes();
        let copy_len = std::cmp::min(self.nonce_1_len, 8);
        let start = self.nonce_1_len.saturating_sub(8);
        nonce_1[start..start + copy_len].copy_from_slice(&id_bytes[8 - copy_len..]);

        NonceRange {
            nonce_1,
            nonce_2_len: 32 - self.nonce_1_len,
        }
    }

    /// Get the nonce_1 length
    pub fn nonce_1_len(&self) -> usize {
        self.nonce_1_len
    }

    /// Get the nonce_2 length
    pub fn nonce_2_len(&self) -> usize {
        32 - self.nonce_1_len
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_make_nonce() {
        let range = NonceRange {
            nonce_1: vec![0x01, 0x02, 0x03, 0x04],
            nonce_2_len: 28,
        };

        let nonce_2 = vec![0xaa; 28];
        let full_nonce = range.make_nonce(&nonce_2);

        assert_eq!(&full_nonce[0..4], &[0x01, 0x02, 0x03, 0x04]);
        assert_eq!(&full_nonce[4..32], &[0xaa; 28]);
    }

    #[test]
    fn test_allocate_increments() {
        let partitioner = NoncePartitioner::new(8);

        let r1 = partitioner.allocate_range();
        let r2 = partitioner.allocate_range();
        let r3 = partitioner.allocate_range();

        assert_ne!(r1.nonce_1, r2.nonce_1);
        assert_ne!(r2.nonce_1, r3.nonce_1);
    }
}
