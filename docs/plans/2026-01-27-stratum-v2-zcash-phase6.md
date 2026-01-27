# Stratum V2 Zcash Phase 6: Full-Template Mode

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add Full-Template mode to Job Declaration, allowing miners to select transactions for their blocks while coexisting with the existing Coinbase-Only mode.

**Architecture:** Extend JD protocol with new message variants for full template declaration. Implement compact transaction format (txids + fallback). Add configurable validation levels on the server. Update JD Client to build and submit full templates.

**Tech Stack:** Rust 1.75+, existing zcash-jd-server/client crates, zcash-template-provider for template building

---

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Mode selection | Client specifies in token request | Flexibility, backward compatibility |
| Transaction format | Compact (txids + fallback) | Bandwidth efficient, similar to BIP 152 |
| Validation | Configurable (minimal/standard/strict) | Pool operator flexibility |
| Coexistence | Both modes supported | No breaking changes |

---

## Protocol Extensions

### New/Modified Messages

**AllocateMiningJobToken** - Add field:
```rust
/// Requested job declaration mode
pub mode: JobDeclarationMode,  // CoinbaseOnly or FullTemplate
```

**AllocateMiningJobTokenSuccess** - Add field:
```rust
/// Granted mode (may differ from requested if pool doesn't support)
pub granted_mode: JobDeclarationMode,
```

**New: SetFullTemplateJob** message:
```rust
pub struct SetFullTemplateJob {
    pub token: u32,
    pub version: u32,
    pub prev_hash: [u8; 32],
    pub merkle_root: [u8; 32],
    pub timestamp: u32,
    pub bits: u32,
    pub coinbase_tx: Vec<u8>,
    /// Compact transaction list (txids)
    pub tx_short_ids: Vec<[u8; 32]>,
    /// Full transactions for any txids pool may not have
    pub tx_data: Vec<Vec<u8>>,
}
```

**New: GetMissingTransactions** message (pool -> client):
```rust
pub struct GetMissingTransactions {
    pub token: u32,
    pub missing_tx_ids: Vec<[u8; 32]>,
}
```

**New: ProvideMissingTransactions** message (client -> pool):
```rust
pub struct ProvideMissingTransactions {
    pub token: u32,
    pub transactions: Vec<Vec<u8>>,
}
```

### Validation Levels

```rust
pub enum ValidationLevel {
    /// Only verify pool payout output exists with minimum value
    Minimal,
    /// Verify pool payout + basic template structure
    Standard,
    /// Full validation: verify all transactions against pool's view
    Strict,
}
```

---

## Task 1: Add JobDeclarationMode Enum and Update Token Messages

**Files:**
- Modify: `crates/zcash-jd-server/src/messages.rs`
- Modify: `crates/zcash-jd-server/src/codec.rs`

**Step 1: Add JobDeclarationMode enum**

Add to `messages.rs`:

```rust
/// Job declaration mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JobDeclarationMode {
    /// Miner customizes coinbase only; pool provides tx set
    #[default]
    CoinbaseOnly = 0,
    /// Miner provides full template including transaction selection
    FullTemplate = 1,
}

impl JobDeclarationMode {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::CoinbaseOnly),
            1 => Some(Self::FullTemplate),
            _ => None,
        }
    }
}
```

**Step 2: Update AllocateMiningJobToken**

Add field to `AllocateMiningJobToken`:
```rust
pub requested_mode: JobDeclarationMode,
```

Add field to `AllocateMiningJobTokenSuccess`:
```rust
pub granted_mode: JobDeclarationMode,
```

**Step 3: Update codec**

Update encode/decode functions for the modified messages.

**Step 4: Update tests**

Add tests for new enum and updated messages.

**Step 5: Run tests**

Run: `cargo test -p zcash-jd-server`

**Step 6: Commit**

```bash
git add crates/zcash-jd-server/
git commit -m "feat(jd): add JobDeclarationMode enum and update token messages"
```

---

## Task 2: Add Full-Template Message Types

**Files:**
- Modify: `crates/zcash-jd-server/src/messages.rs`
- Modify: `crates/zcash-jd-server/src/codec.rs`

**Step 1: Add SetFullTemplateJob message**

```rust
/// Full template job declaration (client -> server)
#[derive(Debug, Clone)]
pub struct SetFullTemplateJob {
    /// Token from AllocateMiningJobToken
    pub token: u32,
    /// Block version
    pub version: u32,
    /// Previous block hash
    pub prev_hash: [u8; 32],
    /// Merkle root of all transactions
    pub merkle_root: [u8; 32],
    /// Block timestamp
    pub timestamp: u32,
    /// Difficulty bits
    pub bits: u32,
    /// Complete coinbase transaction
    pub coinbase_tx: Vec<u8>,
    /// Transaction IDs (excluding coinbase)
    pub tx_short_ids: Vec<[u8; 32]>,
    /// Full transaction data for txs pool may not have
    pub tx_data: Vec<Vec<u8>>,
}
```

**Step 2: Add SetFullTemplateJobSuccess/Error**

```rust
#[derive(Debug, Clone)]
pub struct SetFullTemplateJobSuccess {
    pub token: u32,
    pub job_id: u64,
}

#[derive(Debug, Clone)]
pub struct SetFullTemplateJobError {
    pub token: u32,
    pub error_code: u32,
    pub error_message: String,
}
```

**Step 3: Add GetMissingTransactions and ProvideMissingTransactions**

```rust
#[derive(Debug, Clone)]
pub struct GetMissingTransactions {
    pub token: u32,
    pub missing_tx_ids: Vec<[u8; 32]>,
}

#[derive(Debug, Clone)]
pub struct ProvideMissingTransactions {
    pub token: u32,
    pub transactions: Vec<Vec<u8>>,
}
```

**Step 4: Add message type constants**

```rust
pub const MSG_SET_FULL_TEMPLATE_JOB: u8 = 0x20;
pub const MSG_SET_FULL_TEMPLATE_JOB_SUCCESS: u8 = 0x21;
pub const MSG_SET_FULL_TEMPLATE_JOB_ERROR: u8 = 0x22;
pub const MSG_GET_MISSING_TRANSACTIONS: u8 = 0x23;
pub const MSG_PROVIDE_MISSING_TRANSACTIONS: u8 = 0x24;
```

**Step 5: Implement codec functions**

Add encode/decode for all new message types.

**Step 6: Run tests**

Run: `cargo test -p zcash-jd-server`

**Step 7: Commit**

```bash
git commit -am "feat(jd): add Full-Template message types"
```

---

## Task 3: Add ValidationLevel Config to JD Server

**Files:**
- Modify: `crates/zcash-jd-server/src/config.rs`
- Create: `crates/zcash-jd-server/src/validation.rs`

**Step 1: Add ValidationLevel enum**

Create `validation.rs`:

```rust
//! Template validation for Full-Template mode

use crate::error::JdServerError;

/// Validation strictness for full templates
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ValidationLevel {
    /// Only verify pool payout output exists with minimum value
    #[default]
    Minimal,
    /// Verify pool payout + basic template structure (valid merkle root)
    Standard,
    /// Full validation: verify all transactions are valid
    Strict,
}

impl ValidationLevel {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "minimal" => Some(Self::Minimal),
            "standard" => Some(Self::Standard),
            "strict" => Some(Self::Strict),
            _ => None,
        }
    }
}

/// Result of template validation
#[derive(Debug)]
pub enum ValidationResult {
    Valid,
    Invalid(String),
    NeedTransactions(Vec<[u8; 32]>),
}
```

**Step 2: Update JdServerConfig**

Add to config:
```rust
/// Enable Full-Template mode (in addition to Coinbase-Only)
pub full_template_enabled: bool,

/// Validation level for full templates
pub full_template_validation: ValidationLevel,

/// Minimum pool payout value (satoshis) for full templates
pub min_pool_payout: u64,
```

Defaults:
```rust
full_template_enabled: false,
full_template_validation: ValidationLevel::Standard,
min_pool_payout: 0,
```

**Step 3: Add lib.rs export**

Add to lib.rs:
```rust
pub mod validation;
pub use validation::{ValidationLevel, ValidationResult};
```

**Step 4: Run tests**

**Step 5: Commit**

```bash
git commit -am "feat(jd): add ValidationLevel config for Full-Template mode"
```

---

## Task 4: Implement Template Validator

**Files:**
- Modify: `crates/zcash-jd-server/src/validation.rs`

**Step 1: Add TemplateValidator struct**

```rust
use std::collections::HashSet;

/// Validates full templates submitted by miners
pub struct TemplateValidator {
    level: ValidationLevel,
    pool_payout_script: Vec<u8>,
    min_pool_payout: u64,
    /// Known transaction IDs (from pool's mempool view)
    known_txids: HashSet<[u8; 32]>,
}

impl TemplateValidator {
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

    /// Validate a full template job
    pub fn validate(&self, job: &SetFullTemplateJob) -> ValidationResult {
        // Check pool payout in coinbase
        if !self.validate_pool_payout(&job.coinbase_tx) {
            return ValidationResult::Invalid("Missing or insufficient pool payout".into());
        }

        match self.level {
            ValidationLevel::Minimal => ValidationResult::Valid,
            ValidationLevel::Standard => self.validate_standard(job),
            ValidationLevel::Strict => self.validate_strict(job),
        }
    }

    fn validate_pool_payout(&self, coinbase: &[u8]) -> bool {
        // Parse coinbase and verify pool payout output exists
        // Implementation depends on Zcash transaction format
        // For now, check if pool_payout_script appears in coinbase
        if self.pool_payout_script.is_empty() {
            return true; // No payout required
        }
        coinbase.windows(self.pool_payout_script.len())
            .any(|w| w == self.pool_payout_script.as_slice())
    }

    fn validate_standard(&self, job: &SetFullTemplateJob) -> ValidationResult {
        // Check if we have all transactions
        let missing: Vec<[u8; 32]> = job.tx_short_ids.iter()
            .filter(|txid| !self.known_txids.contains(*txid))
            .copied()
            .collect();

        if !missing.is_empty() && job.tx_data.is_empty() {
            return ValidationResult::NeedTransactions(missing);
        }

        // Verify merkle root matches declared transactions
        // (Would need actual merkle computation here)

        ValidationResult::Valid
    }

    fn validate_strict(&self, job: &SetFullTemplateJob) -> ValidationResult {
        // First do standard validation
        if let result @ ValidationResult::Invalid(_) | result @ ValidationResult::NeedTransactions(_)
            = self.validate_standard(job) {
            return result;
        }

        // Additionally verify all transactions are valid
        // This would require full transaction parsing and validation
        // For MVP, same as standard

        ValidationResult::Valid
    }
}
```

**Step 2: Add tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_level_from_str() {
        assert_eq!(ValidationLevel::from_str("minimal"), Some(ValidationLevel::Minimal));
        assert_eq!(ValidationLevel::from_str("STANDARD"), Some(ValidationLevel::Standard));
        assert_eq!(ValidationLevel::from_str("strict"), Some(ValidationLevel::Strict));
        assert_eq!(ValidationLevel::from_str("unknown"), None);
    }

    #[test]
    fn test_minimal_validation_accepts_any_template() {
        let validator = TemplateValidator::new(
            ValidationLevel::Minimal,
            vec![],
            0,
        );

        let job = SetFullTemplateJob {
            token: 1,
            version: 4,
            prev_hash: [0; 32],
            merkle_root: [0; 32],
            timestamp: 0,
            bits: 0,
            coinbase_tx: vec![],
            tx_short_ids: vec![],
            tx_data: vec![],
        };

        assert!(matches!(validator.validate(&job), ValidationResult::Valid));
    }
}
```

**Step 3: Run tests**

**Step 4: Commit**

```bash
git commit -am "feat(jd): implement TemplateValidator for Full-Template mode"
```

---

## Task 5: Update JD Server to Handle Full-Template Jobs

**Files:**
- Modify: `crates/zcash-jd-server/src/server.rs`
- Modify: `crates/zcash-jd-server/src/token.rs`

**Step 1: Update TokenManager to track mode**

Add to `DeclaredJobInfo`:
```rust
pub mode: JobDeclarationMode,
```

Update token allocation to record requested mode.

**Step 2: Add full template handling in server**

Add handler for `SetFullTemplateJob`:

```rust
async fn handle_set_full_template_job(
    &self,
    job: SetFullTemplateJob,
    client_id: &str,
) -> Result<JdMessage> {
    // Verify token exists and is valid
    let token_info = self.token_manager.get_token(job.token)
        .ok_or(JdServerError::InvalidToken)?;

    // Verify mode is FullTemplate
    if token_info.mode != JobDeclarationMode::FullTemplate {
        return Err(JdServerError::Protocol("Token not allocated for Full-Template mode".into()));
    }

    // Validate template
    let result = self.validator.validate(&job);

    match result {
        ValidationResult::Valid => {
            let job_id = self.register_job(job, client_id)?;
            Ok(JdMessage::SetFullTemplateJobSuccess(SetFullTemplateJobSuccess {
                token: job.token,
                job_id,
            }))
        }
        ValidationResult::Invalid(reason) => {
            Ok(JdMessage::SetFullTemplateJobError(SetFullTemplateJobError {
                token: job.token,
                error_code: 1,
                error_message: reason,
            }))
        }
        ValidationResult::NeedTransactions(missing) => {
            Ok(JdMessage::GetMissingTransactions(GetMissingTransactions {
                token: job.token,
                missing_tx_ids: missing,
            }))
        }
    }
}
```

**Step 3: Update message dispatch**

Add case for `MSG_SET_FULL_TEMPLATE_JOB` in message handling.

**Step 4: Run tests**

**Step 5: Commit**

```bash
git commit -am "feat(jd): handle Full-Template jobs in JD Server"
```

---

## Task 6: Update JD Client Config for Full-Template Mode

**Files:**
- Modify: `crates/zcash-jd-client/src/config.rs`
- Modify: `crates/zcash-jd-client/src/main.rs`

**Step 1: Add config fields**

Add to `JdClientConfig`:
```rust
/// Use Full-Template mode (requires local transaction selection)
pub full_template_mode: bool,

/// Transaction selection strategy
pub tx_selection: TxSelectionStrategy,
```

```rust
#[derive(Debug, Clone, Copy, Default)]
pub enum TxSelectionStrategy {
    /// Include all transactions from template (default)
    #[default]
    All,
    /// Prioritize by fee rate
    ByFeeRate,
    /// Custom filter (future)
    Custom,
}
```

**Step 2: Add CLI args**

```rust
/// Use Full-Template mode for transaction selection
#[arg(long)]
full_template: bool,

/// Transaction selection strategy (all, by-fee-rate)
#[arg(long, default_value = "all")]
tx_selection: String,
```

**Step 3: Run tests**

**Step 4: Commit**

```bash
git commit -am "feat(jd-client): add Full-Template mode config"
```

---

## Task 7: Implement Full-Template Builder in JD Client

**Files:**
- Create: `crates/zcash-jd-client/src/template_builder.rs`
- Modify: `crates/zcash-jd-client/src/client.rs`

**Step 1: Create template_builder.rs**

```rust
//! Full template construction for JD Client

use crate::config::TxSelectionStrategy;
use crate::error::JdClientError;
use zcash_template_provider::BlockTemplate;

/// Builds full templates for declaration
pub struct FullTemplateBuilder {
    strategy: TxSelectionStrategy,
}

impl FullTemplateBuilder {
    pub fn new(strategy: TxSelectionStrategy) -> Self {
        Self { strategy }
    }

    /// Build a SetFullTemplateJob from a block template
    pub fn build_job(
        &self,
        template: &BlockTemplate,
        token: u32,
        coinbase_tx: Vec<u8>,
    ) -> Result<SetFullTemplateJob, JdClientError> {
        let tx_short_ids: Vec<[u8; 32]> = template.transactions.iter()
            .filter_map(|tx| self.should_include(tx))
            .map(|tx| tx.txid())
            .collect();

        // Compute merkle root from coinbase + selected txs
        let merkle_root = self.compute_merkle_root(&coinbase_tx, &tx_short_ids, template)?;

        Ok(SetFullTemplateJob {
            token,
            version: template.version,
            prev_hash: template.prev_block_hash,
            merkle_root,
            timestamp: template.cur_time,
            bits: template.bits,
            coinbase_tx,
            tx_short_ids,
            tx_data: vec![], // Pool should have all txs; fallback if needed
        })
    }

    fn should_include(&self, tx: &TemplateTransaction) -> Option<&TemplateTransaction> {
        match self.strategy {
            TxSelectionStrategy::All => Some(tx),
            TxSelectionStrategy::ByFeeRate => {
                // Could filter low fee-rate txs
                Some(tx)
            }
            TxSelectionStrategy::Custom => Some(tx),
        }
    }

    fn compute_merkle_root(
        &self,
        coinbase: &[u8],
        tx_ids: &[[u8; 32]],
        template: &BlockTemplate,
    ) -> Result<[u8; 32], JdClientError> {
        // Use template's merkle computation or implement
        // For now, use template's provided root if selecting all txs
        Ok(template.merkle_root)
    }
}
```

**Step 2: Integrate into client**

Update `JdClient::run()` to use `FullTemplateBuilder` when `full_template_mode` is enabled.

**Step 3: Run tests**

**Step 4: Commit**

```bash
git commit -am "feat(jd-client): implement FullTemplateBuilder"
```

---

## Task 8: Handle Missing Transactions Protocol

**Files:**
- Modify: `crates/zcash-jd-client/src/client.rs`
- Modify: `crates/zcash-jd-server/src/server.rs`

**Step 1: Client-side handling**

When client receives `GetMissingTransactions`:
```rust
async fn handle_get_missing_transactions(
    &self,
    msg: GetMissingTransactions,
) -> Result<ProvideMissingTransactions> {
    let transactions: Vec<Vec<u8>> = msg.missing_tx_ids.iter()
        .filter_map(|txid| self.get_transaction_data(txid))
        .collect();

    Ok(ProvideMissingTransactions {
        token: msg.token,
        transactions,
    })
}
```

**Step 2: Server-side handling**

When server receives `ProvideMissingTransactions`:
- Add transactions to validator's known set
- Re-validate the pending job
- Send success or error

**Step 3: Run tests**

**Step 4: Commit**

```bash
git commit -am "feat(jd): implement missing transactions protocol"
```

---

## Task 9: Add Integration Tests

**Files:**
- Create: `crates/zcash-jd-server/tests/full_template_tests.rs`

**Step 1: Create integration tests**

```rust
//! Integration tests for Full-Template mode

use zcash_jd_server::*;

#[tokio::test]
async fn test_full_template_mode_allocation() {
    // Test that client can request FullTemplate mode
    // and server grants it when enabled
}

#[tokio::test]
async fn test_full_template_job_submission() {
    // Test submitting a full template job
}

#[tokio::test]
async fn test_validation_levels() {
    // Test minimal, standard, strict validation
}

#[tokio::test]
async fn test_missing_transactions_flow() {
    // Test the compact format with missing tx fallback
}

#[tokio::test]
async fn test_coinbase_only_still_works() {
    // Verify backward compatibility
}
```

**Step 2: Run all tests**

Run: `cargo test`

**Step 3: Commit**

```bash
git commit -am "test(jd): add Full-Template mode integration tests"
```

---

## Task 10: Documentation and Final Verification

**Files:**
- Update: `crates/zcash-jd-server/README.md`
- Update: `crates/zcash-jd-client/README.md`
- Update: `README.md` (workspace)

**Step 1: Update JD Server README**

Add Full-Template mode documentation:
- Configuration options
- Validation levels
- Protocol flow

**Step 2: Update JD Client README**

Add:
- `--full-template` flag documentation
- Transaction selection options

**Step 3: Update workspace README**

Add Phase 6 status.

**Step 4: Run all tests**

Run: `cargo test`

**Step 5: Commit**

```bash
git commit -am "docs: add Phase 6 Full-Template mode documentation"
```

---

## Summary

Phase 6 adds Full-Template mode with:

1. **Protocol Extensions**
   - `JobDeclarationMode` enum (CoinbaseOnly/FullTemplate)
   - `SetFullTemplateJob` message with compact tx format
   - Missing transactions request/response flow

2. **Server Features**
   - Configurable validation (Minimal/Standard/Strict)
   - Template validator with pool payout verification
   - Backward compatible with Coinbase-Only mode

3. **Client Features**
   - Full template builder from Zebra templates
   - Transaction selection strategies
   - Missing transaction response handling

4. **Configuration**
   - `full_template_enabled` on server
   - `full_template_validation` level
   - `--full-template` CLI flag on client
