# Proof Worker Runbook

## Inputs

Proof workers consume committed blocks or zone block headers, deterministic replay state, DA commitments, and witness metadata. Witnesses must be generated from node replay only.

## Circuit Selection

- Use `native_state_transition_v1` only when the block or zone feature set is fully native.
- Use `mixed_state_transition_v1` for any EVM-capable zone or any block using EVM execution or EVM-to-native precompile dispatch.
- Fail closed when the feature set exceeds the circuit coverage manifest.

## Zone Proofs

Zone proof-final updates must bind:

- `ZoneBlockHeaderV1` hash, state root, tx root, message root, DA namespace/root, forced-inclusion root, and timestamp.
- circuit version, coverage manifest digest, feature set, and public-input digest.
- source message root for cross-zone messages.
- required forced-inclusion root for base-layer forced-included transactions.

Native zones require native coverage. EVM-capable zones require mixed coverage.

## Settlement Safety

Bridge and settlement APIs must require proof-final blocks with covered circuit versions. Treat these errors differently:

- `soft-final`: wait for proof finality.
- `uncovered-circuit`: submit a proof with the correct circuit coverage.
- `unavailable-da`: restore or republish the required DA shares before settlement.
