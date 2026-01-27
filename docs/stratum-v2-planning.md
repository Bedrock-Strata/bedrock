# Implementing Stratum V2 for Zcash with decentralized block templates

Zcash can implement Stratum V2's decentralized template construction by adapting the existing SRI codebase, but significant modifications are required for Equihash's **1,344-byte solution format**, **32-byte nonce**, and fundamentally different header structure. The most viable path is forking SRI's Rust protocol libraries and building a Zcash-specific Template Provider that interfaces with Zebra (the recommended node). A minimum viable implementation should prioritize the Template Distribution Protocol first, enabling the core innovation—miner-selected transactions—before full Job Declaration support.

## How SV2 enables decentralized block template construction

Stratum V2 fundamentally restructures mining pool architecture through **three sub-protocols** that together shift transaction selection power from pools to miners. The **Mining Protocol** handles job distribution and share submission with binary encoding (reducing bandwidth **~70%** versus SV1's JSON). The **Template Distribution Protocol** replaces Bitcoin's `getblocktemplate` RPC with a push-based system where miners receive templates directly from their own full node. The critical innovation is the **Job Declaration Protocol**, which allows miners to propose custom block templates containing their chosen transaction sets.

The decentralization mechanism works as follows: miners running a Job Declarator Client (JDC) request an authorization token from the pool's Job Declarator Server (JDS) via `AllocateMiningJobToken`. The JDC then constructs a block template using its local node, declares this custom job to the pool via `DeclareMiningJob` (containing all transaction IDs), and begins mining. The pool verifies transaction validity—requesting unknown transactions via `ProvideMissingTransactions`—but cannot dictate which transactions appear. Miners are paid proportionally to their template's fee revenue, creating economic incentives for optimal transaction selection.

SV1's fundamental limitation is that pools construct all block templates and miners receive only the data needed to hash. Pools control **100%** of transaction selection, enabling potential censorship with no miner visibility. SV2's separation of concerns eliminates this: the Template Distribution Protocol sources templates from miner-operated nodes, while Job Declaration handles pool coordination. Miners can also broadcast found blocks directly via `SubmitSolution`, preventing post-discovery suppression.

## SRI reference implementation is production-ready at the protocol layer

The Stratum Reference Implementation (SRI), maintained at `github.com/stratum-mining/stratum`, is written in **Rust 1.75.0+** with a modular crate structure. The codebase was recently split: protocol libraries in `stratum` (production-ready, v1.7.0) and applications in `sv2-apps` (alpha stage, v0.1.0). DMND Pool operates SRI on Bitcoin mainnet with full Job Declaration support.

The key protocol crates and their functions are:

| Crate | Version | Purpose |
|-------|---------|------------|
| `mining_sv2` | 7.0.0 | Mining Protocol messages |
| `job_declaration_sv2` | 6.0.0 | Job Declaration messages |
| `template_distribution_sv2` | 4.0.1 | Template Distribution messages |
| `noise_sv2` | 1.4.1 | Noise Protocol encryption |
| `binary_sv2` | 5.0.1 | Binary serialization |
| `channels_sv2` | 3.0.0 | Channel management and share validation |

Bitcoin-specific logic concentrates in three areas. **Share validation** in `channels_sv2` uses the `rust-bitcoin` crate for target calculations and difficulty comparison—this requires complete replacement with Equihash verification. **Mining Protocol messages** in `sv2/subprotocols/mining/` define `NewMiningJob`, `SetNewPrevHash`, and `SubmitSharesStandard` with Bitcoin's 80-byte header assumptions. **Template Distribution** in `sv2/subprotocols/template-distribution/` handles BIP141 SegWit coinbase stripping, irrelevant to Zcash.

The modular architecture provides clean abstraction points. The binary encoding layer (`binary-sv2`) is protocol-agnostic and fully reusable. The Noise encryption layer (`noise-sv2`) uses secp256k1 and ChaChaPoly—chain-agnostic and directly reusable. Message framing (`framing-sv2`) handles the 6-byte header format (extension_type, msg_type, length) independent of payload content.

## Equihash requires major protocol modifications

Zcash's Equihash (200,9) differs fundamentally from Bitcoin's SHA256d in ways that impact every layer of the mining protocol. Equihash is **memory-hard**, requiring **144-178 MiB** working memory versus effectively zero for SHA256. Solutions contain **512 indices** (21 bits each) encoding the collision structure, yielding a fixed **1,344-byte solution** versus Bitcoin's 4-byte nonce. Verification requires **512 Blake2b hash operations** compared to 2 SHA256d operations.

The header structure difference is substantial:

**Bitcoin (80 bytes):** `version(4) | prevHash(32) | merkleRoot(32) | time(4) | bits(4) | nonce(4)`

**Zcash (1,487 bytes total):** `version(4) | prevHash(32) | merkleRoot(32) | hashBlockCommitments(32) | time(4) | bits(4) | nonce(32) | solution(1344)`

The **32-byte nonce** fundamentally changes extranonce handling. Bitcoin embeds extranonce in the coinbase transaction; Zcash splits the header nonce as `NONCE_1` (pool-assigned prefix) + `NONCE_2` (miner suffix), with `len(NONCE_1) + len(NONCE_2) = 32`. This eliminates the need for Extended Channels' coinbase manipulation—Standard Channels suffice for header-only mining.

Share submission bandwidth increases dramatically. Bitcoin SV2 share submissions are **~50 bytes**; Equihash submissions with the 1,344-byte solution reach **~1,400 bytes**—a **2,700% increase**. New message types are required:

```
NewEquihashJob:
  channel_id(U32) | job_id(U32) | future_job(bool) | version(U32) |
  prev_hash(B032) | merkle_root(B032) | reserved(B032) | nonce_1(B0_255) |
  time(U32) | bits(U32) | clean_jobs(bool)

SubmitEquihashShare:
  channel_id(U32) | sequence_number(U32) | job_id(U32) | nonce_2(B0_255) |
  time(U32) | solution(B1344)
```

Pool-side validation complexity increases significantly. Each share requires running the full Equihash verification algorithm: confirming all 512 hash values XOR to zero, indices are unique, and the Wagner tree structure is valid. This demands **~144 MB memory per verification thread** and takes **>150 µs** versus microseconds for SHA256d.

## Template Provider integration with Zcash nodes

Both zcashd and Zebra expose `getblocktemplate` following BIP 22 with Zcash-specific extensions. Zebra is **recommended for new deployments**—zcashd is being deprecated in 2025, while Zebra is actively developed with production mining support since v2.0.0.

The `getblocktemplate` response includes Zcash-specific fields absent in Bitcoin:

```json
{
  "previousblockhash": "...",
  "defaultroots": {
    "merkleroot": "...",
    "chainhistoryroot": "...",
    "authdataroot": "...",
    "blockcommitmentshash": "..."
  },
  "transactions": [{"data": "...", "hash": "...", "depends": [...], "fee": n}],
  "coinbasetxn": {...},
  "target": "...",
  "height": n

}
```

The `hashBlockCommitments` field (32 bytes) replaces Bitcoin's reserved field and contains `BLAKE2b-256(historyTreeRoot || authDataRoot || terminator)` post-NU5. Modifying the transaction set requires recomputing `merkleroot`, `authdataroot`, and `blockcommitmentshash`—the Template Provider must implement these calculations.

Transaction selection follows **ZIP 317's Proportional Transfer Fee Mechanism**. Logical actions are calculated as: transparent contribution = `max(ceil(tx_in_size/150), ceil(tx_out_size/34))`, Sapling contribution = `max(spends, outputs)`, Orchard contribution = `nActions`. Conventional fee = `5000 × max(2, logical_actions)` zatoshis. The block template algorithm weights transactions by `min(fee/conventional_fee, 4)`.

A Template Provider component must implement:
- **RPC interface** to Zebra: `getblocktemplate`, `submitblock`, `getrawmempool`, `getbestblockhash`
- **Template caching** with longpoll support for tip changes
- **Header assembly** constructing the 140-byte Equihash input from template fields
- **Commitment recalculation** when transactions are modified
- **Funding stream preservation** in coinbase outputs (required post-NU5)

Zebra's RPC runs on port 8232 (mainnet) with cookie authentication by default. Configuration requires only `miner_address` (transparent p2pkh/p2sh; Zebra doesn't yet support shielded coinbase) and `listen_addr`.

## Job Negotiation enables miner-selected transactions

The Job Declaration Protocol's message flow for miner-proposed templates works as follows:

1. **Token allocation**: JDC sends `AllocateMiningJobToken` → JDS responds with token + `coinbase_output_constraints` specifying required pool payout outputs and maximum additional coinbase size
2. **Template generation**: JDC constructs template from local node via Template Distribution Protocol
3. **Declaration**: JDC sends `DeclareMiningJob` containing token + all transaction IDs (txids)
4. **Validation**: JDS may request `IdentifyTransactions` for txid→index mapping, then `ProvideMissingTransactions` for unknown transactions
5. **Acknowledgment**: JDS returns `DeclareMiningJob.Success` or rejection reason
6. **Mining**: JDC issues `SetCustomMiningJob` to Mining Protocol endpoint; mining begins

**Optimistic mining** allows JDC to start hashing immediately after sending `DeclareMiningJob`, caching shares until acknowledgment. Rejected jobs discard cached shares; accepted jobs credit them normally.

Two declaration modes exist: **Full-Template Mode** where JDC runs its own node and constructs complete templates (maximum decentralization), and **TX-Hash-List Mode** where JDC proposes transaction sets but JDS constructs the actual template (partial decentralization, no local node required).

For Zcash, the infrastructure requirements include:
- **Miner-side**: Zebra node (for templates), JDC modified for Equihash, mining hardware
- **Pool-side**: JDS maintaining synchronized mempool, Equihash solution validator, coinbase output specification compliant with funding streams
- **Network**: Miners need good connectivity to propagate transactions to their mempool

The pool validates proposed templates by checking: all transactions exist and are valid, coinbase includes required outputs, template produces valid block commitments. Pools cannot dictate transaction inclusion—only reject invalid templates.

## Current Zcash mining uses SV1 with dangerous centralization

Zcash mining faces a **critical centralization problem**. In September 2023, ViaBTC controlled **53%+ of network hashrate**, triggering Coinbase to increase confirmation requirements from 40 minutes to 2.5 hours (110 blocks) and move ZEC to limit-only trading.

Major active pools include ViaBTC (dominant), FlyPool (Ethermine), 2Miners, Nanopool, and Suprnova. All use **ZIP 301 Stratum Protocol**—a Zcash adaptation of Stratum V1 with JSON-RPC 1.0 over TCP. ZIP 301 specifies the 32-byte nonce split, 256-bit target representation, and Equihash solution submission format.

ASIC miners dominate since 2018: Antminer Z15 (~420 KSol/s), Z11, and Innosilicon A9++ devices. GPU mining is economically unviable. The community **voted against** ASIC-resistant algorithm changes in 2018, prioritizing security over decentralization at the hardware level.

No prior work exists on Stratum V2 for Zcash. A 2022 forum discussion raised SV2 and P2Pool concepts but produced no implementation. The Electric Coin Company proposed a "Trailing Finality Layer" to address centralization risk at the consensus level rather than the mining protocol level.

The transition path for existing infrastructure would leverage SRI's **Translation Proxy** pattern: pools deploy SV2 internally while proxies translate for legacy SV1 ASIC firmware. This allows incremental adoption without requiring immediate firmware updates.

## Implementation roadmap for minimum viable decentralized templates

Building SV2 for Zcash should follow a phased approach prioritizing the decentralization features:

**Phase 1: Zcash Template Provider (4-8 weeks)**
- Fork `template_distribution_sv2` crate
- Implement Zebra RPC integration (`getblocktemplate`, `submitblock`)
- Build 140-byte header assembly with `hashBlockCommitments` calculation
- Create push-based template notification (longpoll-triggered)
- Test template generation accuracy against zcashd reference

**Phase 2: Equihash Mining Protocol (6-10 weeks)**
- Define new message types: `NewEquihashJob`, `SubmitEquihashShare`
- Implement Equihash solution validation library (port Tromp solver verification)
- Modify `channels_sv2` for Equihash share processing
- Handle 32-byte nonce space partitioning (NONCE_1/NONCE_2)
- Build vardiff algorithm accounting for ~15-30 second solve times

**Phase 3: Basic Pool Server (4-6 weeks)**
- Fork `sv2-apps/pool-apps/pool/`
- Replace Bitcoin block handling with Zcash primitives
- Integrate Equihash validator with ~144 MB per-thread memory allocation
- Implement coinbase construction respecting funding stream outputs
- Basic payout tracking (PPS initially)

**Phase 4: Job Declaration Support (6-8 weeks)**
- Implement JDS with Zebra mempool synchronization
- Modify `job_declaration_sv2` messages for Equihash parameters
- Build JDC component with template declaration flow
- Transaction validation and unknown-transaction retrieval
- Test miner-proposed template acceptance and rejection

**Phase 5: Translation Proxy (3-4 weeks)**
- Fork `sv2-apps/miner-apps/translator/`
- Map ZIP 301 SV1 messages to SV2 Equihash messages
- Handle solution format translation
- Support existing ASIC miners without firmware changes

**Dependencies between components:**
- Phase 2 (Mining Protocol) requires Phase 1 (Template Provider) for job content
- Phase 3 (Pool) requires Phase 2 for share validation
- Phase 4 (Job Declaration) requires Phase 3 as JDS runs pool-side
- Phase 5 (Translator) requires Phase 2 for message format definitions

**Minimum viable demonstration** of decentralized template construction requires Phases 1, 2, 3, and 4—approximately **20-32 weeks** of development. Phase 5 enables production deployment with existing mining hardware.

The recommended starting point is **Phase 1** (Template Provider) as it has no SRI dependencies, establishes the Zcash↔SV2 interface, and validates Zebra integration assumptions. This component can be developed and tested against existing Zebra deployments immediately.
