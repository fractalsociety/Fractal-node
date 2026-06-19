# FractalChain Testnet Release Gates PRD

**Version:** v0.1
**Status:** Draft for implementation tracking
**Companion:** `docs/remaining-work.md`, `docs/prd.md`

## Purpose

Define the milestone gates for moving FractalChain from closed/invite-only
testnet to public RPC and public validator registration.

The external audit remains a separate, parallel release gate. It does not block
the closed/invite-only testnet, but it must land before opening RPC and
validator registration to the public.

## Release Rule

- Closed/invite-only testnet can ship after T3 exits.
- Public RPC and public validator registration require T4.
- T4 requires audit completion or an explicit signed release exception.

## Milestone Checklist

### T0 - Harness

**Workstream:** WS-1

**Exit:** CI green, nightly scheduled.

- [x] Install/activate CI workflows in `.github/workflows/`.
- [x] Decide which jobs are PR-gated versus nightly.
- [x] Add bounded pilot smoke job with log capture.
- [x] Add bounded masterchain + pilot smoke job.
- [x] Schedule nightly long-running validation.
- [x] Publish CI status and artifact retention policy.
- [ ] T0 exit sign-off: CI is green and nightly jobs are scheduled.

### T1 - Hardened

**Workstream:** WS-3

**Exit:** No reachable panics, fuzz clean.

- [x] Audit externally reachable RPC, gossip, block import, proof submission, and validator join paths for panic boundaries. See `docs/panic-boundary-audit.md`.
- [x] Convert reachable `unwrap` / `expect` / panic paths to typed errors where input can be remote or operator-controlled.
- [x] Add fuzz targets for transaction decoding, proof envelope parsing, DA share handling, and peer message parsing.
- [ ] Run fuzz corpus with documented duration and seed retention.
- [x] Add regression tests for discovered crashers and audited panic boundaries.
- [ ] T1 exit sign-off: no reachable panics and fuzz pass is clean.

### T2 - Proven

**Workstream:** WS-2

**Exit:** p99 <= 900 ms, partition-safe, M10.

**Latest local run:** See `docs/t2-validation-report.md`. T2 is not signed off;
local validation found BFT-7 shard progress and M10 proof-worker blockers.
Remediation is tracked in `docs/t2-remediation-prd.md`.

- [ ] Run deterministic BFT-7 torture with partitions and view changes.
- [ ] Run sustained-load lab benchmark on target hardware.
- [ ] Measure and publish p50, p95, p99 finality latency.
- [ ] Confirm p99 finality latency is <= 900 ms under the stated load profile.
- [ ] Confirm partition recovery does not violate safety.
- [ ] Complete M10 exit: two shards finalize independently through the RPC gateway with no proof-worker latency regression.
- [ ] T2 exit sign-off: p99 target met, partition-safe, M10 complete.

### T3 - Shippable

**Workstream:** WS-4

**Exit:** Tagged artifact + operator join + M11.

- [ ] Produce tagged release artifact.
- [ ] Publish artifact checksums and build provenance.
- [ ] Document operator join flow for closed/invite-only validators.
- [ ] Add or verify operator join smoke test.
- [ ] Complete M11 exit benchmark: pruned proof-chain sync versus full replay at realistic large height.
- [ ] Publish closed testnet runbook.
- [ ] T3 exit sign-off: tagged artifact, operator join path, and M11 complete.

### T4 - Open

**Workstream:** Audit

**Exit:** Public RPC + validator registration.

- [ ] Complete external security audit or record a signed release exception.
- [ ] Triage audit findings and assign severity.
- [ ] Fix all critical/high findings or explicitly defer with signed risk acceptance.
- [ ] Verify remediation with tests and auditor review where applicable.
- [ ] Open public RPC only after audit gate is satisfied.
- [ ] Open public validator registration only after audit gate is satisfied.
- [ ] T4 exit sign-off: public RPC and validator registration are approved.

## Gate Matrix

| Milestone | Workstreams | Exit |
| --- | --- | --- |
| T0 - Harness | WS-1 | CI green, nightly scheduled |
| T1 - Hardened | WS-3 | No reachable panics, fuzz clean |
| T2 - Proven | WS-2 | p99 <= 900 ms, partition-safe, M10 |
| T3 - Shippable | WS-4 | Tagged artifact + operator join + M11 |
| T4 - Open | Audit | Public RPC + validator registration |

## Tracking Notes

- Audit is parallel to T0-T3. It should start as soon as protocol surfaces are
  stable enough for useful review.
- T4 is not required for closed/invite-only testnet.
- Public-facing expansion is blocked until T4 is satisfied.
- Each exit sign-off should include date, commit/tag, command set, and result
  artifacts.
