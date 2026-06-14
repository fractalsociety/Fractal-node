# FractalChain Improve PRD

## Purpose

This PRD captures the recent blockchain architecture improvement track separately from the base testnet PRD and the benchmark PRD.

The goal is to move FractalChain from committee-executed sharding toward an agent-economy architecture that improves both:

- throughput, by avoiding unnecessary total ordering and re-execution;
- decentralization, by moving safety from executor committees toward proof verification and data availability.

The migration should be incremental. The current HotStuff/HyperBFT-style committee path remains the fast soft-finality path while new lanes are added beneath it.

## Architecture Thesis

Most FractalChain traffic is expected to be agent-local or append-only:

- task receipt posting;
- agent registry self-updates;
- wallet task receipt anchors;
- verifier scores and attestations;
- append-only work records.

These transactions do not always require global total ordering. Shared-state transactions still do:

- fee market updates;
- disputes;
- treasury operations;
- payout claims touching shared batch state;
- governance and slashing;
- cross-zone settlement.

The improvement plan therefore splits execution into three layers:

1. Owned-object fast path for non-contended agent operations.
2. Committee commit as fast soft finality.
3. Validity proof finality for settlement, bridging, and high-value state.

## Improvement 1: Owned-Object Transaction Semantics

### Requirement

Transactions that only touch signer-owned or append-only objects must be classifiable without executing them. The chain must be able to route these transactions into a future certified fast path.

### Initial Owned Objects

- `AccountNonce(address)`
- `Agent(agent_id)`
- `Receipt(receipt_id)`
- `WalletTaskReceipt(commitment)`

### Initial Owned Calls

- `UpdateAgent`
- `SettleReceipt`
- `WalletTaskReceiptAnchorV1`
- `NoOp`

### Shared/Consensus Calls

These remain on the ordered consensus path:

- transfers;
- EVM calls and creates;
- `RegisterAgent`;
- `SettleBatch`;
- `ClaimPayout`;
- disputes;
- staking, delegation, slashing, rewards;
- governance-controlled actions.

### Acceptance Criteria

- Every transaction has a deterministic execution scope: `Owned` or `Consensus`.
- Owned transactions expose their owned object set before execution.
- Mempool block selection prioritizes owned transactions.
- Mempool does not drain conflicting owned-object transactions in the same selection pass.
- Existing consensus execution remains valid and deterministic.

### Status

Implemented.

Current implementation:

- `OwnedObjectId`
- `TxExecutionScope`
- `Transaction::execution_scope()`
- owned-object prioritization and conflict filtering in the mempool

## Improvement 2: Proof-Final Settlement

### Requirement

Committee commits must remain fast and user-visible, but settlement finality must become proof-gated.

This creates two finality levels:

- `soft`: block committed by the committee / consensus path;
- `proof`: block has an accepted validity proof bound to its public inputs.

### Proof Public Inputs

A validity proof must bind to:

- `chain_id`
- `height`
- `block_hash`
- `state_root`
- `tx_root`

### Finality Rule

A block is proof-final only if:

1. the block exists locally;
2. the proof references the exact block hash;
3. the proof references the exact state root and transaction root;
4. the proof verifier accepts the proof system payload.

Until a production verifier accepts a concrete proof payload, the chain must not pretend STWO/Plonky2 proofs are verified.

### Acceptance Criteria

- Blocks are soft-final immediately after committee commit.
- Blocks become proof-final only after an accepted validity proof.
- RPC exposes block finality status.
- Unsupported production proof systems fail closed.
- Dev/test proof mode exists only to exercise the protocol flow.

### Status

Implemented.

Current implementation:

- `BlockValidityProof`
- `ValidityProofSystem`
- `verify_block_validity_proof`
- node proof-finality store
- `finalityStatus: "soft" | "proof"` in RPC block responses

Production Plonky2 verification is linked. The STWO execution AIR adapter and canonical recursive fixture are defined, but direct STWO proof acceptance remains fail-closed until the concrete STWO verifier is wired to that AIR.

## Improvement 3: Data Availability Commitments

### Requirement

The masterchain should move from full execution/data anchoring toward data availability commitments and sampling.

Validators should eventually verify:

- block headers;
- validity proofs;
- DA commitments;
- sampling evidence.

Validators should not execute all agent work in the long-term target.

### Target Block Additions

Future block/header structures should carry:

- DA root or namespace commitment;
- erasure-coded data commitment;
- blob count / byte count;
- sampling parameters;
- forced-inclusion queue commitment.

### Acceptance Criteria

- Light samplers can verify data availability without downloading all zone data.
- Full nodes can reconstruct zone data from posted shares.
- Proof-finality cannot be reached if required data is unavailable.
- DA pricing is explicit and separate from execution gas.

### Status

Planned.

## Improvement 4: Permissionless Execution Zones

### Requirement

Execution zones should eventually be permissionlessly spawnable without weakening global safety.

Zones may be operated by:

- one sequencer;
- a small sequencer set;
- a committee;
- a child zone settling recursively upward.

Safety must come from proof verification plus DA, not from trusting the zone operator.

### Zone Finality

Zones should have:

- local soft finality from their sequencer or committee;
- proof finality after mandatory validity proof acceptance;
- forced inclusion through the base/masterchain for censorship resistance.

### Acceptance Criteria

- Zone creation does not require governance approval once the proof/DA rules are live.
- Zone state transitions are rejected without valid proofs.
- Cross-zone interactions are asynchronous messages, not synchronous shared-state calls.
- Forced inclusion is available for censored users.

### Status

Planned.

## Improvement 5: Owned-Object Certificate Fast Path

### Requirement

After owned-object semantics are stable, implement a certificate path for eligible owned transactions.

Target flow:

1. owner signs transaction;
2. validators independently check shape, ownership, nonce/object version, and gas;
3. validators countersign;
4. `2f + 1` signatures form a certificate;
5. certified transaction is final for the owned object without waiting for global ordering.

### Acceptance Criteria

- Owned-object certificate includes transaction hash, owned object ids, owner, nonce/version, and validator signatures.
- Conflicting certificates for the same object are slashable evidence.
- Shared-state and mixed-object transactions fall back to consensus.
- Existing consensus blocks can still include certified transactions for archival/indexing purposes.

### Status

Planned.

The prerequisite classification layer is implemented.

## Implementation Checklist

### Phase A: Owned-Object Classification and Routing

- [x] Define owned-object ids for the first protocol surface: account nonce, agent, receipt, wallet task receipt.
- [x] Define transaction execution scope: `Owned` versus `Consensus`.
- [x] Add deterministic transaction scope classification.
- [x] Classify `UpdateAgent` as owned.
- [x] Classify `SettleReceipt` as owned.
- [x] Classify `WalletTaskReceiptAnchorV1` as owned.
- [x] Keep transfers, EVM calls, batch settlement, payout claims, disputes, staking, slashing, and governance on the consensus path.
- [x] Export owned-object protocol types from `fractal-core`.
- [x] Prioritize owned-object transactions in mempool selection.
- [x] Keep conflicting owned-object transactions queued instead of draining them together.
- [x] Add focused tests for owned-object classification.
- [x] Add focused tests for mempool owned-object prioritization and conflict filtering.
- [x] Add object-version tracking beyond account nonce.
- [x] Add explicit mixed-transaction detection for transactions touching both owned and shared state.
- [x] Add metrics for owned versus consensus mempool lanes.
- [x] Add RPC/debug endpoint to inspect a transaction's execution scope before submission.

### Phase B: Proof-Final Settlement

- [x] Define validity proof public inputs: `chain_id`, `height`, `block_hash`, `state_root`, `tx_root`.
- [x] Add `BlockValidityProof`.
- [x] Add `ValidityProofSystem`.
- [x] Add proof verifier boundary.
- [x] Add dev/test proof mode for exercising the finality flow.
- [x] Make unsupported production proof systems fail closed.
- [x] Store proof-finalized blocks separately from soft committee commits.
- [x] Add `BlockFinality::Soft` and `BlockFinality::Proof`.
- [x] Add node method for submitting a validity proof.
- [x] Add node method for querying block finality.
- [x] Expose `finalityStatus: "soft" | "proof"` in RPC block responses.
- [x] Add tests for soft-to-proof finality promotion.
- [x] Add tests for rejecting proofs for unknown blocks.
- [x] Link the real STWO/Plonky2 verifier.
- [x] Define production proof serialization format.
- [x] Add STWO AIR adapter and canonical recursive proof fixture for the execution circuit.
- [x] Add proof submission RPC or gossip path.
- [x] Persist proof-finality records to storage.
- [x] Add bridge/settlement APIs that require proof-final blocks.
- [x] Add chain config switch for proof-required settlement mode.
- [x] Add metrics for proof latency, proof rejection reason, and proof-final height.

### Phase C: Owned-Object Certificate Fast Path

- [x] Define `OwnedObjectCertificate` wire type.
- [x] Include transaction hash, owner, owned object ids, object versions, and signer nonce in the certificate.
- [x] Include validator countersignatures and signer bitmap/set.
- [x] Add certificate hash and canonical serialization.
- [x] Add validator-side precheck for owned transactions: shape, owner, object version, nonce, gas, fee.
- [x] Add validator countersign API.
- [x] Add certificate aggregation path for `2f + 1` validator signatures.
- [x] Add certificate verification function.
- [x] Add certificate mempool/lane separate from consensus mempool.
- [x] Add conflict detection for duplicate certificates over the same object/version.
- [x] Add slashable evidence type for conflicting owned-object certificates.
- [x] Add tests for valid certificate creation and verification.
- [x] Add tests rejecting mixed/shared transactions on the certificate path.
- [x] Add tests for conflicting certificate evidence.
- [x] Decide whether certified transactions are later embedded in consensus blocks for indexing/archival history.

Decision: certified owned-object transactions should be embedded later in consensus
checkpoint blocks for indexing, archival history, replay, and explorer consistency.
The certificate path remains the execution/finality fast path; consensus embedding is
an archival/indexing commitment and must not re-execute the already-certified
owned-object transition.

### Phase D: Data Availability Commitments

- [x] Define DA commitment type.
- [x] Add DA commitment fields to block/header or sidecar structure.
- [x] Define namespace model for execution zones.
- [x] Define initial blob/share encoding format.
- [x] Add erasure-coding prototype.
- [x] Add DA root calculation.
- [x] Add DA sidecar serialization.
- [x] Add full-node reconstruction test from shares.
- [x] Add initial deterministic sampling verifier model.
- [x] Add peer DA share request/response protocol.
- [x] Add follower-side DA sampling before synced block application.
- [x] Add multi-peer DA share custody/discovery bootstrap path.
- [x] Add automatic DA provider advertisement/discovery.
- [x] Add proof-finality rule that rejects proofs when required DA is unavailable.
- [x] Add synced-block replay rule that rejects invalid DA sidecars.
- [x] Expose DA commitment fields over RPC block responses.
- [x] Add DA gas/pricing fields separate from execution gas.
- [x] Add metrics for DA bytes, sampling success, reconstruction success, and DA fee revenue.

### Phase E: Proof-Secured Execution Zones

- [x] Define `ZoneId`.
- [x] Define zone registry state.
- [x] Add zone creation transaction.
- [x] Add zone metadata: proof system, DA namespace, sequencer policy, forced-inclusion policy.
- [x] Define zone block/header commitment.
- [x] Define zone state root and message root.
- [x] Add zone proof submission.
- [x] Add zone proof verification against the masterchain.
- [x] Add async message envelope.
- [x] Add cross-zone message inclusion proof format.
- [x] Add forced-inclusion queue.
- [x] Add forced-inclusion timeout/SLA rule.
- [x] Add tests for zone creation and proof-final zone updates.
- [x] Add tests for async cross-zone message delivery.
- [x] Add tests for forced inclusion after sequencer censorship.

### Phase F: Production Hardening and Rollout

- [x] Add operator documentation for soft finality versus proof finality.
- [x] Update explorer UI/API to show finality status clearly.
- [x] Update SDKs to expose finality status.
- [x] Add wallet warning for high-value actions that are only soft-final.
- [x] Add benchmark for proof latency and prover cost.
- [x] Add benchmark for owned-object certificate throughput.
- [x] Add benchmark for DA sampling bandwidth.
- [x] Add economics model for prover rewards.
- [x] Add economics model for sequencer rewards and forced-inclusion penalties.
- [x] Add governance/config parameters for enabling each phase.

## Expected Impact

### Throughput

Owned-object classification and certificates should improve per-zone throughput for agent-local traffic by removing unnecessary global ordering.

Proof-secured zones plus DA should improve aggregate throughput by allowing zones to scale horizontally without diluting validator security.

### Decentralization

The main decentralization improvement is safety decentralization:

- current model: safety depends on executor/committee honesty;
- target model: safety depends on cheap proof verification and data availability sampling.

Liveness and censorship resistance still require careful sequencer/forced-inclusion design.

### Tradeoffs

- Hard settlement finality moves from committee latency to proof latency.
- UX must present soft finality and proof finality distinctly.
- Cross-zone synchronous composability is not a target.
- Prover and sequencer markets need incentive design to avoid quiet centralization.
- Complexity increases because the system operates consensus, certificates, proofs, and DA.

## Non-Goals

- Replace HotStuff/BFT immediately.
- Claim production ZK security before the STWO/Plonky2 verifier accepts a concrete execution proof.
- Make every transaction owned-object eligible.
- Preserve synchronous composability across future execution zones.
- Hide proof latency from bridges, settlement systems, or high-value users.

## Open Questions

- What is the exact object versioning model for owned-object certificates?
- What stake/slash ratio is required for conflicting owned-object certificates?
- Which DA construction will be used for the first sampled prototype?
- What is the minimum forced-inclusion SLA per zone?
- Mixed native+EVM state transition proving is the product path. See
  `docs/mixed-state-transition-prd.md` for the implementation checklist. A
  native-only circuit may be used as a development fixture, but it must not grant
  production settlement finality for blocks that can include EVM execution.
