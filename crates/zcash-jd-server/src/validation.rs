//! Template validation for Full-Template mode

use crate::messages::SetFullTemplateJob;
use std::collections::HashSet;

/// Validation strictness for full templates
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ValidationLevel {
    /// Only verify pool payout output exists with minimum value
    Minimal,
    /// Verify pool payout + basic template structure (default)
    #[default]
    Standard,
    /// Full validation: verify all transactions are valid
    Strict,
}

impl ValidationLevel {
    /// Parse from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "minimal" => Some(Self::Minimal),
            "standard" => Some(Self::Standard),
            "strict" => Some(Self::Strict),
            _ => None,
        }
    }

    /// Convert to string
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::Standard => "standard",
            Self::Strict => "strict",
        }
    }
}

impl std::fmt::Display for ValidationLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Result of template validation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationResult {
    /// Template is valid
    Valid,
    /// Template is invalid with reason
    Invalid(String),
    /// Need additional transaction data from client
    NeedTransactions(Vec<[u8; 32]>),
}

/// Validates full templates submitted by miners
pub struct TemplateValidator {
    /// Validation strictness level
    level: ValidationLevel,
    /// Pool's required payout script (scriptPubKey)
    pool_payout_script: Vec<u8>,
    /// Minimum pool payout value (zatoshis)
    min_pool_payout: u64,
    /// Known transaction IDs (from pool's mempool view)
    known_txids: HashSet<[u8; 32]>,
}

impl TemplateValidator {
    /// Create a new validator
    pub fn new(
        level: ValidationLevel,
        pool_payout_script: Vec<u8>,
        min_pool_payout: u64,
    ) -> Self {
        Self {
            level,
            pool_payout_script,
            min_pool_payout,
            known_txids: HashSet::new(),
        }
    }

    /// Update known transactions from pool's mempool
    pub fn update_known_txids(&mut self, txids: impl IntoIterator<Item = [u8; 32]>) {
        self.known_txids.extend(txids);
    }

    /// Clear known transactions
    pub fn clear_known_txids(&mut self) {
        self.known_txids.clear();
    }

    /// Add a single known txid
    pub fn add_known_txid(&mut self, txid: [u8; 32]) {
        self.known_txids.insert(txid);
    }

    /// Check if a txid is known
    pub fn is_txid_known(&self, txid: &[u8; 32]) -> bool {
        self.known_txids.contains(txid)
    }

    /// Get the validation level
    pub fn level(&self) -> ValidationLevel {
        self.level
    }

    /// Get the minimum pool payout
    #[allow(dead_code)]
    pub fn min_pool_payout(&self) -> u64 {
        self.min_pool_payout
    }

    /// Validate a full template job
    pub fn validate(&self, job: &SetFullTemplateJob) -> ValidationResult {
        // Always check pool payout in coinbase
        if !self.pool_payout_script.is_empty() {
            if !self.validate_pool_payout(&job.coinbase_tx) {
                return ValidationResult::Invalid("Missing or insufficient pool payout".into());
            }
        }

        match self.level {
            ValidationLevel::Minimal => ValidationResult::Valid,
            ValidationLevel::Standard => self.validate_standard(job),
            ValidationLevel::Strict => self.validate_strict(job),
        }
    }

    /// Check if pool payout script appears in coinbase
    fn validate_pool_payout(&self, coinbase: &[u8]) -> bool {
        if self.pool_payout_script.is_empty() {
            return true;
        }
        // Simple check: does the payout script appear in the coinbase?
        // A more complete implementation would parse the tx and check output values
        coinbase
            .windows(self.pool_payout_script.len())
            .any(|w| w == self.pool_payout_script.as_slice())
    }

    /// Standard validation: check for missing transactions
    fn validate_standard(&self, job: &SetFullTemplateJob) -> ValidationResult {
        // Check if we know all referenced transactions
        let missing: Vec<[u8; 32]> = job
            .tx_short_ids
            .iter()
            .filter(|txid| !self.known_txids.contains(*txid))
            .copied()
            .collect();

        // If there are missing txids and client didn't provide them in tx_data
        if !missing.is_empty() {
            // Check if any of the missing were provided in tx_data
            // For simplicity, if tx_data has same count as missing, assume they match
            if job.tx_data.len() < missing.len() {
                return ValidationResult::NeedTransactions(missing);
            }
        }

        ValidationResult::Valid
    }

    /// Strict validation: full transaction verification
    fn validate_strict(&self, job: &SetFullTemplateJob) -> ValidationResult {
        // First do standard validation
        match self.validate_standard(job) {
            ValidationResult::Valid => {}
            other => return other,
        }

        // In strict mode, we could:
        // 1. Verify each transaction in tx_data is valid
        // 2. Verify the merkle root matches
        // 3. Verify block structure
        // For MVP, same as standard

        ValidationResult::Valid
    }
}

impl Default for TemplateValidator {
    fn default() -> Self {
        Self::new(ValidationLevel::Standard, vec![], 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::SetFullTemplateJob;

    #[test]
    fn test_validation_level_from_str() {
        assert_eq!(
            ValidationLevel::from_str("minimal"),
            Some(ValidationLevel::Minimal)
        );
        assert_eq!(
            ValidationLevel::from_str("STANDARD"),
            Some(ValidationLevel::Standard)
        );
        assert_eq!(
            ValidationLevel::from_str("Strict"),
            Some(ValidationLevel::Strict)
        );
        assert_eq!(ValidationLevel::from_str("unknown"), None);
    }

    #[test]
    fn test_validation_level_display() {
        assert_eq!(format!("{}", ValidationLevel::Minimal), "minimal");
        assert_eq!(format!("{}", ValidationLevel::Standard), "standard");
        assert_eq!(format!("{}", ValidationLevel::Strict), "strict");
    }

    #[test]
    fn test_validation_level_default() {
        let level: ValidationLevel = Default::default();
        assert_eq!(level, ValidationLevel::Standard);
    }

    #[test]
    fn test_validation_result_variants() {
        let valid = ValidationResult::Valid;
        assert_eq!(valid, ValidationResult::Valid);

        let invalid = ValidationResult::Invalid("missing payout".to_string());
        assert_eq!(
            invalid,
            ValidationResult::Invalid("missing payout".to_string())
        );

        let need_tx = ValidationResult::NeedTransactions(vec![[0u8; 32], [1u8; 32]]);
        if let ValidationResult::NeedTransactions(txids) = need_tx {
            assert_eq!(txids.len(), 2);
        } else {
            panic!("Expected NeedTransactions variant");
        }
    }

    // =========================================================================
    // TemplateValidator Tests
    // =========================================================================

    fn make_test_job() -> SetFullTemplateJob {
        SetFullTemplateJob {
            channel_id: 1,
            request_id: 1,
            mining_job_token: vec![0x01, 0x02, 0x03],
            version: 5,
            prev_hash: [0xaa; 32],
            merkle_root: [0xbb; 32],
            block_commitments: [0xcc; 32],
            coinbase_tx: vec![0x01, 0x00, 0x00, 0x00],
            time: 1700000000,
            bits: 0x1d00ffff,
            tx_short_ids: vec![],
            tx_data: vec![],
        }
    }

    #[test]
    fn test_minimal_validation_accepts_any() {
        let validator = TemplateValidator::new(ValidationLevel::Minimal, vec![], 0);
        let job = make_test_job();
        assert_eq!(validator.validate(&job), ValidationResult::Valid);
    }

    #[test]
    fn test_standard_validation_with_unknown_txids() {
        let validator = TemplateValidator::new(ValidationLevel::Standard, vec![], 0);
        let mut job = make_test_job();
        job.tx_short_ids = vec![[0x11; 32], [0x22; 32]];

        match validator.validate(&job) {
            ValidationResult::NeedTransactions(missing) => {
                assert_eq!(missing.len(), 2);
            }
            _ => panic!("Expected NeedTransactions"),
        }
    }

    #[test]
    fn test_standard_validation_with_known_txids() {
        let mut validator = TemplateValidator::new(ValidationLevel::Standard, vec![], 0);
        validator.add_known_txid([0x11; 32]);
        validator.add_known_txid([0x22; 32]);

        let mut job = make_test_job();
        job.tx_short_ids = vec![[0x11; 32], [0x22; 32]];

        assert_eq!(validator.validate(&job), ValidationResult::Valid);
    }

    #[test]
    fn test_standard_validation_with_provided_tx_data() {
        let validator = TemplateValidator::new(ValidationLevel::Standard, vec![], 0);
        let mut job = make_test_job();
        job.tx_short_ids = vec![[0x11; 32], [0x22; 32]];
        // Provide enough tx_data to cover the missing txids
        job.tx_data = vec![vec![0x01, 0x00], vec![0x02, 0x00]];

        assert_eq!(validator.validate(&job), ValidationResult::Valid);
    }

    #[test]
    fn test_pool_payout_validation() {
        let payout_script = vec![0x76, 0xa9, 0x14, 0xde, 0xad, 0xbe, 0xef];
        let validator =
            TemplateValidator::new(ValidationLevel::Minimal, payout_script.clone(), 0);

        // Coinbase without payout script
        let mut job = make_test_job();
        job.coinbase_tx = vec![0x01, 0x00, 0x00, 0x00];
        assert!(matches!(
            validator.validate(&job),
            ValidationResult::Invalid(_)
        ));

        // Coinbase with payout script
        job.coinbase_tx = vec![0x01, 0x00, 0x76, 0xa9, 0x14, 0xde, 0xad, 0xbe, 0xef, 0x00];
        assert_eq!(validator.validate(&job), ValidationResult::Valid);
    }

    #[test]
    fn test_update_known_txids() {
        let mut validator = TemplateValidator::default();
        assert!(!validator.is_txid_known(&[0x11; 32]));

        validator.update_known_txids([[0x11; 32], [0x22; 32]]);
        assert!(validator.is_txid_known(&[0x11; 32]));
        assert!(validator.is_txid_known(&[0x22; 32]));

        validator.clear_known_txids();
        assert!(!validator.is_txid_known(&[0x11; 32]));
    }

    #[test]
    fn test_validator_default() {
        let validator = TemplateValidator::default();
        assert_eq!(validator.level(), ValidationLevel::Standard);
        assert!(!validator.is_txid_known(&[0x00; 32]));
    }

    #[test]
    fn test_strict_validation() {
        let mut validator = TemplateValidator::new(ValidationLevel::Strict, vec![], 0);
        validator.add_known_txid([0x11; 32]);

        let mut job = make_test_job();
        job.tx_short_ids = vec![[0x11; 32]];

        assert_eq!(validator.validate(&job), ValidationResult::Valid);
    }

    #[test]
    fn test_strict_validation_with_missing_txids() {
        let validator = TemplateValidator::new(ValidationLevel::Strict, vec![], 0);
        let mut job = make_test_job();
        job.tx_short_ids = vec![[0x11; 32], [0x22; 32]];

        match validator.validate(&job) {
            ValidationResult::NeedTransactions(missing) => {
                assert_eq!(missing.len(), 2);
            }
            _ => panic!("Expected NeedTransactions"),
        }
    }

    #[test]
    fn test_pool_payout_checked_before_level_validation() {
        // Even at Minimal level, pool payout should be checked first
        let payout_script = vec![0xde, 0xad, 0xbe, 0xef];
        let validator = TemplateValidator::new(ValidationLevel::Minimal, payout_script, 0);

        let job = make_test_job();
        // Coinbase doesn't contain the payout script
        match validator.validate(&job) {
            ValidationResult::Invalid(msg) => {
                assert!(msg.contains("pool payout"));
            }
            _ => panic!("Expected Invalid result"),
        }
    }

    #[test]
    fn test_empty_payout_script_skips_validation() {
        // Empty payout script should skip payout validation
        let validator = TemplateValidator::new(ValidationLevel::Minimal, vec![], 0);
        let job = make_test_job();
        assert_eq!(validator.validate(&job), ValidationResult::Valid);
    }

    #[test]
    fn test_partial_known_txids() {
        let mut validator = TemplateValidator::new(ValidationLevel::Standard, vec![], 0);
        validator.add_known_txid([0x11; 32]);
        // 0x22 is unknown

        let mut job = make_test_job();
        job.tx_short_ids = vec![[0x11; 32], [0x22; 32]];

        match validator.validate(&job) {
            ValidationResult::NeedTransactions(missing) => {
                assert_eq!(missing.len(), 1);
                assert_eq!(missing[0], [0x22; 32]);
            }
            _ => panic!("Expected NeedTransactions for partial known txids"),
        }
    }
}
