# Proof-Ingestion Decoupling PRD

## Purpose

This PRD defines the `fractalchain2` experiment: convert FractalChain from a block-production path that executes and carries most transactions into a base-chain path that primarily ingests proofs, data-availability commitments, owned-object certificates, and cross-zone message roots.

The goal is to benchmark this design against the current `fractalchain` baseline and determine whether decoupling execution from block production improves transaction throughput and latency without reducing decentralization.

## Baseline

The current design already has the right primitives:

- HyperBFT shard block production.
- Data-availability sidecars and sampling hooks.
- Proof-finality status.
- Owned-object classification.
- Owned-object certificate wire types.
- Execution-zone and masterchain coordination sketches.

The remaining coupling is that normal block production still:

- drains transactions from the mempool;
- executes them during block construction;
- carries full transaction bodies in blocks;
- builds DA sidecars from the transaction list;
- requires followers to reconstruct DA payloads and replay transactions;
- submits proof hashes through the normal mempool.

## Target Architecture

Base-chain blocks should become settlement envelopes:

```text
Block
  header
  proof_updates[]
  certificate_batch_roots[]
  da_commitments[]
  cross_zone_message_roots[]
  forced_inclusion_queue_root
  optional_full_transactions[]   # compatibility lane only
```

Execution should move to independent lanes:

```text
Owned-object lane:
  user tx -> validator precheck signatures -> certificate -> object-local finality

Execution zone lane:
  zone txs -> local execution -> DA blob -> validity proof -> base-chain proof update

Shared-state lane:
  transfers, EVM, disputes, governance -> HyperBFT ordered execution
```

The base chain remains decentralized because validators still verify:

- quorum certificates;
- proof public inputs;
- validity proofs;
- DA commitments and sampling evidence;
- forced-inclusion constraints;
- slashable evidence for conflicting owned-object certificates.

## Non-Goals

- Do not remove the current full-transaction block path.
- Do not weaken proof verification into trusted sequencer assertions.
- Do not make cross-zone calls synchronous.
- Do not benchmark only singleton mode; BFT-7 must be included before judging the design.
- Do not claim production STWO proof acceptance until the concrete verifier is wired.

## Success Metrics

Compare `fractalchain2` against the original `fractalchain` baseline.

| Metric | Goal |
|---|---|
| Block production latency | Lower p50 and p95 than baseline under equivalent load |
| Owned-object finality latency | Sub-block or one network round-trip after quorum countersignature |
| Proof ingestion throughput | Higher accepted updates/sec than transaction execution blocks |
| Validator CPU per finalized state update | Lower than replay-based path |
| Block payload bytes | Lower for proof-covered workloads |
| DA sampling cost | Bounded independently of full transaction volume |
| Shared-state correctness | No regression versus baseline tests |
| Decentralization | Same validator quorum assumptions; no trusted single sequencer finality |

## Workstream A: Block Payload Refactor

### A1. Add block payload variants

Create explicit payload variants so proof-ingestion blocks can coexist with legacy full-transaction blocks.

Proposed type:

```rust
pub enum BlockPayload {
    FullTransactions {
        transactions: Vec<Transaction>,
        eth_signed_raw: Vec<Option<Vec<u8>>>,
    },
    ProofUpdates(Vec<ZoneProofUpdateV1>),
    CertificateBatches(Vec<OwnedObjectCertificateBatchV1>),
    Mixed(Vec<BlockPayloadItem>),
}
```

Acceptance criteria:

- Legacy full-transaction blocks still encode/decode.
- `tx_root` remains deterministic for legacy blocks.
- New payload roots are committed in the header or a versioned payload root.
- RPC can report the payload type for a block.

### A2. Add proof-update payload root

Add a deterministic Merkle/root commitment for proof updates.

Acceptance criteria:

- Root is stable across nodes.
- Root binds zone id, parent root, new root, DA root, message root, circuit version, and proof digest.
- Header hash changes if any proof update field changes.

### A3. Keep compatibility mode

Add an environment flag:

```text
FRACTAL_BLOCK_PAYLOAD_MODE=legacy|proof_ingestion|mixed
```

Acceptance criteria:

- `legacy` matches current behavior.
- `proof_ingestion` produces proof/certificate payloads when available.
- `mixed` can include shared-state transactions plus proof updates.

## Workstream B: Proof Pool and Proof Ingestion

### B1. Add a proof pool separate from the mempool

Proof updates should not enter the normal transaction mempool.

Acceptance criteria:

- `ProofPool` stores pending proof updates by zone and height.
- Conflicting updates for the same `(zone_id, height)` are rejected or retained as evidence.
- Proof pool has metrics independent of transaction mempool metrics.

### B2. Convert `fractal_submitProofHash` into proof-update submission

The current proof-hash path inserts `ProofCommitmentV1` into the mempool. Add a direct proof-update path.

Acceptance criteria:

- New RPC: `fractal_submitProofUpdate`.
- Existing `fractal_submitProofHash` remains as a compatibility wrapper or legacy method.
- Direct proof submission can be included in a proof-ingestion block without consuming transaction gas.

### B3. Verify proof public inputs against block payload

Proof updates must bind to the committed payload root and DA commitment.

Acceptance criteria:

- Verification fails if `parent_state_root`, `state_root`, `tx_root`, `da_root`, `message_root`, or circuit metadata mismatch.
- Unsupported production proof systems fail closed.
- Dev digest proof mode remains available only for local benchmarking.

## Workstream C: Replay-Free Apply Path

### C1. Split block application into verification modes

Current follower application reconstructs DA and replays transactions. Add a proof-covered apply mode.

Proposed modes:

```rust
pub enum BlockApplyMode {
    ReplayFullTransactions,
    VerifyProofAndDa,
    HeaderOnlyAfterProofFinal,
}
```

Acceptance criteria:

- Legacy blocks use replay mode.
- Proof-ingestion blocks use proof and DA mode.
- Replay mode remains available for archival/full-verifier nodes.
- Consensus validators can skip transaction replay for proof-covered zone updates.

### C2. Update state roots from verified proof updates

For proof-ingestion blocks, state transition comes from accepted proof updates, not local transaction execution.

Acceptance criteria:

- Zone state root advances only if proof verification succeeds.
- Global/masterchain root derives deterministically from accepted zone roots.
- Rejected proof updates do not mutate state.

### C3. Add proof-finality indexing

Extend finality tracking from block-level proof finality to zone/update proof finality.

Acceptance criteria:

- RPC exposes soft/proof finality for proof updates.
- RPC can query latest proof-final height per zone.
- Proof-finality survives restart once persistence is enabled.

## Workstream D: DA Decoupling

### D1. Replace transaction-list DA with zone blob DA

Do not build DA sidecars only from `borsh(transactions)`.

Acceptance criteria:

- Zone blob DA can be submitted independently of base-chain tx list.
- Header commits to namespace, DA root, byte count, share count, and sampling parameters.
- Proof finality requires DA verification.

### D2. Add DA certificate or sampling receipt

Validators should verify availability without reconstructing every transaction payload.

Acceptance criteria:

- Sampling receipt binds sampled indexes and commitments.
- DA verification can pass without full payload reconstruction.
- Full reconstruction remains available for archive/debug nodes.

### D3. Split DA fee accounting from execution gas

DA costs should not be coupled to EVM/native execution gas.

Acceptance criteria:

- DA fee metrics are reported separately.
- Proof updates pay proof verification cost separately.
- Shared-state transactions continue using execution gas.

## Workstream E: Owned-Object Certificate Fast Path

### E1. Add certificate request and countersign RPC

Implement the missing network/RPC path around existing certificate types.

Acceptance criteria:

- User/client can request owned-object precheck data.
- Validators countersign eligible owned transactions.
- Client can aggregate `2f + 1` signatures into a certificate.

### E2. Add certificate pool and direct finality

Owned certificates should become final without waiting for global transaction ordering.

Acceptance criteria:

- `CertificatePool` accepts valid certificates.
- Conflicting object versions are rejected and exposed as slashable evidence.
- RPC reports certificate finality for the object.
- Blocks include certificate batch roots for history and proof inputs.

### E3. Add certificate batch payload

Commit owned-object certificates in proof-ingestion blocks without replaying the underlying transactions.

Acceptance criteria:

- Batch root is deterministic.
- Batch root binds all certificate hashes and object versions.
- Conflicting certificates cannot appear in one accepted batch.

## Workstream F: Scope-Aware Routing

### F1. Route by execution scope, not only signer

Signer-based routing is simple but can bottleneck hot accounts.

Acceptance criteria:

- Owned `Agent(agent_id)` operations route by agent id.
- Owned `Receipt(receipt_id)` operations route by receipt id.
- Wallet anchors route by commitment.
- Shared/EVM transactions keep consensus routing.

### F2. Add routing diagnostics

Acceptance criteria:

- RPC reports computed home shard and route key.
- Mempool rejects wrong-shard submissions with route details.
- Load tests can report per-shard imbalance.

## Workstream G: Cross-Zone Messages and Forced Inclusion

### G1. Add message root payloads

Cross-zone interactions must be asynchronous.

Acceptance criteria:

- Zone proof update includes outbound message root.
- Base chain orders message roots.
- Destination zone consumes messages by inclusion proof.

### G2. Add forced-inclusion queue root

Users must have a censorship escape hatch.

Acceptance criteria:

- Base chain accepts forced-inclusion requests.
- Zone proof must include required forced-inclusion items after timeout.
- Missing forced-inclusion items reject zone proof finality.

## Workstream H: Benchmark Harness

Status: initial baseline-vs-experiment harness implemented in `scripts/bench-proof-ingestion-compare.sh`.

### H1. Baseline benchmark path

Run current `fractalchain` with full transaction execution.

Scenarios:

- native `NoOp` load;
- owned-object transaction load;
- proof commitment load;
- mixed EVM/native load;
- BFT-7 validator lab.

Metrics:

- submitted tx/sec;
- committed tx/sec;
- block p50/p95 production latency;
- CPU and memory;
- block bytes;
- DA bytes;
- validator replay time.

### H2. Proof-ingestion benchmark path

Run `fractalchain2` with proof-ingestion payloads.

Scenarios:

- proof updates/sec;
- certificate updates/sec;
- mixed proof updates plus shared-state txs;
- DA sampling enabled;
- BFT-7 validator lab.

Metrics:

- accepted proof updates/sec;
- accepted certificate updates/sec;
- block p50/p95 production latency;
- proof verification time;
- DA sampling time;
- CPU and memory;
- payload bytes.

### H3. Comparison report

Acceptance criteria:

- [x] Script emits JSON summaries for both repos.
- [x] Report includes baseline versus experiment side-by-side metrics.
- [ ] Report identifies bottleneck category: consensus, proof verification, DA sampling, network, or storage.

## Implementation Order

1. Add benchmark scripts that can run baseline and experiment side by side.
2. Add proof pool and direct proof-update RPC.
3. Add proof-ingestion payload roots while preserving legacy blocks.
4. Add replay-free apply mode for proof-covered updates.
5. Move DA commitments from transaction-list sidecars to zone blob commitments.
6. Add owned-object countersign RPC and certificate pool.
7. Add certificate batch payloads.
8. Add scope-aware routing.
9. Add cross-zone message roots and forced-inclusion queue root.
10. Run BFT-7 benchmark comparison.

## Open Questions

- Should proof-ingestion blocks have a single generalized payload root or separate roots per lane?
- Should owned-object certificates be signed by the full validator set or by the object's home-shard committee?
- Should DA sampling be mandatory for every validator or delegated to a sampler committee with aggregate evidence?
- How should base fees price proof verification versus DA bytes versus shared-state execution?
- What is the minimum proof-update public input set needed for benchmark realism before production STWO verification is complete?

## Definition of Done

This experiment is complete when:

- `fractalchain` and `fractalchain2` can run the same benchmark scenarios;
- `fractalchain2` can produce blocks containing proof updates and certificate batch roots;
- validators can accept proof-covered updates without replaying transaction bodies;
- DA verification can be sampled rather than fully reconstructed on the fast path;
- owned-object certificates can finalize object-local transactions before global ordering;
- a benchmark report shows whether proof ingestion improves throughput and latency without reducing validator quorum safety.
