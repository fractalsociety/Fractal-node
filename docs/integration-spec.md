# Integration Spec — fractalchain2 Proof-Ingestion System

> **Audience:** an engineer/agent wiring a client, relayer, SDK, or node component against the **finished** proof-ingestion blockchain. This documents what actually exists and how to call it — exact types, function signatures, RPC methods, config knobs, and known gaps. All anchors are `file_path:line`.
>
> **Status snapshot (2026-06-21):** all 8 PRD workstreams shipped. Two hardening items remain open (#25 fuzz CI hookup, #27 DA-sampling policy centralization) and one apply path is **stubbed** — all flagged in §12.
>
> Sibling docs: vision [`proof-ingestion-decoupling-prd.md`](./proof-ingestion-decoupling-prd.md), status [`master-prd.md`](./master-prd.md), task list [`proof-ingestion-tasks.json`](./proof-ingestion-tasks.json).

---

## 1. Crate map

| Crate | Role |
|-------|------|
| `fractal-consensus` (`crates/consensus`) | Block/payload contract, payload roots, proof verification, DA sampling receipts, `ValidityProofSystem`. **Primary integration surface.** |
| `fractal-core` (`crates/core`) | `Transaction`, `NativeCall`, `OwnedObjectCertificate`, `OwnedObjectId`, `TxExecutionScope`. |
| `fractal-shard` (`crates/shard`) | `ZoneProofFinalUpdateV1`, zone public-input verification, cross-zone messages, execution-zone registry. |
| `fractal-masterchain` (`crates/masterchain`) | `MasterchainBlockV1`, forced-inclusion ledger + queue root. |
| `fractal-mempool` (`crates/mempool`) | `ProofPool`, `CertificatePool`, tx mempool. |
| `fractal-node` (`crates/node`) | Node process: block production, apply modes, RPC handlers, config parsing. |
| `fractal-rpc` (`crates/rpc`) | JSON-RPC method registry (`fractal_*`). |
| `fractal-benchmarks` (`crates/benchmarks`) | `baseline` + `proof_ingestion` binaries, `BaselineBenchReport` schema. |

## 2. Runtime configuration

| Knob | Where | Values / Effect |
|------|-------|-----------------|
| `FRACTAL_BLOCK_PAYLOAD_MODE` | `crates/node/src/lib.rs:145` (`BlockPayloadMode::from_env`) | `""`/`legacy` → **Legacy** (default); `proof_ingestion`/`proof`/`proof-ingestion` → **ProofIngestion**; `mixed` → **Mixed**. Selects block-proposal payload. |
| `FRACTAL_PROOF_REQUIRED_SETTLEMENT` | `crates/node/src/lib.rs:397` | `1`/`true`/`yes`/`on`/`proof`/`required` → `chain_config.proof_required_settlement = true`. |
| `FRACTAL_PROOF_REQUIRED_NATIVE` / `_EVM` | `crates/node/src/lib.rs:444` | Set individual bits in `proofs_required_for_settlement`. |
| `FRACTAL_NETWORK` / `FRACTAL_ENV` | `crates/consensus/src/lib.rs` (`dev_digest_allowed_for_runtime`) | Production-like values (`mainnet`/`production`/`prod`/`release`/`testnet`) → **hard-reject DevDigest** even if feature on. |
| Cargo feature `dev-digest` | `crates/consensus/Cargo.toml` (`dev-digest = []`) | Off by default. Gates `ValidityProofSystem::DevDigest`. Without it, `DevDigest` cannot be (de)serialized. |

## 3. Core data model

### 3.1 Block payload contract — `crates/consensus/src/payload.rs`

```rust
// payload.rs:26  — the WIRE/SUBMISSION proof-update type (what you submit & what blocks carry)
pub struct ZoneProofUpdateV1 {
    pub zone_id: u64,
    pub height: u64,
    pub parent_root: Hash256,
    pub new_root: Hash256,
    pub tx_root: Hash256,
    pub da_root: Hash256,
    pub message_root: Hash256,
    pub forced_inclusion_root: Hash256,
    pub circuit_version: CircuitVersion,
    pub feature_set: ExecutionFeatureSetV1,
    pub proof_digest: Hash256,
}

// payload.rs:41
pub struct OwnedObjectCertificateBatchV1 {
    pub certificates: Vec<OwnedObjectCertificate>,
}

// payload.rs:46
pub enum BlockPayloadItem {
    Transaction { transaction: Transaction, eth_signed_raw: Option<Vec<u8>> },
    ProofUpdate(ZoneProofUpdateV1),
    CertificateBatch(OwnedObjectCertificateBatchV1),
}

// payload.rs:56
pub enum BlockPayload {
    FullTransactions { transactions: Vec<Transaction>, eth_signed_raw: Vec<Option<Vec<u8>>> },
    ProofUpdates(Vec<ZoneProofUpdateV1>),
    CertificateBatches(Vec<OwnedObjectCertificateBatchV1>),
    Mixed(Vec<BlockPayloadItem>),
}

// payload.rs:67  — runtime tag
pub enum BlockPayloadKind { FullTransactions, ProofUpdates, CertificateBatches, Mixed }
impl BlockPayload { pub fn kind(&self) -> BlockPayloadKind; pub fn payload_root(&self) -> Result<Hash256, std::io::Error>; }
```

> **Do not confuse** `ZoneProofUpdateV1` (consensus; 11-field wire type, used by `ProofPool` + `BlockPayload::ProofUpdates`) with `ZoneProofFinalUpdateV1` (shard; the fully-resolved final update carrying all public inputs + `proof_digest`, used by verification — see §3.3). They are distinct types.

### 3.2 Owned objects — `crates/core/src/tx.rs`

```rust
// tx.rs:113
pub enum OwnedObjectId {
    AccountNonce(Address), Agent(u64), Receipt(Hash256),
    WalletTaskReceipt(Hash256), ProofCommitment(Hash256),
}
// tx.rs:122
pub enum TxExecutionScope {
    Consensus,
    Mixed { owner: Address, owned_objects: Vec<OwnedObjectId> },
    Owned { owner: Address, objects: Vec<OwnedObjectId> },
}
// tx.rs:244
pub struct OwnedObjectCertificate {
    pub tx_hash: Hash256,
    pub owner: Address,
    pub signer_nonce: u64,
    pub object_versions: Vec<OwnedObjectVersion>,
    pub signer_indices: Vec<u32>,
    pub validator_signatures: Vec<OwnedObjectValidatorSignature>,
}
// tx.rs:256
pub struct OwnedObjectConflictingCertificateEvidence {
    pub certificate_a: OwnedObjectCertificate,
    pub certificate_b: OwnedObjectCertificate,
}
```
Native-call anchors on the tx itself: `NativeCall::ProofCommitmentV1` (`tx.rs:83`), `WalletTaskReceiptAnchorV1` (`tx.rs:77`).

### 3.3 Zone final update + public inputs — `crates/shard/src/lib.rs`

```rust
// shard/src/lib.rs:452  — resolved final update; carries ALL public inputs bound by verification
pub struct ZoneProofFinalUpdateV1 {
    pub zone_id: ZoneId, pub zone_block_height: u64, pub zone_block_hash: Hash256,
    pub state_root: Hash256, pub message_root: Hash256, pub tx_root: Hash256,
    pub da_root: Hash256, pub da_namespace: [u8; 8], pub forced_inclusion_root: Hash256,
    pub timestamp_ms: u64, pub circuit_version: CircuitVersion,
    pub coverage_manifest_digest: Hash256, pub covered_features: ExecutionFeatureSetV1,
    pub feature_set: ExecutionFeatureSetV1, pub public_input_digest: Hash256,
    pub source_message_root: Hash256, pub required_forced_inclusion_root: Hash256,
    pub da_available: bool, pub proof_digest: Hash256, pub prover: [u8; 20],
}
```
The public-input digest binds: `zone_id, zone_block_height, zone_block_hash, state_root, message_root, tx_root, da_root, da_namespace, forced_inclusion_root, timestamp_ms, circuit_version, coverage_manifest_digest, feature_set, source_message_root, required_forced_inclusion_root`.

### 3.4 DA sampling — `crates/consensus/src/lib.rs`

```rust
// consensus/src/lib.rs:248
pub struct DaSamplingReceiptSample {
    pub index: u32, pub is_parity: bool,
    pub commitment: Hash256, pub merkle_path: Vec<Hash256>,
}
// consensus/src/lib.rs:256
pub struct DaSamplingReceipt {
    pub namespace: DaNamespace, pub da_root: Hash256, pub share_count: u32,
    pub seed: u64, pub sample_count: u32, pub samples: Vec<DaSamplingReceiptSample>,
}
```

### 3.5 Forced inclusion + cross-zone — `crates/masterchain/src/ledger.rs`, `crates/shard/src/lib.rs`

```rust
// masterchain/src/ledger.rs:69
pub struct ForcedInclusionRequestV1 {
    pub zone_id: ZoneId, pub requester: [u8; 20], pub request_id: Hash256,
    pub tx_hash: Hash256, pub payload: Vec<u8>,
    pub submitted_at_masterchain_height: u64, pub deadline_masterchain_height: u64,
}
// masterchain/src/ledger.rs:80
pub struct ForcedInclusionEventV1 {
    pub version: u8, pub request: ForcedInclusionRequestV1,
    pub included_at_masterchain_height: u64, pub sequencer_late_by_blocks: u64,
}
// shard/src/lib.rs:527
pub struct AsyncCrossZoneMessageV1 {
    pub from_zone: ZoneId, pub to_zone: ZoneId, pub nonce: u64,
    pub payload_hash: Hash256, pub payload: Vec<u8>,
}
```

## 4. Root commitments (all return `Hash256`)

```rust
// consensus/src/payload.rs
pub fn proof_updates_root(updates: &[ZoneProofUpdateV1]) -> Result<Hash256, std::io::Error>;            // :168
pub fn proof_update_leaf_hash(update: &ZoneProofUpdateV1) -> Result<Hash256, std::io::Error>;           // :179
pub fn certificate_batches_root(batches: &[OwnedObjectCertificateBatchV1]) -> Result<Hash256, std::io::Error>; // :214
pub fn certificate_batch_root(batch: &OwnedObjectCertificateBatchV1) -> Result<Hash256, std::io::Error>;      // :227
pub fn certificate_batch_conflicts(batch: &OwnedObjectCertificateBatchV1) -> bool;                       // :196
impl BlockPayload { pub fn payload_root(&self) -> Result<Hash256, std::io::Error>; }                     // :98

// shard/src/lib.rs
pub fn zone_proof_public_input_digest(update: &ZoneProofFinalUpdateV1) -> Hash256;                       // :1041
pub fn cross_zone_message_root(messages: &[AsyncCrossZoneMessageV1]) -> Hash256;                         // :1281

// masterchain/src/ledger.rs
pub fn forced_inclusion_queue_root(requests: &[ForcedInclusionRequestV1]) -> Hash256;                    // :1153
```
Domain separation constants: `PAYLOAD_ROOT_DOMAIN`, `PAYLOAD_LEAF_DOMAIN`, `PROOF_UPDATE_LEAF_DOMAIN`, `CERTIFICATE_BATCH_LEAF_DOMAIN`, `CERTIFICATE_BATCH_ROOT_DOMAIN` (`payload.rs:14-18`). All roots are deterministic pure functions of ordered inputs — reuse these exact fns; **do not** recompute roots independently or consensus will diverge.

## 5. Proof verification — `crates/consensus/src/lib.rs`, `crates/shard/src/lib.rs`

```rust
// consensus/src/lib.rs:2004  — main entry: verifies all public inputs vs block header, then delegates
pub fn verify_block_validity_proof(block: &Block, proof: &BlockValidityProof) -> Result<(), ProofVerifyError>;

// shard/src/lib.rs:1197
fn verify_zone_update_public_inputs(update: &ZoneProofFinalUpdateV1) -> Result<(), ExecutionZoneError>;
```

`ProofVerifyError` (`consensus/src/lib.rs:101-144`) enumerates **every** public-input bound — verification fails closed on any mismatch: `ChainId`, `Height`, `BlockHash`, `Timestamp`, `StateRoot`, `ParentStateRoot`, `TxRoot`, `ReceiptRoot`, `NativeEventRoot`, `EvmLogRoot`, `DaRoot`, `ZoneNamespace`, `FeatureSet`, `CoverageManifest`, `CircuitCoverage`, `EmptyProof`, `Production(ProductionProofVerifyError)`, `BadDevDigest`, `DevDigestDisabled`, `DataAvailability`, `Io`.

```rust
// consensus/src/lib.rs:322  — borsh discriminants are fixed
#[borsh(use_discriminant = true)]
pub enum ValidityProofSystem {
    #[cfg(feature = "dev-digest")] DevDigest = 0,
    StwoPlonky2 = 1,
}
```
**DevDigest gating:** without the `dev-digest` feature, discriminant `0` cannot deserialize (so a default build rejects DevDigest at the wire). At runtime, `dev_digest_allowed_for_runtime(network, environment)` hard-rejects when `FRACTAL_NETWORK`/`FRACTAL_ENV` look production-like. DevDigest is for **local benchmarking only**.

## 6. RPC reference (`crates/rpc/src/module.rs`)

All are JSON-RPC `fractal_*` methods. Borsh payloads are hex-encoded strings.

### Proof submission & finality
| Method (line) | Params | Response |
|---|---|---|
| `fractal_submitProofUpdate` (:1791) | `{proofUpdate|proof_update_borsh: <hex>}`, optional `maxPriorityFee` | `network`, `proof_update_hash`, `zone_id`, `height`, `pending_proof_updates` |
| `fractal_submitProofHash` (:1832) | `proof_hash` (32-byte hex) | `network`, `transaction_hash`, `block_number`, `finalized` (legacy compat path) |
| `fractal_submitValidityProof` (:993) | borsh `BlockValidityProof` hex | `block_hash`, `finality_status:"proof"` |
| `fractal_getSettlementBlock` (:1196) | block hash | `block_hash`, `block_number`, `finality_status`, `proof_circuit_version`, `proof_coverage_manifest_digest`, `proof_covered_features`, `settlement_allowed`, `proof_required_settlement`. Errors `-32010` soft-final / `-32011` uncovered-circuit / `-32012` unavailable-da |
| `fractal_getProofFinalHeight` (:1149) | `zone_id` | `zone_id`, `proof_final_height: Option<hex>` |
| `fractal_getZoneUpdateFinality` (:1171) | `[zoneId, height]` | `zone_id`, `height`, `finality_status:"soft"|"proof"` |

```rust
// module.rs:696  — in-process trait check (for settlement gating)
fn settlement_finality_for_block_hash(&self, hash: &[u8; 32]) -> Result<(), String>;
// Ok = proof-final or proof not required; Err = "block not found" / "not proof-final" / "DA unavailable"
```

### Owned-object certificate fast path (E1)
| Method (line) | Params | Response |
|---|---|---|
| `fractal_ownedObjectPrecheck` (:1018) | `[rawTxHex, maxFeePerGas?]` | `tx_hash`, `owner`, `signer_nonce`, `object_versions`, `object_versions_borsh`, `sign_body_borsh`, `tx_gas`, fees |
| `fractal_countersignOwnedObjectTx` (:1048) | `[rawTxHex, maxFeePerGas?]` | `validator_index`, `signature_borsh`, `sign_body_borsh` |
| `fractal_aggregateOwnedObjectCertificate` (:1078) | `[rawTxHex, objectVersionsBorshHex, signatureBorshHexArray]` | `certificate_hash`, `certificate_borsh`, `signer_indices` |
| `fractal_getOwnedObjectFinality` (:1120) | borsh `OwnedObjectVersion` hex | `object_version_borsh`, `finality_status:"none"|"certificate"`, `certificate_hash?`, `certificate_borsh?` |

### Routing diagnostics (F2)
| Method (line) | Params | Response |
|---|---|---|
| `fractal_debugTxRouting` (:950) | raw tx hex | `source_shard`, `expected_shard`, `shard_count`, `route_key`, `accepted` |

### Example: submit a proof update
```bash
curl -s localhost:8545 -X POST -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"fractal_submitProofUpdate",
       "params":{"proofUpdate":"<borsh-hex-of-ZoneProofUpdateV1>"}}'
```

## 7. Pools (`crates/mempool`)

### ProofPool — `crates/mempool/src/proof_pool.rs`
```rust
ProofPool::new(conflict_policy: ProofPoolConflictPolicy /* Reject | RetainEvidence */) -> Self;  // :82
fn insert(&mut self, update: PooledProofUpdate) -> Result<(), ProofPoolError>;   // :117  keyed by (zone_id,height); conflict if same key != digest
fn get(&self, key: ProofUpdateKey) -> Option<&PooledProofUpdate>;               // :113
fn drain_ready(&mut self, max_updates: usize) -> Vec<ZoneProofUpdateV1>;        // :150  sorted by max_priority_fee desc, then key
fn remove(&mut self, key: ProofUpdateKey) -> Option<PooledProofUpdate>;          // :142
fn metrics(&self) -> ProofPoolMetrics;   // :100  {pending_total, inserted_total, evicted_total, drained_total, conflict_total, retained_conflicts}
fn conflicts(&self) -> &[ProofUpdateConflict];  // :108
```

### CertificatePool — `crates/mempool/src/certificate_pool.rs`
```rust
fn insert(&mut self, certificate: OwnedObjectCertificate,
          validator_pubkeys: &[BlsPublicKey], quorum_threshold: usize) -> Result<Hash256, CertificatePoolError>;  // :80
fn finality_for_object_version(&self, ov: &OwnedObjectVersion) -> Option<&CertificateFinalityRecord>;             // :63
fn accepted_certificates(&self) -> Vec<OwnedObjectCertificate>;   // :71
fn conflicts(&self) -> &[CertificateConflictRecord];              // :57
```
Certificate crypto on `OwnedObjectCertificate` (`crates/core/src/tx.rs`): `countersign(sign_body, validator_index, secret) -> Result<…>` (:286); `aggregate(tx, object_versions, sigs, quorum_threshold) -> Result<Self,_>` (:298); `verify(&self, validator_pubkeys, quorum_threshold) -> Result<(),_>` (:314).

## 8. Block production & apply (`crates/node/src/lib.rs`)

```rust
// :161
pub enum BlockApplyMode { ReplayFullTransactions, VerifyProofAndDa, HeaderOnlyAfterProofFinal }
// :1364
pub fn apply_synced_block_with_mode(&mut self, block: &Block, mode: BlockApplyMode) -> Result<(), SyncApplyError>;
// :2238  — builds the BlockPayload variant for the active mode
fn proposal_payload_for_mode(...) -> BlockPayload;   // Legacy→FullTransactions; ProofIngestion→ProofUpdates/CertificateBatches; Mixed→Mixed
// :2332  — producer tick (called ~every 500ms)
fn try_produce_one_tick(node: &NodeHandle) -> ProduceTickOutcome;  // Legacy drains mempool; ProofIngestion/Mixed also drain proof_pool(1024) + certificate_pool
```

> ⚠️ **Stub:** `BlockApplyVerifier::verify_proof_and_da` (`node/src/lib.rs:1450`) currently returns `Err(SyncApplyError::ProofIngestionApplyUnavailable)`. The `ReplayFullTransactions` and `HeaderOnlyAfterProofFinal` paths work; the **`VerifyProofAndDa` apply branch is not yet implemented** — do not wire followers to it expecting state advancement. (See §12.)

## 9. DA sampling

```rust
// consensus/src/lib.rs
pub fn build_da_sampling_receipt(sidecar: &DaSidecar, expected_root: Hash256,
        seed: u64, sample_count: u32) -> Result<DaSamplingReceipt, DaVerifyError>;       // :1431
pub fn verify_da_sampling_receipt(receipt: &DaSamplingReceipt, expected_root: Hash256,
        expected_namespace: DaNamespace, min_samples: u32) -> Result<(), DaVerifyError>; // :1474  rejects if sample_count < min_samples
```
> ⚠️ `min_samples` is a **caller-supplied parameter** today (e.g. literal `4` at `consensus/src/lib.rs:4223,4243,4597`), not a centralized policy. Task #27 will introduce a `DaSamplingPolicy`. Until then, agree on the value out-of-band or you and your peer may reject/accept inconsistently.

## 10. Benchmarks & report schema

Binaries: `baseline` (`crates/benchmarks/src/bin/baseline.rs`) and `proof_ingestion` (`crates/benchmarks/src/bin/proof_ingestion.rs`). **Identical CLI:**
```
--blocks <usize> (16)  --txs-per-block <usize> (64)  --chain-id <u64> (41)
--gas-limit <u64> (60_000_000)  --seed <u64> (41)  --output <path?>
```
Both emit the shared `BaselineBenchReport` JSON (`crates/benchmarks/src/lib.rs`): `schema_version=1`, `run_kind` (`"baseline"` / `"proof_ingestion"`), `config`, `scenarios: Vec<BaselineScenarioReport>`. Per-scenario fields include `name`, `kind`, `blocks`, `submitted_txs`, `committed_txs`, `submitted_tx_per_second`, `committed_tx_per_second`, `block_p50_latency_nanos`, `block_p95_latency_nanos`, `cpu_nanos`, `peak_working_set_bytes`, `total_block_bytes`/`avg_block_bytes`, `total_da_bytes`/`avg_da_bytes`, `replay_time_nanos`/`replay_tx_per_second`, `accepted_proof_updates`, `accepted_certificate_updates`.

Comparison report:
```bash
python3 scripts/compare-proof-ingestion-bench.py \
  --baseline baseline.json --proof proof_ingestion.json \
  [--json out.json] [--markdown out.md] [--html out.html] [--title "..."]
# classifies bottleneck per scenario: consensus | proof verification | DA sampling | network | storage
```

## 11. End-to-end usage walkthroughs

**A. Submit a zone proof update (proof-ingestion lane).** Build a `ZoneProofUpdateV1` (11 fields, §3.1) whose `proof_digest` matches a `ZoneProofFinalUpdateV1` your prover produced; borsh-encode; POST `fractal_submitProofUpdate`. Poll `fractal_getZoneUpdateFinality(zoneId,height)` until `finality_status:"proof"`. The node routes it into `ProofPool`, includes it in a `ProofUpdates` block, and commits `proof_updates_root`.

**B. Finalize an owned-object tx off-consensus.** `fractal_ownedObjectPrecheck(rawTx)` → collect `2f+1` `fractal_countersignOwnedObjectTx(rawTx)` from validators → `fractal_aggregateOwnedObjectCertificate(rawTx, versions, sigs[])` → `fractal_getOwnedObjectFinality(version)` returns `certificate`. The certificate can later be carried in a `CertificateBatches` block via `certificate_batch_root`.

**C. Verify DA without full reconstruction.** `build_da_sampling_receipt(sidecar, da_root, seed, min_samples)` on the prover side; peers call `verify_da_sampling_receipt(receipt, da_root, namespace, min_samples)` — passes on the receipt alone (Merkle paths), no full blob.

**D. Benchmark + compare.** `cargo run --release --bin baseline -- --output b.json` then `... --bin proof_ingestion -- --output p.json` then the compare script → verdict per scenario.

## 12. Critical caveats & known gaps

1. **`VerifyProofAndDa` apply path is stubbed** (`node/src/lib.rs:1450` → `ProofIngestionApplyUnavailable`). Proof-covered blocks can be *produced and verified* via `verify_block_validity_proof`, but follower state-advance through that mode isn't wired. Use `ReplayFullTransactions` or `HeaderOnlyAfterProofFinal` until implemented.
2. **`min_samples` is not centralized** (task #27 open). Pass/derive it consistently; see §9.
3. **Fuzz targets exist but aren't in CI** (task #25 open): `fuzz/fuzz_targets/payload_roots.rs` etc. — run manually with `cargo +nightly fuzz`.
4. **Two zone-update types** — `ZoneProofUpdateV1` (consensus, wire) vs `ZoneProofFinalUpdateV1` (shard, verification). Don't mix them.
5. **DevDigest is dev-only** — feature-gated + runtime-gated; never enable in production. Production proof verification must use `StwoPlonky2` (and a concrete STWO verifier is future work — do not claim production STWO acceptance yet).
6. **`fractal_submitProofHash` is the legacy compat path** (proof hash → tx mempool). New integrations should use `fractal_submitProofUpdate`.

## 13. Quick links

- Payload contract + roots: `crates/consensus/src/payload.rs`
- Proof verification + DA receipts: `crates/consensus/src/lib.rs`
- Owned-object certs: `crates/core/src/tx.rs`
- Zone final updates + message/forced-inclusion roots: `crates/shard/src/lib.rs`
- Forced-inclusion ledger + queue root: `crates/masterchain/src/ledger.rs`
- Pools: `crates/mempool/src/proof_pool.rs`, `crates/mempool/src/certificate_pool.rs`
- RPC handlers: `crates/rpc/src/module.rs`
- Node production/apply/config: `crates/node/src/lib.rs`
- Benchmarks: `crates/benchmarks/src/bin/{baseline,proof_ingestion}.rs`, `scripts/compare-proof-ingestion-bench.py`
