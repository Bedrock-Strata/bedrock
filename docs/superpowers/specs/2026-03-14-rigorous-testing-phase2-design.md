# Rigorous Testing Phase 2 -- Design Spec

**Goal:** Catch bugs before mainnet by filling critical testing gaps: end-to-end mining flow, concurrent stress tests, error path coverage for network-facing code, and porting upstream test patterns from Bitcoin Core BIP 152 and SRI Stratum V2.

**Depends on:** Phase 1 (completed) which added proptest, config validation, payout edge cases, codec roundtrips, Noise transport edge cases, and compact block reconstruction tests.

---

## Section 1: Mock Zebra RPC + TestTemplateFactory

### Problem

`TemplateProvider` depends on `ZebraRpc` which makes HTTP calls to a live Zebra node. The entire template -> job -> share pipeline is untestable without it.

### Design

**Trait extraction:** Define a `RpcProvider` trait in `zcash-template-provider`:

```rust
#[async_trait]
pub trait RpcProvider: Send + Sync {
    async fn get_block_template(&self) -> Result<GetBlockTemplateResponse>;
    async fn submit_block(&self, block_hex: &str) -> Result<Option<String>>;
    async fn get_best_block_hash(&self) -> Result<String>;
}
```

`ZebraRpc` implements this trait. A `MockZebraRpc` (test-only) returns pre-built responses from a `VecDeque`, with configurable failure injection.

**TestTemplateFactory:** Helper that builds valid `GetBlockTemplateResponse` values with sensible defaults. Builder methods allow tweaking individual fields:

```rust
TestTemplateFactory::new()
    .height(500_000)
    .prev_hash("00000000...")
    .with_transactions(3)
    .build()
```

This avoids every test manually constructing the full 10+ field response struct.

### Files

- Modify: `crates/zcash-template-provider/src/rpc.rs` -- extract `RpcProvider` trait
- Modify: `crates/zcash-template-provider/src/template.rs` -- make `TemplateProvider` generic over `RpcProvider`
- Create: `crates/zcash-template-provider/src/testutil.rs` -- `MockZebraRpc` + `TestTemplateFactory` (behind `#[cfg(test)]` or `pub` for cross-crate test use)

---

## Section 2: End-to-End Mining Flow Integration Test

### Problem

No test exercises the full mining lifecycle. Integration bugs between components (template -> job -> share -> validation -> payout) can only be caught by testing the complete pipeline.

### Design

A single comprehensive test that runs entirely in-process, no network required:

1. **Setup:** `MockZebraRpc` with valid template, `JobDistributor`, `ShareProcessor`, `InMemoryDuplicateDetector`, `PayoutTracker`, `Channel` with 4-byte nonce_1
2. **Template -> Job:** Feed mock template through `JobDistributor::update_template()`, create `NewEquihashJob` for the channel
3. **Share submission:** Build `SubmitEquihashShare` with valid timestamp and nonce_2. Run through `ShareProcessor::validate_share_with_job()`. Verify the pipeline executes correctly (share is rejected for InvalidSolution since we can't produce real Equihash, but the pipeline doesn't panic or return unexpected errors)
4. **Duplicate detection:** Same share again -> `Duplicate` rejection
5. **Timestamp validation:** Shares at boundary values (job_time - 60, job_time + 7200 accepted; job_time - 61, job_time + 7201 rejected)
6. **Payout tracking:** Record shares, verify `PayoutTracker` accumulates correctly
7. **Vardiff cycle:** Record multiple shares, trigger `maybe_retarget()`, verify difficulty adjusts
8. **New block:** Second template with different `prev_hash`, verify new block detection, `clean_jobs` invalidates old jobs

### Files

- Create: `crates/zcash-pool-server/tests/e2e_mining_flow.rs`

---

## Section 3: Concurrent Stress Tests

### Problem

The pool server's async code (PayoutTracker, DuplicateDetector, JobDistributor, Channel) uses `RwLock<HashMap>` and `Arc` sharing across tasks. No tests exercise these under contention.

### Design

All tests use `#[tokio::test(flavor = "multi_thread")]` and `tokio::sync::Barrier` to ensure tasks start simultaneously.

**Test 1: PayoutTracker under contention.**
50 tasks x 1000 shares each -> verify total_shares == 50,000 and total_difficulty is exact expected sum. Catches lock poisoning, lost updates, f64 drift.

**Test 2: DuplicateDetector concurrent submissions.**
100 tasks submit the exact same (job_id, nonce_2, solution) simultaneously. Exactly 1 succeeds, 99 report duplicate. Catches TOCTOU in check-and-record.

**Test 3: JobDistributor template update race.**
1 task rapidly updates templates with incrementing heights. 20 tasks call `create_job()` concurrently. No panics, no jobs returned for superseded heights.

**Test 4: Channel cleanup during active reads.**
1 task calls `add_job` with `clean_jobs=true` while another calls `is_job_active()`. No panics.

### Files

- Create: `crates/zcash-pool-server/tests/concurrent_stress.rs`

---

## Section 4: ForgeRelay Error Path Tests

### Problem

`forge.rs` has 200 lines and zero tests. All error paths in block relay construction are uncovered.

### Design

Extract `build_compact_block_from_template()` and `compute_header_hash()` as testable standalone functions (or methods on a lightweight struct without the `RelayClient`). This avoids needing UDP socket binding in unit tests.

**Test 1:** `new()` rejects empty peers -> `Err(PoolError::Config(...))`

**Test 2:** `build_compact_block_from_template()` with valid template -> correct short ID count, coinbase prefilled at index 0, header is 1487 bytes (140 + 3 compactSize + 1344 placeholder solution)

**Test 3:** Template with invalid (non-hex) tx hash -> transaction skipped, block still builds

**Test 4:** `compute_header_hash()` produces correct double-SHA256 for known input, consistent with `bedrock-forge::CompactBlock::header_hash()`

### Files

- Modify: `crates/zcash-pool-server/src/forge.rs` -- refactor builder logic to be independently testable, add `#[cfg(test)] mod tests`

---

## Section 5: Noise Handshake Error Paths

### Problem

`handshake.rs` establishes every encrypted miner connection (163 lines, zero tests). Unhandled failures here mean miners silently can't connect.

### Design

**Test 1:** Client initiates with wrong server public key -> handshake error (not hang/panic)

**Test 2:** Server drops TCP mid-handshake -> client gets IO error

**Test 3:** Client sends garbage bytes instead of Noise handshake -> server's `accept()` returns error

**Test 4:** Successful handshake produces working `NoiseStream` (sanity check of the entry points)

### Files

- Modify: `crates/bedrock-noise/src/handshake.rs` -- add `#[cfg(test)] mod tests`

---

## Section 6: ShareProcessor Deeper Coverage

### Problem

`share.rs` has tests for timestamp validation and hash-to-difficulty, but no tests for `validate_share()` (the method that takes a `Channel` and checks job existence/staleness). This is the exact code path that runs on every share submission.

### Design

**Test 1:** Share for unknown job -> `PoolError::UnknownJob(999)`

**Test 2:** Share for stale job (after `clean_jobs=true`) -> `ShareResult::Rejected(RejectReason::StaleJob)`

**Test 3:** Share with wrong nonce_2 length -> `PoolError::InvalidMessage`

**Test 4:** Duplicate through Channel path -- first returns InvalidSolution, second returns Duplicate

**Test 5:** Timestamp boundary acceptance -- exactly `job_time - 60` and `job_time + 7200` both pass time check

### Files

- Modify: `crates/zcash-pool-server/src/share.rs` -- extend existing `#[cfg(test)] mod tests`

---

## Section 7: Benchmarks

### Problem

Only 1 benchmark exists in the entire workspace. For mainnet, we need to know critical path performance.

### Design

**Benchmark 1: ShareProcessor throughput.** `validate_share_with_job()` calls/sec with dummy solutions. Measures overhead before Equihash becomes bottleneck.

**Benchmark 2: PayoutTracker contention.** `record_share()` throughput with 1/10/100 concurrent writers.

**Benchmark 3: CompactSize codec throughput.** Encode/decode cycles/sec across full u64 range.

**Benchmark 4: Noise encrypt/decrypt roundtrip.** `write_message()` + `read_message()` latency for 100-byte and 10KB messages.

All use `criterion`.

### Files

- Create: `crates/zcash-pool-server/benches/share_bench.rs`
- Create: `crates/zcash-pool-server/benches/payout_bench.rs`
- Create: `crates/zcash-pool-common/benches/compact_size_bench.rs`
- Create: `crates/bedrock-noise/benches/transport_bench.rs`
- Modify: Cargo.toml files to add `criterion` dev-dependency and `[[bench]]` entries

---

## Section 8: Upstream Test Ports

### Problem

Bitcoin Core, rust-bitcoin, and SRI Stratum V2 have battle-tested test patterns we should adopt rather than reinvent.

### From Bitcoin Core BIP 152 (`blockencodings_tests.cpp`)

Port these 4 test cases to `bedrock-forge`:

1. **SimpleRoundTripTest** -- Compact block construction, short ID matching from mempool, reconstruction with missing tx. Validates the full sender -> receiver flow.
2. **EmptyBlockRoundTripTest** -- Coinbase-only compact block encoding roundtrip.
3. **TransactionsRequestSerializationTest** -- Differential index encoding roundtrip for `GetBlockTxn`.
4. **TransactionsRequestDeserializationOverflowTest** -- Index overflow edge case (security-relevant).

### From rust-bitcoin BIP 152 (`p2p/src/bip152.rs`)

5. **Short ID SipHash key derivation** -- Verify our `ShortId::compute()` produces the same result as `SHA256(header || nonce)` split into SipHash k0/k1. This is the BIP 152 interop test.
6. **Real block test vector** -- Create a Zcash-specific test vector from a real mainnet block (serialize to compact block, verify byte-exact output).

### From SRI Stratum V2

7. **Vardiff timing simulation pattern** -- Adopt SRI's `simulate_shares_and_wait()` helper to replace our fragile `thread::sleep`-based vardiff tests with deterministic timing.
8. **Double-roundtrip fuzz pattern** -- Update our existing fuzz targets to use `parse -> serialize -> re-parse -> re-serialize -> compare bytes` (SRI's `test_roundtrip!` macro pattern).

### From Noise Protocol Spec

9. **Noise NK test vectors** -- Validate `bedrock-noise` handshake against cacophony's published test vectors for Noise_NK_25519_ChaChaPoly_BLAKE2s (or whichever variant we use). Verify byte-identical handshake messages for known keys/nonces.

### Files

- Create: `crates/bedrock-forge/tests/bip152_compat.rs` -- tests 1-6
- Modify: `crates/zcash-equihash-validator/tests/vardiff_proptest.rs` -- adopt timing simulation (test 7)
- Modify: `crates/zcash-mining-protocol/fuzz/` -- adopt double-roundtrip pattern (test 8)
- Create: `crates/bedrock-noise/tests/noise_test_vectors.rs` -- test 9

### Sources

- Bitcoin Core: `src/test/blockencodings_tests.cpp` (8 test cases)
- rust-bitcoin: `p2p/src/bip152.rs` (4 test cases + test vector)
- SRI: `sv2/channels-sv2/src/vardiff/test/classic.rs` (12 vardiff tests)
- SRI: `fuzz/fuzz_targets/common.rs` (`test_roundtrip!` macro)
- Noise Protocol: `github.com/noiseprotocol/noise_wiki/wiki/Test-vectors`

---

## Execution Order

Sections have dependencies:

```
Section 1 (Mock Zebra) ──> Section 2 (E2E test) ──> Section 4 (ForgeRelay, uses templates)
                       ──> Section 8 (Upstream ports, some use mock)

Section 3 (Concurrent stress) -- independent
Section 5 (Noise handshake)   -- independent
Section 6 (ShareProcessor)    -- independent
Section 7 (Benchmarks)        -- independent, do last
```

Recommended order: 1 -> 6 -> 5 -> 3 -> 2 -> 4 -> 8 -> 7

Start with the mock (unlocks everything), then independent unit tests (6, 5, 3) in parallel, then integration (2, 4), then upstream ports (8), benchmarks last.
