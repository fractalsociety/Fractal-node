# FractalChain Mixed State Transition Primitive PRD

**Version:** v1.1 (revised)
**Status:** Draft for Implementation
**Supersedes:** Mixed State Transition Primitive PRD (v1.0 draft)
**Companion to:** FractalChain L1 Testnet PRD v0.1; Dual-VM Execution Layer PRD v0.2; consensus/scaling track (owned-object fast path, proof-secured zones)

---

## Purpose

This PRD defines the implementation path for the first production state-transition proof primitive for FractalChain: a proof system whose output is a single validity proof over the unified state root, covering whichever execution surface a block actually used - native FractalWork/science operations, EVM execution, and EVM-to-native precompile calls - and binding to one ordered transaction list producing one post-state root.

The proof format, witness, public inputs, and verifier are mixed-ready from day one so we never run two incompatible proof systems. But delivery is sequenced native-first, because native execution is the launch wedge and native-block settlement finality is a real product that must not wait on a zkEVM.

## What Changed From v1.0

v1.0 made the mixed transition the first acceptance target and demoted native-only proving to a throwaway fixture. That coupled the ship date of all settlement finality to the hardest component in the system, faithful EVM proving, and contradicted the project's own positioning, where native is the launch wedge. v1.1 keeps the unified architecture but changes five things:

1. Native-block proof-finality is a shippable product, not a fixture. It is gated by a block-purity rule, so it confers genuine finality on native-only blocks without ever granting false finality to EVM-containing blocks.
2. The fail-closed rule is reframed from "reject native-only proofs for mixed blocks" to the more general and self-enforcing "a block is proof-eligible under `circuit_version V` only if its entire execution surface is covered by V" - and coverage is enforced as a circuit constraint, not a trusted node-side classifier.
3. Provers are heterogeneous; output is homogeneous. Native ops are proven by a bespoke STWO AIR. EVM execution is proven by running the node's revm transition inside a general-purpose zkVM rather than a hand-written EVM AIR. Both are unified at the Plonky2 recursion/aggregation layer into one proof binding one `post_state_root`.
4. The witness model now specifies its two largest and previously implicit components: the state-commitment hash choice and the Merkle inclusion witnesses for every pre-state read and post-state write.
5. A proving SLO is now mandatory: latency, throughput, and acceptable proof-final lag behind soft-final.

## Background

FractalChain already commits to one account/address space and one state root across native and EVM execution. The proof-finality track already requires block validity proofs to bind to `chain_id`, `height`, `block_hash`, `state_root`, `tx_root`, and zone namespace / DA roots when execution zones are involved.

The missing primitive is a canonical witness and circuit interface that proves the block transition that produced those public inputs - and a delivery sequence that ships finality for the launch wedge before the zkEVM exists.

## Product Decision

The production primitive is the unified state-transition proof. The first shippable milestone with real settlement value is purity-gated native-block proof-finality.

- The mixed transition is the eventual product and the permanent spec target.
- EVM-only proving is not a product. The chain always has native capacity, so practically all blocks are native or mixed; mixed coverage subsumes any EVM execution.
- Native-only proving is a product, not merely a fixture, because native is the launch market and purity-gating makes its finality sound.

### The Block-Purity Rule

A block carries a `feature_set`: the set of execution features its transactions used, including native opcode classes, EVM opcode/precompile classes, and bridge/materialization actions. A proof carries a `circuit_version` with a published coverage manifest enumerating the features it soundly proves.

> A block may be promoted to `proof-final` under `circuit_version V` iff `block.feature_set <= coverage(V)`, and the circuit itself proves that the block used no feature outside `coverage(V)`.

The second clause is the important one: coverage is a soundness property of the proof, not a trusted node-side check. The native-only circuit includes a constraint that no transaction has `VM-kind = EVM` and no precompile-dispatch row is present. A misclassifying node cannot manufacture false finality, because the proof would not verify.

## Heterogeneous Prover Architecture

```text
        +----------------------------+      +----------------------------+
        |  Native execution rows     |      |  EVM execution rows        |
        |  (from node replay)        |      |  (from node replay)        |
        +-------------+--------------+      +-------------+--------------+
                      |                                   |
                      v                                   v
        +----------------------------+      +----------------------------+
        |  STWO native AIR           |      |  General zkVM proving      |
        |  (bespoke, our edge)       |      |  revm transition in-zkVM   |
        |  fixed-cost opcodes,       |      |  (SP1 / RISC0 / Zeth)      |
        |  SNARK-friendly subtrie    |      |  EVM subset the node runs  |
        +-------------+--------------+      +-------------+--------------+
                      |  STWO statement digest            |  zkVM proof
                      +-------------------+---------------+
                                          |
                                          v
                       +-------------------------------------+
                       |   Plonky2 recursion / aggregation   |
                       |   - verify each component proof     |
                       |   - bind one post_state_root        |
                       |   - bind public-input digest        |
                       |   - compress for cheap verification |
                       +------------------+------------------+
                                          |
                                          v
                              one BlockValidityProof
```

Rationale: hand-writing a STWO AIR for full EVM semantics is reinventing the most expensive component in the industry. Native ops are where a bespoke AIR pays off: they are fixed-cost, dedicated-subtrie, near-branch-free. EVM execution is proven by proving the same revm code the node runs inside a general zkVM, which gives EVM fidelity for free and fails closed on any unsupported subset. Unification happens once, at recursion, where it is cheap relative to re-deriving an EVM circuit.

Open decision before Phase F: choose the general zkVM, such as SP1, RISC Zero, or a Zeth-style Type-1 approach, and define the exact revm subset compiled into it. Phase F is gated on this choice.

## Scope

### In Scope

- Canonical, versioned, mixed-ready execution witness including Merkle inclusion witnesses and a decided state-commitment hash.
- Canonical pre-state and post-state commitments.
- Transaction-level execution trace rows for native, EVM, and precompile-dispatched native calls.
- Public-input binding to the proof-finality fields, with header-hash re-derivation and time-context binding.
- Deterministic gas accounting in the witness, with `sum(per-tx gas) == gas_used` constrained.
- Deterministic event/log commitment in the witness.
- STWO AIR for the native execution trace.
- General-zkVM proving for the EVM execution trace.
- Recursive Plonky2 aggregation binding both into one statement, with the three recursion jobs distinguished: wrap, chain, and zone-aggregate.
- In-circuit coverage enforcement and a versioned coverage manifest.
- Node-side proof acceptance rules that reject proofs for unsupported circuit versions and uncovered feature sets.
- A proving SLO: latency, throughput, proof-final lag bound.
- Tests proving that proof-finality cannot be granted for a mismatched transition or an uncovered feature set.

### Out of Scope

- Full Ethereum mainnet historical compatibility proofs.
- A bespoke hand-written AIR for EVM semantics.
- Synchronous native-to-arbitrary-EVM callbacks.
- Proving off-chain model inference or LLM output quality.
- On-chain canonicalization of science identifiers.
- Replacing committee soft finality.
- Proving data availability. That is DAS's job; see "Validity vs Availability".

## Terminology

- `soft-final`: committee or sequencer committed the block.
- `proof-final`: an accepted validity proof proves the transition into the committed state root and proves the block's feature set is within the proof's coverage.
- `mixed transition`: a block transition that may contain both native and EVM execution.
- `native syscall`: a native operation invoked from EVM through a reserved precompile address.
- `witness`: canonical data consumed by the prover to recreate and prove the transition, generated from node replay.
- `feature_set` / `coverage(V)`: the features a block used / the features a circuit version soundly proves.

## State Commitment Decision

The trie hash function dominates circuit cost and was unspecified in v1.0. Decision:

- Native subtries use a SNARK-friendly sparse Merkle tree, such as Poseidon or Rescue, field-aligned to the STWO native AIR. We fully control these, so there is no compatibility cost and a large proving-cost win.
- EVM storage keeps an internal SNARK-friendly commitment for proving, while the RPC layer serves `eth_getProof`-style proofs by translation so external EVM tooling sees expected semantics. We do not prove keccak-MPT paths in-circuit unless a concrete tooling requirement forces it.
- The unified `post_state_root` is the commitment over both namespaces.

If a downstream requirement forces keccak-MPT for EVM storage in-circuit, treat it as a major cost event and revisit the EVM proving budget; do not absorb it silently.

## Required Public Inputs

The proof public statement binds at minimum to:

- `chain_id`
- `height`
- `block_hash`: re-derived in-circuit from the constituent roots and constrained equal; it is not a free input.
- `timestamp`: public input, required because EVM `TIMESTAMP` and native deadline ops depend on it.
- `parent_state_root`
- `post_state_root`
- `tx_root`
- `receipt_root`
- `native_event_root`
- `evm_log_root`
- `gas_used`: constrained equal to the sum of per-tx gas in the witness.
- `da_root` when DA is required.
- `zone_id` and `zone_block_hash` when proving an execution-zone block.
- `circuit_version` with its coverage manifest digest.

The existing `state_root` field in `BlockValidityProof` maps to `post_state_root`.

### Validity vs Availability

The proof proves the transition is correct given the data. It does not prove the data was published; that is data-availability sampling's job. `da_root` is bound so the two can be composed, but no part of the security model may assume the validity proof implies availability.

## Witness Model

Versioned and canonical: `MixedExecutionWitnessV1`. Generated from node replay, never hand-assembled by external provers. Includes:

- block context: chain id, height, timestamp, parent hash, block hash;
- pre-state root and post-state root;
- ordered transactions with canonical hashes; signer, nonce, gas limit, fee fields, VM kind per transaction;
- native call payloads and native state writes;
- EVM call/create payloads, touched accounts, storage reads/writes, gas usage, and logs;
- EVM-to-native precompile invocation rows: caller, precompile address, decoded native call, native result, gas charged;
- receipt status and per-transaction gas used;
- event/log roots;
- Merkle inclusion witnesses for every pre-state read, authenticated against `parent_state_root`;
- Merkle write witnesses for every post-state write, including sibling paths required to recompute toward `post_state_root`;
- DA namespace/root if the transition belongs to a zone.

The Merkle witness data is the bulk of the witness and the bulk of circuit cost; it is not optional.

### Determinism Requirements

- Canonical serialization; no host-dependent map/iteration order.
- No floating point anywhere in the replay or witness path.
- The EVM fork rules and gas schedule are pinned by `circuit_version`. An opcode repricing across a fork is a new circuit version, because it changes the trace.

## Verification Rules

A proof can promote a block or zone block to proof-final only if:

1. the proof envelope `circuit_version` is enabled by chain config;
2. the proof public inputs exactly match the committed block/header, including in-circuit re-derived `block_hash`;
3. the proof public inputs bind to the canonical witness digest;
4. the verifier accepts the recursive Plonky2 proof payload;
5. required DA commitments are present and available;
6. `block.feature_set <= coverage(circuit_version)`, and the proof itself attests the block used no feature outside that coverage.

If any transaction uses an opcode, precompile, native call, or bridge/materialization action not covered by the active circuit version, production proof verification fails closed.

## Recursion / Aggregation

v1.0 conflated these. They are separate and must be specified separately:

1. Wrap, STARK to SNARK: verify a component proof, such as the STWO native statement or the EVM zkVM proof, inside Plonky2 to compress verification cost.
2. Aggregate, intra-block: combine the native component proof and the EVM component proof for a single block into one proof binding that block's single `post_state_root`.
3. Chain / zone-aggregate, inter-block: enforce `post_state_root(N) == parent_state_root(N+1)` for chain linking, and aggregate zone block proofs upward to the masterchain.

Critical-path validation item: the STWO to Plonky2 step crosses fields, Circle STARK / Mersenne-31 to Plonky2 / Goldilocks. Efficient recursive verification of a Circle STARK inside Plonky2 must be confirmed to be production-cheap and not itself a research effort before committing Phases C-D. If it is not yet mature, the wrap step may need an intermediate proof system or a different field-aligned recursion target. Validate against current tooling.

## Proving SLO

These are target budgets to be validated against real benchmarks, not asserted facts. Fill from Phase G measurements; treat as falsifiable acceptance gates.

- Native block proof latency: target a single native block proven within a small multiple of block time, such as within minutes, on defined prover hardware.
- Proof-final lag bound: proof-final is expected to trail soft-final; define the maximum acceptable lag, such as bounded minutes for native blocks, and alert when exceeded.
- Prover throughput: aggregate prover capacity must keep pace with block production over a sustained window so the proof-final frontier does not fall permanently behind soft-final.
- EVM block proof budget: a separate, explicitly larger budget for mixed blocks, set after the zkVM choice in Phase F. If mixed-block proving cannot meet a usable lag, mixed proof-finality may be batched/periodic rather than per-block. That is acceptable only as a stated decision, not an accident.

## Rollout Strategy

The witness, public inputs, and verifier are mixed-ready throughout. Delivery sequences native-first.

### Phase A: Canonical Mixed-Ready Witness

- [x] Define `MixedExecutionWitnessV1` wire type.
- [x] Define `MixedExecutionPublicInputsV1`.
- [x] Add `parent_state_root` to the proof public statement path.
- [x] Add `receipt_root`, `native_event_root`, and `evm_log_root` commitments where missing.
- [x] Decide and implement the state-commitment hash: SNARK-friendly native subtrie; internal EVM commitment.
- [x] Add Merkle inclusion witnesses for all pre-state reads and post-state writes to the witness format.
- [x] Add `timestamp` to public inputs; re-derive `block_hash` in-circuit and constrain equality.
- [x] Add `feature_set` to the witness and `coverage manifest` to `circuit_version`.
- [x] Define canonical witness hash / digest.
- [x] Generate mixed witnesses from deterministic block replay.
- [x] Persist witness metadata for proof workers and decide witness retention/availability policy.
- [x] Tests: replaying the same block produces identical witness bytes.
- [x] Tests: transaction reordering changes the witness digest.
- [x] Tests: native-only, EVM-only, and mixed blocks all produce valid witness shapes.

### Phase B: Execution Trace Coverage

- [x] B1 native: trace rows for native transaction execution; bind native subtrie writes, nonce/balance changes, event order, gas.
- [x] B1 native: trace rows for EVM-to-native precompile dispatch, the native side of the syscall.
- [x] B1 native: in-circuit constraint that a native-only block contains no EVM VM-kind tx and no precompile-dispatch row.
- [x] B2 EVM: define the EVM transition surface the node executes, the exact revm subset, as the zkVM proving target; mark uncovered features.
- [x] B2 EVM: bind EVM account/storage/code-hash/log commitments and gas to the zkVM input/output.
- [x] Bind `sum(per-tx gas) == gas_used`.
- [x] Tests: proof eligibility rejected for unsupported opcodes/actions.

### Phase C: Native STWO AIR

- [x] Define `NativeStateTransitionAirV1`.
- [x] Map native witness rows into STWO trace columns.
- [x] Bind public-input digest into the Fiat-Shamir transcript.
- [x] Bind witness digest into the STWO statement.
- [x] Native-op constraint subset: fixed-cost opcodes, science modules.
- [x] Precompile-dispatch constraint subset: native side.
- [x] Native subtrie Poseidon/Rescue inclusion/update constraints.
- [x] Gas + receipt + native-event-root constraints.
- [x] Coverage constraint: no EVM execution present.
- [x] Fixture: prove a block of native science txs, including dataset register, provenance, experiment attest, bounty claim. Current fixture uses the implemented native work/research equivalents: agent registration, wallet receipt/provenance anchor, batch settlement, and payout claim; replace with science-specific opcodes when they land.

### Phase D: Plonky2 Recursion - Wrap + Chain

- [x] Define canonical recursive fixture for the native STWO statement: job 1, wrap.
- [x] Add `circuit_version = native_state_transition_v1` with coverage manifest.
- [x] Bind STWO statement digest and public inputs to existing `BlockValidityProof` fields.
- [x] Enforce inter-block chaining `post_state_root(N) == parent_state_root(N+1)`: job 3, native.
- [x] Compressed Plonky2 proof fixture for the native circuit.
- [x] Verifier tests: valid native proof; reject public-input mismatch; reject stale circuit versions; reject EVM-containing block under native coverage.

### Phase E: Node Integration - Purity-Gated Native Proof-Finality

- [x] Chain config switch `native_transition_proofs_enabled`.
- [x] Chain config switch `proofs_required_for_settlement`, per feature class.
- [x] Compute `block.feature_set` and gate proof submission/acceptance by coverage.
- [x] Native-only blocks become proof-final after a valid native proof.
- [x] Reject proof-finality for any block whose feature set exceeds the submitted proof's coverage.
- [x] Store proof-final records with circuit version, coverage manifest digest, public-input digest.
- [x] Expose proof circuit version + coverage in RPC finality status.
- [x] Metrics: witness-gen latency, proof latency, proof-final lag, unsupported-feature rejections.
- [x] Tests: soft-final native block becomes proof-final after valid proof.
- [x] Tests: reject proof-finality when public inputs match but witness digest does not.

### Phase F: EVM Proving via General zkVM

- [x] Confirm the zkVM choice: RISC Zero guest proving the node's `fractal_evm::RevmEngine` transition, revm `38.0.0`; compiled subset is value-zero EVM transfer/call/create plus reserved native-precompile dispatch.
- [x] Prove the node's revm transition in-zkVM over the EVM trace surface from B2 via `EvmZkVmTransitionStatementV1` / `EvmZkVmFixtureV1`.
- [x] Bind zkVM output to the unified `post_state_root` EVM namespace and to logs/gas.
- [x] Coverage manifest for `circuit_version = mixed_state_transition_v1`.
- [x] Fail closed on any revm feature outside the compiled subset.
- [x] Set the EVM/mixed proving budget: native target 120s, mixed target 900s, max proof-final lag 1,800s, 4 blocks/min sustained prover target, per-block preferred with batched fallback.

### Phase G: Mixed Aggregation + Proof-Finality

- [x] Plonky2 intra-block aggregation of native STWO proof + EVM zkVM proof into one block proof: job 2.
- [x] `circuit_version = mixed_state_transition_v1` end-to-end.
- [x] Fixture: block with one native tx and one EVM tx.
- [x] Fixture: EVM tx that calls a native precompile, with cross-VM atomicity reflected in the proof.
- [x] Node accepts mixed proof-finality under mixed coverage; rejects mixed blocks under native-only coverage.
- [x] Benchmarks feeding the SLO: witness-gen, native proof, EVM proof, aggregation, verification latency.

### Phase H: Zone, Bridge, SDK, Explorer, Ops

- [x] Bind `ZoneBlockHeaderV1` fields into public inputs; zone-aggregate recursion upward: job 3, zones.
- [x] Require appropriate-coverage proof-final before zone proof-final updates: native coverage for native zones; mixed coverage for EVM-capable zones.
- [x] Require proof-final blocks of covered circuit version for bridge settlement APIs.
- [x] Bridge API error distinguishing soft-final, uncovered-circuit, and unavailable-DA.
- [x] Cross-zone message tests where source-zone message root is proven.
- [x] Forced-inclusion: circuit proves base-layer forced-included txs were executed, as a constraint, not just a test.
- [x] SDK + explorer surface proof circuit version and coverage; operator docs for fail-closed behavior; proof-worker runbook; benchmarks.

## Acceptance Criteria

- A native-only block can be witnessed and proven and promoted to proof-final via `native_state_transition_v1`, with the proof attesting no EVM execution was present. This is the first product milestone and does not require the EVM circuit.
- A block with only EVM transactions can be witnessed and proven via the mixed circuit.
- A block with both native and EVM transactions can be witnessed and proven via the mixed circuit.
- A block with an EVM-to-native precompile call can be witnessed and proven, with cross-VM atomicity reflected in the proof.
- The verifier rejects proofs whose public inputs do not match the committed block, including in-circuit `block_hash`.
- The verifier rejects proofs whose circuit version is not enabled.
- The verifier rejects proof-finality for any block whose feature set exceeds the proof's coverage, enforced both by node check and by an in-circuit coverage constraint.
- Bridge and settlement APIs can require proof-final blocks backed by a circuit whose coverage matches the block.
- The proving SLO targets are defined and measured; proof-final lag stays within the stated bound for native blocks.
- The full stack uses no trusted setup.

## Design Invariants

- No trusted setup, anywhere. STWO and Plonky2 are FRI-based and transparent; this is a credibility asset for a DeSci settlement layer. No KZG-based wrapper or any component requiring a ceremony may be introduced without an explicit decision to abandon this invariant.
- Witness from node replay only. External provers never hand-assemble witnesses.
- Coverage is a proof property, not a trusted classifier output.
- One block, one post-state root, one proof, regardless of how many heterogeneous provers contributed.

## Current Status

Not implemented as a production primitive.

Already available:

- proof-finality state and RPC status;
- `BlockValidityProof` and proof verifier boundary;
- production Plonky2 verifier linkage;
- STWO adapter and recursive fixture scaffolding;
- zone block/header commitments;
- DA commitments and sampling path;
- bridge/settlement proof-finality requirement hooks.

Still missing:

- decided state-commitment hash + Merkle inclusion witnesses in the witness;
- `feature_set` / coverage manifest and in-circuit coverage constraint;
- native STWO AIR and native recursive fixture: the first product path;
- general-zkVM EVM proving and the zkVM choice;
- intra-block aggregation of heterogeneous proofs;
- node purity-gating of proof-finality;
- proving SLO instrumentation;
- SDK/explorer visibility into circuit version and coverage.

## Risks

- Faithful EVM proving is expensive. Mitigated by proving the node's own revm inside a general zkVM rather than hand-writing an EVM AIR, starting from the exact executable subset, and failing closed. Mixed proof-finality may be batched if per-block latency is infeasible, but only as a stated decision.
- STWO to Plonky2 cross-field recursion may be immature. It is on the critical path; validate against current tooling before Phases C-D. If not production-cheap, an intermediate wrap target may be needed.
- State-commitment hash choice drives cost. Resolved toward SNARK-friendly commitments; do not silently absorb keccak-MPT-in-circuit if a tooling requirement later pushes for it.
- Coverage misclassification could cause false finality. Mitigated by enforcing coverage as an in-circuit constraint, not a node-side check alone.
- Witness determinism across platforms: canonical serialization, no floating point, no host-dependent iteration; fork/gas schedule pinned by circuit version.
- Proof-final silently lagging soft-final. Circuit version, coverage, and lag must be visible in RPC/explorer/metrics so a stuck block has a legible reason.
