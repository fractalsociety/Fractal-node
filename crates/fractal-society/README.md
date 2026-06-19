# fractal-society

Canonical, tested protocol spec for the **Fractal Society** subsystem
("Train in Simulation. Prove in Public. Deploy with Confidence.").

This crate implements the **PHASE-01** canonical research schema + artifact
ledger and the **PHASE-02** generic simulation kernel, per
[`docs/new doc/fractal_society_simulation_proof_deployment_prd_v0_2_reconciled.md`](../../docs/new%20doc/fractal_society_simulation_proof_deployment_prd_v0_2_reconciled.md)
and the gate checklist in
[`fractal-society-phase-gates-v0.2-reconciled.yaml`](../../docs/new%20doc/fractal-society-phase-gates-v0.2-reconciled.yaml).

## Role

- **This crate is the canonical protocol spec** (types, integrity rules, the
  generic kernel contract, and a deterministic reference adapter). It is
  domain-neutral: trading/venue code is forbidden here (enforced by
  `tests/architecture_boundary.rs`, gate P02-N09).
- **The TypeScript app in `~/fractalwork` mirrors these schemas at runtime** and
  reuses its own `packages/core` crypto. The two layers must agree on the
  conventions below so an artifact hashed/signed in Rust verifies in TS.

## Conventions (must match `fractalwork`)

| Concern | Convention |
| --- | --- |
| Canonical object hash | **SHA-256** of canonical JSON (JCS-style: sorted keys, compact, finite floats, `-0`→`0`) — see [`canonical`](src/canonical.rs) |
| Raw-bytes hash | SHA-256 (`Hash::new`) |
| Author signatures | **Ed25519** over canonical signable bytes, hex-encoded — see [`signing`](src/signing.rs) |
| Determinism | kernel draws all randomness from a seed; no wall-clock or OS RNG in the run hot path |

## Modules

| Module | Purpose |
| --- | --- |
| `protocol` | Canonical research schemas (ResearchProject, Protocol, EvidenceBundle, ProofManifest, …) |
| `artifact` | Content-addressed artifact manifests + signed package digests |
| `canonical` | Canonical JSON + SHA-256 content hashing (P01-N02) |
| `signing` | Ed25519 author signatures over canonical payloads (P01-N03) |
| `simulation` | `DomainAdapter` + `Agent` contracts, `RunTrace`, runtime types |
| `kernel` | Generic, deterministic run/replay driver (P02-N01, P02-N10) |
| `adapters` | Domain adapters; `reference` is a seeded bandit proving genericity |
| `verifier` | Verifier packages, proof levels (P0–P6), simulation tiers, scorecards |

## Gate → implementation map

| Gate | Where |
| --- | --- |
| **P01-N02** canonical hash | `canonical::content_hash`, `tests/canonical_hash.rs` |
| **P01-N03** tamper detection + signatures | `signing`, `ProofManifest::verify_author`, `tests/tamper_detection.rs` |
| **P01-N01** unified schema | single `EvidenceBundle`/`ResourceLimits`; `Review`/`Replication` in `verifier` |
| **P02-N01** generic kernel | `kernel::run` |
| **P02-N02** reference adapter | `adapters::ReferenceAdapter`, `tests/reference_adapter.rs` |
| **P02-N03/N04/N07** determinism | `tests/kernel_determinism.rs` |
| **P02-N05** invalid-action rejection | `ReferenceAdapter::validate_action` |
| **P02-N09** architecture boundary | `tests/architecture_boundary.rs` |
| **P02-N10** export/replay | `kernel::replay`, `tests/replay.rs` |

## Verify

```sh
cargo test  -p fractal-society          # 30 tests (16 lib + 14 integration)
cargo clippy -p fractal-society --all-targets
cargo fmt   -p fractal-society -- --check
```

## Out of scope for this slice

Trading adapter/ledger (PHASE-04), Hyperliquid recorder (PHASE-03), verifier
runtime + holdouts (PHASE-06), proof registry + chain adapter (PHASE-07 — the TS
app already has a settlement adapter), the pnpm gate harness, and PHASE-00
founder-decision signing docs.
