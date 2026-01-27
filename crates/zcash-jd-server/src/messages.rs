//! Job Declaration Protocol message types
//!
//! Message types for the JD protocol in Coinbase-Only mode:
//! - AllocateMiningJobToken: Client requests a token for job declaration
//! - AllocateMiningJobTokenSuccess: Server returns token and coinbase requirements
//! - SetCustomMiningJob: Client declares a custom mining job
//! - SetCustomMiningJobSuccess: Server confirms job acceptance
//! - SetCustomMiningJobError: Server rejects job with error code
//! - PushSolution: Client submits a block solution

#[allow(unused_imports)]
use serde::{Deserialize, Serialize};

/// Message type identifiers for JD protocol (0x50-0x5F range)
pub mod message_types {
    /// AllocateMiningJobToken message type
    pub const ALLOCATE_MINING_JOB_TOKEN: u8 = 0x50;
    /// AllocateMiningJobTokenSuccess message type
    pub const ALLOCATE_MINING_JOB_TOKEN_SUCCESS: u8 = 0x51;
    /// SetCustomMiningJob message type
    pub const SET_CUSTOM_MINING_JOB: u8 = 0x52;
    /// SetCustomMiningJobSuccess message type
    pub const SET_CUSTOM_MINING_JOB_SUCCESS: u8 = 0x53;
    /// SetCustomMiningJobError message type
    pub const SET_CUSTOM_MINING_JOB_ERROR: u8 = 0x54;
    /// PushSolution message type
    pub const PUSH_SOLUTION: u8 = 0x55;
}

/// Client -> Server: Request a token for job declaration
///
/// The client sends this message to request a token that will be used
/// to declare custom mining jobs. The token prevents replay attacks
/// and allows the server to track job allocations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllocateMiningJobToken {
    /// Request identifier for matching response
    pub request_id: u32,
    /// Human-readable identifier for the mining device (UTF-8)
    pub mining_device_id: String,
}

impl AllocateMiningJobToken {
    /// Create a new token allocation request
    pub fn new(request_id: u32, mining_device_id: impl Into<String>) -> Self {
        Self {
            request_id,
            mining_device_id: mining_device_id.into(),
        }
    }
}

/// Server -> Client: Token allocation success response
///
/// Contains the allocated token and constraints for coinbase transactions
/// that will be accepted by the pool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllocateMiningJobTokenSuccess {
    /// Request identifier matching the request
    pub request_id: u32,
    /// Allocated token for job declaration (unique identifier)
    pub mining_job_token: Vec<u8>,
    /// Minimum required coinbase transaction outputs
    /// This enforces pool payout addresses in the coinbase
    pub coinbase_output_constraints: Vec<CoinbaseOutputConstraint>,
    /// Maximum additional size allowed in coinbase (bytes)
    pub coinbase_output_max_additional_size: u32,
    /// Token validity period (seconds from now)
    pub token_validity_duration: u32,
}

impl AllocateMiningJobTokenSuccess {
    /// Check if the token allows additional coinbase space
    pub fn allows_additional_outputs(&self) -> bool {
        self.coinbase_output_max_additional_size > 0
    }
}

/// Constraint on coinbase transaction outputs
///
/// Pools use these to ensure their payout addresses are included
/// in any custom coinbase transactions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoinbaseOutputConstraint {
    /// Output script (scriptPubKey)
    pub output_script: Vec<u8>,
    /// Minimum value in zatoshis
    pub min_value: u64,
}

impl CoinbaseOutputConstraint {
    /// Create a new coinbase output constraint
    pub fn new(output_script: Vec<u8>, min_value: u64) -> Self {
        Self {
            output_script,
            min_value,
        }
    }
}

/// Client -> Server: Declare a custom mining job
///
/// The client uses a previously allocated token to declare a custom
/// mining job with their own coinbase transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetCustomMiningJob {
    /// Request identifier for matching response
    pub request_id: u32,
    /// Token from AllocateMiningJobTokenSuccess
    pub mining_job_token: Vec<u8>,
    /// Block version
    pub version: u32,
    /// Previous block hash (32 bytes)
    pub prev_hash: [u8; 32],
    /// Merkle root of transactions (32 bytes)
    pub merkle_root: [u8; 32],
    /// hashBlockCommitments for NU5+ (32 bytes)
    pub block_commitments: [u8; 32],
    /// Complete coinbase transaction (serialized)
    pub coinbase_tx: Vec<u8>,
    /// Block timestamp
    pub time: u32,
    /// Compact difficulty target (nBits)
    pub bits: u32,
}

impl SetCustomMiningJob {
    /// Construct the 140-byte block header for Equihash input
    /// (version || prev_hash || merkle_root || block_commitments || time || bits || nonce)
    pub fn build_header(&self, nonce: &[u8; 32]) -> [u8; 140] {
        let mut header = [0u8; 140];
        header[0..4].copy_from_slice(&self.version.to_le_bytes());
        header[4..36].copy_from_slice(&self.prev_hash);
        header[36..68].copy_from_slice(&self.merkle_root);
        header[68..100].copy_from_slice(&self.block_commitments);
        header[100..104].copy_from_slice(&self.time.to_le_bytes());
        header[104..108].copy_from_slice(&self.bits.to_le_bytes());
        header[108..140].copy_from_slice(nonce);
        header
    }

    /// Validate basic structure
    pub fn validate(&self) -> Result<(), SetCustomMiningJobErrorCode> {
        if self.mining_job_token.is_empty() {
            return Err(SetCustomMiningJobErrorCode::InvalidToken);
        }
        if self.coinbase_tx.is_empty() {
            return Err(SetCustomMiningJobErrorCode::InvalidCoinbase);
        }
        Ok(())
    }
}

/// Server -> Client: Custom job accepted
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetCustomMiningJobSuccess {
    /// Request identifier matching the request
    pub request_id: u32,
    /// Server-assigned job identifier
    pub job_id: u32,
    /// If true, discard all previous jobs for this channel
    pub clean_jobs: bool,
}

impl SetCustomMiningJobSuccess {
    /// Create a new success response
    pub fn new(request_id: u32, job_id: u32, clean_jobs: bool) -> Self {
        Self {
            request_id,
            job_id,
            clean_jobs,
        }
    }
}

/// Server -> Client: Custom job rejected
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetCustomMiningJobError {
    /// Request identifier matching the request
    pub request_id: u32,
    /// Error code indicating the reason for rejection
    pub error_code: SetCustomMiningJobErrorCode,
    /// Human-readable error message
    pub error_message: String,
}

impl SetCustomMiningJobError {
    /// Create a new error response
    pub fn new(
        request_id: u32,
        error_code: SetCustomMiningJobErrorCode,
        message: impl Into<String>,
    ) -> Self {
        Self {
            request_id,
            error_code,
            error_message: message.into(),
        }
    }

    /// Create error for invalid token
    pub fn invalid_token(request_id: u32) -> Self {
        Self::new(
            request_id,
            SetCustomMiningJobErrorCode::InvalidToken,
            "Invalid or unknown mining job token",
        )
    }

    /// Create error for expired token
    pub fn token_expired(request_id: u32) -> Self {
        Self::new(
            request_id,
            SetCustomMiningJobErrorCode::TokenExpired,
            "Mining job token has expired",
        )
    }

    /// Create error for invalid coinbase
    pub fn invalid_coinbase(request_id: u32, reason: impl Into<String>) -> Self {
        Self::new(
            request_id,
            SetCustomMiningJobErrorCode::InvalidCoinbase,
            reason,
        )
    }
}

/// Error codes for SetCustomMiningJob rejection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SetCustomMiningJobErrorCode {
    /// Token is invalid or unknown
    InvalidToken = 0x01,
    /// Token has expired
    TokenExpired = 0x02,
    /// Coinbase transaction is invalid
    InvalidCoinbase = 0x03,
    /// Coinbase doesn't meet output constraints
    CoinbaseConstraintViolation = 0x04,
    /// Previous block hash doesn't match current chain tip
    InvalidPrevHash = 0x05,
    /// Merkle root is invalid
    InvalidMerkleRoot = 0x06,
    /// Block version is not supported
    InvalidVersion = 0x07,
    /// nBits doesn't match network difficulty
    InvalidBits = 0x08,
    /// Server is overloaded
    ServerOverloaded = 0x09,
    /// Other error
    Other = 0xFF,
}

impl SetCustomMiningJobErrorCode {
    /// Convert error code to u8
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    /// Try to convert from u8
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0x01 => Some(Self::InvalidToken),
            0x02 => Some(Self::TokenExpired),
            0x03 => Some(Self::InvalidCoinbase),
            0x04 => Some(Self::CoinbaseConstraintViolation),
            0x05 => Some(Self::InvalidPrevHash),
            0x06 => Some(Self::InvalidMerkleRoot),
            0x07 => Some(Self::InvalidVersion),
            0x08 => Some(Self::InvalidBits),
            0x09 => Some(Self::ServerOverloaded),
            0xFF => Some(Self::Other),
            _ => None,
        }
    }
}

impl std::fmt::Display for SetCustomMiningJobErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidToken => write!(f, "invalid token"),
            Self::TokenExpired => write!(f, "token expired"),
            Self::InvalidCoinbase => write!(f, "invalid coinbase"),
            Self::CoinbaseConstraintViolation => write!(f, "coinbase constraint violation"),
            Self::InvalidPrevHash => write!(f, "invalid previous hash"),
            Self::InvalidMerkleRoot => write!(f, "invalid merkle root"),
            Self::InvalidVersion => write!(f, "invalid version"),
            Self::InvalidBits => write!(f, "invalid bits"),
            Self::ServerOverloaded => write!(f, "server overloaded"),
            Self::Other => write!(f, "other error"),
        }
    }
}

/// Client -> Server: Submit a block solution
///
/// When a miner finds a valid solution for a custom job,
/// they submit it to the server for block propagation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushSolution {
    /// Channel ID this solution belongs to
    pub channel_id: u32,
    /// Job ID from SetCustomMiningJobSuccess
    pub job_id: u32,
    /// Full 32-byte nonce
    pub nonce: [u8; 32],
    /// Block timestamp (may differ from job time)
    pub time: u32,
    /// Equihash (200,9) solution (1344 bytes)
    pub solution: [u8; 1344],
}

impl PushSolution {
    /// Equihash (200,9) solution size
    pub const SOLUTION_SIZE: usize = 1344;

    /// Create a new solution submission
    pub fn new(
        channel_id: u32,
        job_id: u32,
        nonce: [u8; 32],
        time: u32,
        solution: [u8; 1344],
    ) -> Self {
        Self {
            channel_id,
            job_id,
            nonce,
            time,
            solution,
        }
    }

    /// Validate solution length (always true for fixed-size array)
    pub fn validate_solution_len(&self) -> bool {
        self.solution.len() == Self::SOLUTION_SIZE
    }
}

/// Server -> Client: Solution accepted/rejected
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushSolutionResponse {
    /// Channel ID
    pub channel_id: u32,
    /// Job ID
    pub job_id: u32,
    /// Result of solution validation
    pub result: SolutionResult,
}

/// Result of solution validation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SolutionResult {
    /// Solution accepted and block submitted to network
    Accepted {
        /// Block hash if successfully mined
        block_hash: Option<[u8; 32]>,
    },
    /// Solution rejected with reason
    Rejected(SolutionRejectReason),
}

/// Reasons for solution rejection
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SolutionRejectReason {
    /// Job ID not found or expired
    StaleJob,
    /// Duplicate solution already submitted
    Duplicate,
    /// Solution does not verify (invalid Equihash)
    InvalidSolution,
    /// Block doesn't meet network difficulty
    LowDifficulty,
    /// Nonce is invalid
    InvalidNonce,
    /// Other error
    Other(String),
}

impl std::fmt::Display for SolutionRejectReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StaleJob => write!(f, "stale job"),
            Self::Duplicate => write!(f, "duplicate solution"),
            Self::InvalidSolution => write!(f, "invalid solution"),
            Self::LowDifficulty => write!(f, "low difficulty"),
            Self::InvalidNonce => write!(f, "invalid nonce"),
            Self::Other(msg) => write!(f, "other: {}", msg),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allocate_token_request() {
        let request = AllocateMiningJobToken::new(1, "miner-001");
        assert_eq!(request.request_id, 1);
        assert_eq!(request.mining_device_id, "miner-001");
    }

    #[test]
    fn test_allocate_token_success() {
        let response = AllocateMiningJobTokenSuccess {
            request_id: 1,
            mining_job_token: vec![0x01, 0x02, 0x03],
            coinbase_output_constraints: vec![
                CoinbaseOutputConstraint::new(vec![0x76, 0xa9], 100_000_000),
            ],
            coinbase_output_max_additional_size: 1000,
            token_validity_duration: 3600,
        };

        assert!(response.allows_additional_outputs());
        assert_eq!(response.coinbase_output_constraints.len(), 1);
        assert_eq!(response.coinbase_output_constraints[0].min_value, 100_000_000);
    }

    #[test]
    fn test_set_custom_mining_job_validation() {
        let job = SetCustomMiningJob {
            request_id: 1,
            mining_job_token: vec![0x01, 0x02, 0x03],
            version: 5,
            prev_hash: [0xaa; 32],
            merkle_root: [0xbb; 32],
            block_commitments: [0xcc; 32],
            coinbase_tx: vec![0x01, 0x00, 0x00, 0x00], // minimal tx
            time: 1700000000,
            bits: 0x1d00ffff,
        };

        assert!(job.validate().is_ok());

        // Test with empty token
        let bad_job = SetCustomMiningJob {
            mining_job_token: vec![],
            ..job.clone()
        };
        assert_eq!(
            bad_job.validate().unwrap_err(),
            SetCustomMiningJobErrorCode::InvalidToken
        );

        // Test with empty coinbase
        let bad_job = SetCustomMiningJob {
            coinbase_tx: vec![],
            ..job
        };
        assert_eq!(
            bad_job.validate().unwrap_err(),
            SetCustomMiningJobErrorCode::InvalidCoinbase
        );
    }

    #[test]
    fn test_build_header() {
        let job = SetCustomMiningJob {
            request_id: 1,
            mining_job_token: vec![0x01],
            version: 5,
            prev_hash: [0xaa; 32],
            merkle_root: [0xbb; 32],
            block_commitments: [0xcc; 32],
            coinbase_tx: vec![0x01],
            time: 0x12345678,
            bits: 0xaabbccdd,
        };

        let nonce = [0xff; 32];
        let header = job.build_header(&nonce);

        assert_eq!(header.len(), 140);
        // Version at offset 0 (little-endian)
        assert_eq!(&header[0..4], &[0x05, 0x00, 0x00, 0x00]);
        // prev_hash at offset 4
        assert_eq!(&header[4..36], &[0xaa; 32]);
        // merkle_root at offset 36
        assert_eq!(&header[36..68], &[0xbb; 32]);
        // block_commitments at offset 68
        assert_eq!(&header[68..100], &[0xcc; 32]);
        // time at offset 100 (little-endian)
        assert_eq!(&header[100..104], &[0x78, 0x56, 0x34, 0x12]);
        // bits at offset 104 (little-endian)
        assert_eq!(&header[104..108], &[0xdd, 0xcc, 0xbb, 0xaa]);
        // nonce at offset 108
        assert_eq!(&header[108..140], &[0xff; 32]);
    }

    #[test]
    fn test_error_codes() {
        // Test conversion roundtrip
        for code in [
            SetCustomMiningJobErrorCode::InvalidToken,
            SetCustomMiningJobErrorCode::TokenExpired,
            SetCustomMiningJobErrorCode::InvalidCoinbase,
            SetCustomMiningJobErrorCode::CoinbaseConstraintViolation,
            SetCustomMiningJobErrorCode::InvalidPrevHash,
            SetCustomMiningJobErrorCode::InvalidMerkleRoot,
            SetCustomMiningJobErrorCode::InvalidVersion,
            SetCustomMiningJobErrorCode::InvalidBits,
            SetCustomMiningJobErrorCode::ServerOverloaded,
            SetCustomMiningJobErrorCode::Other,
        ] {
            let byte = code.as_u8();
            let recovered = SetCustomMiningJobErrorCode::from_u8(byte).unwrap();
            assert_eq!(code, recovered);
        }

        // Test invalid code
        assert!(SetCustomMiningJobErrorCode::from_u8(0x00).is_none());
        assert!(SetCustomMiningJobErrorCode::from_u8(0x10).is_none());
    }

    #[test]
    fn test_error_helpers() {
        let error = SetCustomMiningJobError::invalid_token(42);
        assert_eq!(error.request_id, 42);
        assert_eq!(error.error_code, SetCustomMiningJobErrorCode::InvalidToken);

        let error = SetCustomMiningJobError::token_expired(43);
        assert_eq!(error.request_id, 43);
        assert_eq!(error.error_code, SetCustomMiningJobErrorCode::TokenExpired);

        let error = SetCustomMiningJobError::invalid_coinbase(44, "missing pool output");
        assert_eq!(error.request_id, 44);
        assert_eq!(error.error_code, SetCustomMiningJobErrorCode::InvalidCoinbase);
        assert_eq!(error.error_message, "missing pool output");
    }

    #[test]
    fn test_push_solution() {
        let solution = PushSolution::new(
            1,      // channel_id
            100,    // job_id
            [0x11; 32], // nonce
            1700000000, // time
            [0x22; 1344], // solution
        );

        assert_eq!(solution.channel_id, 1);
        assert_eq!(solution.job_id, 100);
        assert!(solution.validate_solution_len());
    }

    #[test]
    fn test_solution_result() {
        let accepted = SolutionResult::Accepted {
            block_hash: Some([0xaa; 32]),
        };
        match accepted {
            SolutionResult::Accepted { block_hash } => {
                assert!(block_hash.is_some());
            }
            _ => panic!("Expected Accepted"),
        }

        let rejected = SolutionResult::Rejected(SolutionRejectReason::StaleJob);
        match rejected {
            SolutionResult::Rejected(reason) => {
                assert_eq!(reason.to_string(), "stale job");
            }
            _ => panic!("Expected Rejected"),
        }
    }

    #[test]
    fn test_message_type_constants() {
        // Verify message types are in the 0x50-0x5F range
        assert_eq!(message_types::ALLOCATE_MINING_JOB_TOKEN, 0x50);
        assert_eq!(message_types::ALLOCATE_MINING_JOB_TOKEN_SUCCESS, 0x51);
        assert_eq!(message_types::SET_CUSTOM_MINING_JOB, 0x52);
        assert_eq!(message_types::SET_CUSTOM_MINING_JOB_SUCCESS, 0x53);
        assert_eq!(message_types::SET_CUSTOM_MINING_JOB_ERROR, 0x54);
        assert_eq!(message_types::PUSH_SOLUTION, 0x55);
    }
}
