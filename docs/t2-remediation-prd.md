# FractalChain T2 Validation Gate - Remediation PRD

**Version:** v1.0
**Status:** Active remediation
**Trigger:** T2 / M10 gate did not pass; see `docs/t2-validation-report.md`.
**Goal:** Get the T2 gate to a true pass: confirmed p99 <= 900 ms finality,
confirmed partition recovery, and M10 complete by fixing blockers in dependency
order and separating real bugs from harness/scope problems.

## What Failed

- BFT-7 multi-process shard smoke: all 7 validators started, stalled at height
  1, and timed out waiting for height 3.
- Two-shard M10 smoke: block production reached, but `proofsAccepted = 0x0`
  after 61 blocks.
- Sustained load plus p50/p95/p99 finality: could not complete; the local lab
  node did not stay available long enough to sample load.

Consequence: p99 <= 900 ms is unconfirmed, partition recovery is unconfirmed,
and M10 is incomplete. T2 remains unchecked.

## Triage

| # | Failure | Type | Severity | Blocks |
| --- | --- | --- | --- | --- |
| 1 | Height-1 BFT stall | Real consensus liveness bug | Hard blocker | Everything below |
| 2 | `proofsAccepted = 0` | Scope/wiring question, likely not a bug | Decision required | M10 proof criterion only |
| 3 | Node not available under load | Harness/supervision, possibly masking a crash | Hard blocker | Load, finality, partition tests |

Dependency order:

```text
WS1 consensus liveness -> WS3 process supervision -> WS4 load + p99 finality
                                                \-> WS5 partition recovery
WS2 proof scope decision -> wire proving OR split gate
```

Do not measure finality percentiles until the chain advances past height 1 and
nodes stay up under load. WS2 can proceed in parallel because it depends on a
scope decision.

## WS1 - Consensus Liveness: Height-1 Stall

Reaching height 1 but never height 3 means the genesis/bootstrap commit
succeeded, but the general pipelined-QC path did not form a quorum certificate.
For n=7, f=2, each view needs at least 5 mutually reachable, agreeing voters.

### WS1.0 - Make The Stall Legible

- [x] Add or inspect per-node metric: connected validator count; expected >= 6. Implemented via `fractal_consensusDiagnostics.connectedValidatorCount`.
- [x] Add or inspect per-node metric: current view/round number. Implemented via `fractal_consensusDiagnostics.currentView`.
- [x] Add or inspect per-node metric: current leader for height N. Implemented via `currentLeaderIndex` and `currentLeaderFingerprint`.
- [x] Add or inspect leader-side metric: votes received for height 2. Implemented via `height2VotesReceived` and signer/header fields.
- [x] Add or inspect leader-side metric: QC formed status/reason. Implemented via `qcStatus` and `qcReason`.
- [x] Capture full consensus logs from all 7 nodes around height 1 -> 2. Implemented in `scripts/run-hyperbft-bft7-shard.sh` diagnostics capture on smoke failure.
- [x] Record genesis hash on each node at startup. Implemented in startup consensus diagnostics log.
- [x] Record validator-set hash on each node at startup. Implemented in startup consensus diagnostics log.

### H1 - Mesh Did Not Form A Quorum-Connected Graph

Symptom: connected-validator count < 5 on some or all nodes.

- [ ] Verify each validator advertises a unique reachable address and port.
- [ ] Verify no multi-process localhost port collisions.
- [ ] Verify the static address book / bootstrap peer list is correct.
- [ ] Verify all nodes have identical peer membership.
- [ ] Add a wait-for-quorum-connected gate before consensus starts.
- [ ] Confirm libp2p/QUIC listeners are bound and dialable for all 7 validators.

### H2 - Validator-Set / Genesis Disagreement

Symptom: connected count is fine, but votes are discarded.

- [ ] Confirm all 7 nodes share an identical genesis hash.
- [ ] Confirm all 7 nodes share the same validator pubkey set.
- [ ] Confirm `FRACTAL_SHARD_ID` is unique per node and in range.
- [ ] Confirm `FRACTAL_SHARD_ID` does not remap validator identity/keys.
- [ ] Confirm `FRACTAL_SHARD_COUNT` is identical across nodes.
- [ ] Confirm `FRACTAL_CONSENSUS_MODE` resolves to the BFT path on all nodes.

### H3 - Leader Rotation / View-Change Defect

Symptom: nobody proposes height 2, or nodes disagree on leader.

- [ ] Verify leader election is deterministic across all nodes for height 2.
- [ ] Verify the elected height-2 leader proposes.
- [ ] Verify view-change timeout starts a new view instead of hanging.
- [ ] Add logs for `expected_leader`, `local_validator_index`, and `is_my_turn`.

### H4 - QC Aggregation / Threshold / Signature Bug

Symptom: >= 5 valid votes arrive at leader, but no QC forms.

- [ ] Verify QC threshold is `2f + 1 = 5`, not `n = 7`.
- [ ] Verify vote signature domain separation matches signer and verifier.
- [ ] Verify aggregate signature verification succeeds for known-good votes.
- [ ] Verify `highQC` propagates so the next view extends the correct block.
- [ ] Add rejection counters for invalid vote signature, wrong height, wrong view,
  wrong parent, wrong signer, and duplicate signer.

### H5 - View Timer Too Short For Multi-Process Cold Start

Symptom: rising view numbers, perpetual view changes, no commits.

- [ ] Increase initial view timeout or add startup grace period.
- [ ] Re-test after quorum-connected startup gate exists.
- [ ] Record p50/p95/p99 view-change duration during cold start.

### WS1 Exit

- [ ] BFT-7 smoke advances to height >= 3.
- [ ] BFT-7 smoke sustains commits beyond the initial height.
- [ ] Root cause branch is identified and recorded.

## WS2 - Proof-Finality Pipeline: `proofsAccepted = 0`

`proofsAccepted = 0` after 61 blocks is expected if no proof worker is running
and the native circuit is not enabled. Decide scope before treating this as a
bug.

### WS2.0 - Disambiguate

- [ ] Check `proofsSubmitted` / equivalent counter.
- [ ] Check `proofsRejected` counter.
- [ ] Distinguish zero submitted from rejected proofs.
- [ ] Check whether a proof worker process is running in the lab config.
- [ ] Check `native_transition_proofs_enabled`.
- [ ] Check `proofs_required_for_settlement`.
- [ ] Document whether proof-finality is in T2/M10 scope.

### Branch A - Proving Is In T2/M10 Scope

- [ ] Run a proof worker against the lab node.
- [ ] Generate witnesses from node replay.
- [ ] Enable `native_transition_proofs_enabled` in the smoke config.
- [ ] Confirm native-only blocks reach `proofsAccepted > 0`.
- [ ] Confirm native-only blocks become proof-final.
- [ ] If proofs are rejected, record rejection reason distribution.
- [ ] Fix rejection causes: circuit version enabled, public input match, coverage
  match, witness digest match.

### Branch B - Proving Is Not In T2/M10 Scope

- [ ] Split M10 gate into soft-final consensus finality and proof-finality.
- [ ] Move `proofsAccepted` criterion to the native proof milestone.
- [ ] Mark the T2 proof criterion N/A with written rationale.
- [ ] Update smoke scripts so T2 measures soft-final p99 independently of proof
  acceptance.

### WS2 Exit

- [ ] Branch A: proofs are accepted in smoke.
- [ ] Branch A: proof-finality records are visible in RPC.
- [ ] Branch B: proof criterion is formally deferred out of T2.
- [ ] Branch B: the deferral is documented in gate docs and runbook.

## WS3 - Node Availability And Process Supervision

`nohup` is a temporary harness fix. It helps only if processes died on session
hangup; it can hide panics and OOM kills.

### WS3.0 - Determine Why Node Left

- [ ] Capture node exit code.
- [ ] Capture terminating signal.
- [ ] Capture last N log lines.
- [ ] Capture panic/stack trace when present.
- [ ] Capture peak memory during load.
- [ ] Capture OOM-killer / system diagnostic signal where available.

### WS3.1 - Replace `nohup` With Real Supervision

- [ ] Add a supervised lab harness using systemd, a process manager, or a test
  supervisor.
- [ ] Add health checks for each node.
- [ ] Add restart policy for non-testnet local labs.
- [ ] Capture logs to deterministic files.
- [ ] Capture core dumps or panic artifacts.
- [ ] Add a node liveness endpoint or health check: RPC up plus height advancing.
- [ ] Make launcher wait for all nodes healthy before starting load.

### WS3.2 - If It Was A Real Crash Under Load

- [ ] Reproduce crash under bounded load.
- [ ] Classify cause: resource exhaustion, panic path, networking, storage, or
  proof worker.
- [ ] Fix crash.
- [ ] Add regression test or smoke reproducer.

### WS3 Exit

- [ ] All lab nodes stay up for the entire load window.
- [ ] All lab nodes stay up for the entire sampling window.
- [ ] Health checks remain green.
- [ ] Exit mode is understood and recorded.
- [ ] Any real crash is fixed before p99 measurement.

## WS4 - Load And Finality Percentiles

Depends on WS1 and WS3.

- [ ] Confirm `tools/load-tps` connects to supervised RPC endpoints.
- [ ] Keep load tool behavior: unavailable RPC returns normal error, not panic.
- [ ] Define sustained load profile: duration, workers, target TPS, tx type, gas
  limits, hardware.
- [ ] Run sustained load for the defined window.
- [ ] Sample finality latency during the same window.
- [ ] Compute p50 finality latency.
- [ ] Compute p95 finality latency.
- [ ] Compute p99 finality latency.
- [ ] Confirm p99 <= 900 ms for soft-finality, unless WS2 Branch A makes proof
  finality part of T2.
- [ ] Record results in the run report.

### WS4 Exit

- [ ] Sustained load completes.
- [ ] p50/p95/p99 are published.
- [ ] p99 <= 900 ms is confirmed over the sustained window.

## WS5 - Partition Recovery

Depends on WS1.

- [ ] Define partition scenario: 4/3 split.
- [ ] Define partition scenario below quorum, if separate from 4/3.
- [ ] Confirm minority side stalls and cannot finalize without `2f + 1`.
- [ ] Confirm minority side does not fork-commit.
- [ ] Heal partition.
- [ ] Confirm chain resumes commits after heal.
- [ ] Confirm both sides converge to one history.
- [ ] Confirm no double-commit or safety violation.

### WS5 Exit

- [ ] Partition recovery is confirmed.
- [ ] No safety violation observed.
- [ ] Partition test artifacts are attached to the run report.

## In-Flight Fix Disposition

| Fix made during failed gate | Verdict | Follow-up |
| --- | --- | --- |
| Added `fractal_getShardId`, `fractal_getShardCount`, `fractal_getConsensusMode` | Keep | Feed WS1 diagnostics. |
| Wired `FRACTAL_SHARD_ID`, `FRACTAL_SHARD_COUNT`, `FRACTAL_CONSENSUS_MODE` | Keep but audit | Verify H2: no validator-set or identity desync. |
| `nohup` in pilot / Track-B launchers | Replace | Supersede with WS3 supervised harness. |
| Added `tools/load-tps` to workspace | Keep | Use in WS4. |
| Load tool returns errors instead of panicking | Keep | Add regression if load tool gets tests. |
| Added `docs/t2-validation-report.md` | Keep | Extend with root cause and measured numbers. |

## T2 Exit Checklist

- [ ] BFT-7 smoke advances past height 3 and sustains commits.
- [ ] Lab nodes stay available for full load and sampling window under
  supervision.
- [ ] Sustained load completes.
- [ ] p99 finality <= 900 ms is confirmed.
- [ ] Partition recovery is confirmed with no safety violation.
- [ ] Proof criterion resolved: `proofsAccepted > 0` in smoke, or formally
  deferred out of T2 with rationale.
- [ ] Run report updated with root causes and measured numbers.
- [ ] T2 is checked only after every item above passes.

## Notes

This remediation PRD is reasoned from the run summary and current local logs.
WS1.0 and WS3.0 exist to turn hypotheses into known root causes before applying
fixes. Do not treat a one-time height-3 pass as sufficient unless the root cause
is recorded and the sustained/partition tests pass afterward.
