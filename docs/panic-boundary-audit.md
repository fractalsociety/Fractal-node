# T1 Panic Boundary Audit

**Date:** 2026-06-14
**Gate:** T1 - Hardened

## Scope

Externally reachable and operator-controlled inputs were reviewed for panic
boundaries:

| Surface | Entry points reviewed | Status |
| --- | --- | --- |
| RPC | `crates/rpc/src/module.rs`, `crates/rpc/src/masterchain_module.rs` | Request parsing returns JSON-RPC errors. Registration `expect` calls are startup invariants, not remote input. |
| Gossip / peer sync | `crates/node/src/p2p.rs`, `crates/network/src/codec.rs` | Borsh decode failures are logged or returned as `InvalidData`; frame size is bounded. |
| Block import / DA | `crates/consensus/src/lib.rs`, `crates/node/src/lib.rs` | DA sidecar construction now returns `DaVerifyError` instead of panicking on erasure-coding setup/encode failure. |
| Proof submission | `BlockValidityProof`, proof envelope, witness decode and verifier boundary | Decode paths return errors; fuzz target added for malformed proof/witness bytes. |
| Validator join | consensus stake deposit/register paths | Missing signer and treasury debit paths now return typed execution errors instead of relying on `expect`. |

## Fixed Panic Boundaries

- `deposit_consensus_stake` no longer uses `expect("signer")` for the debiting
  account after validation.
- `finalize_block_hooks` no longer uses `expect("treasury")` for block reward
  debit.
- Reward compounding no longer panics if a stake-share row is missing during
  distribution.
- `build_da_sidecar` is now fallible and propagates `DaVerifyError::ErasureCoding`.
- DA share commitment serialization no longer panics if serialization ever
  returns an error.

## Fuzz Targets

The `fuzz/` cargo-fuzz package contains:

- `tx_decode` - transaction and transaction-list decoding plus basic transaction
  shape/gas inspection.
- `proof_envelope` - block validity proof, Plonky2 proof envelope, mixed witness,
  and public input decoding.
- `da_share` - DA share/sidecar decoding, sampling verification, and
  reconstruction.
- `peer_message` - sync request/response, DA provider announcement, vote, and QC
  decoding.

Artifacts and generated fuzz targets are ignored under `fuzz/artifacts/` and
`fuzz/target/`. Seed corpus directories should be retained under
`fuzz/corpus/<target>/` when crashers or minimized seeds are added.

## Release-Gate Status

This audit completes the implementation and harness setup portion of T1. Full T1
exit still requires a timed fuzz run recorded with command, duration, target
commit, and retained seed corpus metadata.

Local validation on 2026-06-14:

- `cargo fuzz build` completed successfully for all four targets.
- Short local `cargo fuzz run` smoke attempts for `tx_decode` and
  `proof_envelope` did not return in the local session and were terminated. Do
  not mark the fuzz-corpus run complete until this is rerun in CI or a dedicated
  fuzz host with recorded duration and retained corpus metadata.
