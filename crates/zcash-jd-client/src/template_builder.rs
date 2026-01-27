//! Template builder for JD Client
//!
//! Constructs custom block templates using the Template Provider
//! and adds the pool's required coinbase output.

use crate::error::{JdClientError, Result};
use zcash_template_provider::types::BlockTemplate;

/// Template builder that adds pool coinbase requirements
pub struct TemplateBuilder {
    /// Pool's required coinbase output script
    pool_coinbase_output: Vec<u8>,
    /// Maximum additional coinbase size allowed
    max_additional_size: u32,
    /// Optional miner payout address
    miner_payout_address: Option<String>,
}

impl TemplateBuilder {
    /// Create a new template builder
    pub fn new(
        pool_coinbase_output: Vec<u8>,
        max_additional_size: u32,
        miner_payout_address: Option<String>,
    ) -> Self {
        Self {
            pool_coinbase_output,
            max_additional_size,
            miner_payout_address,
        }
    }

    /// Build a custom coinbase transaction from a template
    ///
    /// For Phase 4 MVP, we use the template's coinbase directly.
    /// In production, we'd construct a proper coinbase with:
    /// 1. Pool's required output (for payout)
    /// 2. Miner's optional output
    /// 3. Funding stream outputs (handled by Zebra)
    pub fn build_coinbase(&self, template: &BlockTemplate) -> Result<Vec<u8>> {
        // Validate size
        let additional_size = self.pool_coinbase_output.len() as u32;
        if additional_size > self.max_additional_size {
            return Err(JdClientError::Protocol(format!(
                "Pool output too large: {} > {}",
                additional_size, self.max_additional_size
            )));
        }

        // For MVP, return the template's coinbase
        // TODO: Proper coinbase construction with pool output
        Ok(template.coinbase.clone())
    }

    /// Calculate merkle root for a modified coinbase
    ///
    /// For MVP, we use the template's merkle root directly
    /// since we're not modifying the coinbase.
    pub fn calculate_merkle_root(&self, template: &BlockTemplate, _coinbase: &[u8]) -> [u8; 32] {
        template.header.merkle_root.0
    }

    /// Get the block commitments hash
    pub fn block_commitments(&self, template: &BlockTemplate) -> [u8; 32] {
        template.header.hash_block_commitments.0
    }

    /// Update pool coinbase output (when receiving new token)
    pub fn set_pool_output(&mut self, output: Vec<u8>, max_size: u32) {
        self.pool_coinbase_output = output;
        self.max_additional_size = max_size;
    }

    /// Get the current pool coinbase output
    pub fn pool_coinbase_output(&self) -> &[u8] {
        &self.pool_coinbase_output
    }

    /// Get the maximum additional size allowed
    pub fn max_additional_size(&self) -> u32 {
        self.max_additional_size
    }

    /// Get the miner payout address if set
    pub fn miner_payout_address(&self) -> Option<&str> {
        self.miner_payout_address.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_builder_creation() {
        let builder = TemplateBuilder::new(
            vec![0x76, 0xa9], // P2PKH prefix
            256,
            None,
        );

        assert_eq!(builder.max_additional_size(), 256);
        assert_eq!(builder.pool_coinbase_output(), &[0x76, 0xa9]);
        assert!(builder.miner_payout_address().is_none());
    }

    #[test]
    fn test_template_builder_with_miner_address() {
        let builder = TemplateBuilder::new(
            vec![0x76, 0xa9],
            256,
            Some("t1exampleaddress".to_string()),
        );

        assert_eq!(builder.miner_payout_address(), Some("t1exampleaddress"));
    }

    #[test]
    fn test_set_pool_output() {
        let mut builder = TemplateBuilder::new(vec![], 0, None);

        builder.set_pool_output(vec![0x01, 0x02, 0x03], 512);

        assert_eq!(builder.pool_coinbase_output(), &[0x01, 0x02, 0x03]);
        assert_eq!(builder.max_additional_size(), 512);
    }
}
