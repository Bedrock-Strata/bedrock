//! Template validation for Full-Template mode

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_level_from_str() {
        assert_eq!(ValidationLevel::from_str("minimal"), Some(ValidationLevel::Minimal));
        assert_eq!(ValidationLevel::from_str("STANDARD"), Some(ValidationLevel::Standard));
        assert_eq!(ValidationLevel::from_str("Strict"), Some(ValidationLevel::Strict));
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
        assert_eq!(invalid, ValidationResult::Invalid("missing payout".to_string()));

        let need_tx = ValidationResult::NeedTransactions(vec![[0u8; 32], [1u8; 32]]);
        if let ValidationResult::NeedTransactions(txids) = need_tx {
            assert_eq!(txids.len(), 2);
        } else {
            panic!("Expected NeedTransactions variant");
        }
    }
}
