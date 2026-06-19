# Fractal Society Legacy Reconciliation Report

**Date:** 2026-06-19
**Baseline:** `mater-june-19th.md`
**Tracker header:** 650/1666 implementation items checked (39.0%); 1016 open.
**Parser audit:** 1682 checkbox lines total, including 16 manual-review placeholders; 1032 unchecked checkbox lines.

## Evidence policy

A legacy checkbox is not treated as a passed gate. It is a **reuse candidate** until PHASE-00R finds current code, a runnable test, runtime evidence, an owner, and no critical regression. Manual PRDs with no checkbox-formatted items are classified as **unknown**, not as 0% implemented. The 16 added manual-review placeholders explain why the physical markdown contains 1,682 checkbox lines while its header reports 1,666 implementation items.

## Source status

| Source | Checked | Total | Completion | Planning interpretation |
|---|---:|---:|---:|---|
| Fractal Website Blockchain Integration PRD | 0 | 7 | 0.0% | Manual review only; zero checkbox evidence does not prove absence or completion. |
| Repository Feature Graph PRD | 0 | 6 | 0.0% | Manual review only; verify code before reuse. |
| FractalWork Core MVP Implementation Plan | 0 | 1 | 0.0% | Manual review only; verify signed events, artifacts, timeouts, and persistence. |
| PRD: Forge - Train, Deploy, and Rent Specialist Agents | 0 | 1 | 0.0% | Manual review only; marketplace is not an MVP blocker. |
| PRD: Forge Router - Route Work to Specialized Monetized Agents | 0 | 1 | 0.0% | Manual review only; use detailed router checklist for evidence. |
| Implementation Checklist: Forge Specialist Agent Router | 32 | 189 | 16.9% | Partial base: models/index/classifier/outcomes exist; filters/ranker/API/explanations remain open. |
| PRD Checklist: RL Gyms and Verifiers Marketplace | 238 | 343 | 69.4% | Strongest reusable substrate: schemas, runner, scorecards, sandbox, and training loop are substantially checked. |
| PRD: Fractal Forge Local Engine, Headless CLI, and Dashboard Integration | 112 | 365 | 30.7% | Useful partial base: dashboard/training/eval pieces exist; full CLI/data/privacy/production acceptance is incomplete. |
| PRD: Hermes and OpenClaw Agent Integration for Forge and FractalWork | 173 | 185 | 93.5% | Highly reusable agent-runtime, permission, connector, evaluation, and marketplace scaffold; revalidate real endpoints. |
| PRD: Digital Employee Skills, RAG Memory, and Codex Offload Layer | 95 | 241 | 39.4% | Reusable skill schemas, packs, ingestion, and retrieval; generic tool-permission/eval/dashboard work remains open. |
| PRD: Fractal Society Graph Operating System | 0 | 343 | 0.0% | Treat as unimplemented for planning: graph, reputation, fraud, and settlement remain new work. |

## Reconciled phase impact

| Phase | Reuse level | Reuse focus | Primary remaining work |
|---|---|---|---|
| PHASE-00 | `decision_only` | Existing mission, product thesis, privacy principles, and simulation-first non-goals. | Approve the reconciled scope and the rule that legacy checkmarks require regression evidence.; Freeze the narrow first market and non-token reward policy. |
| PHASE-00R | `audit_all_existing` | All checked legacy capabilities that can be tied to current code, tests, and an executable runtime. | Build a machine-readable capability inventory.; Downgrade unsupported checkmarks. |
| PHASE-01 | `substantial_reuse` | VerifierPackage, RLGymPackage, SuitePackage, run manifests, package digests, visibility controls, scorecard objects, and agent harness metadata. | Add generic research objects and cross-artifact relations.; Standardize canonical serialization, audit events, privacy contracts, and project export/import. |
| PHASE-02 | `substantial_reuse` | Episode runner, seeded replay, reward traces, attached verifiers, run manifests, sandbox execution, resource limits, and egress denial. | Create the domain adapter contract and deterministic reference adapter.; Close raw-trace policy, flaky-run detection, invalid-action penalties, and safety-hard-failure gaps. |
| PHASE-03 | `mostly_new` | Generic artifact manifests, run event formats, secret handling, observability conventions, and sandbox/network policy. | Implement the venue adapter, normalized market schema, gap detection, dataset manifests, rate-limit budgeting, and recorder health operations. |
| PHASE-04 | `generic_runtime_reuse_domain_new` | Generic episode lifecycle, seed control, reward traces, artifact manifests, verifier hooks, and sandboxing. | Trading state/action schemas, portfolio ledger, fill models, fees, funding, margin, liquidation, baselines, and simulation disclosures. |
| PHASE-05 | `substantial_reuse` | Forge dashboard and packaging, model registry, trainer abstraction, base-vs-adapter evaluation, imported-agent harnesses, connector health, permission denial, secret isolation, skill registry, and sandbox runtime. | Add the trading agent manifest/action contract and starter templates.; Unify native/imported-agent permissions with skill tool scopes. |
| PHASE-06 | `substantial_reuse_hardening_required` | Verifier runtime, suite packaging, baseline comparisons, scorecards, public/private disclosure, hidden episode metadata, calibration, score provenance, batch runs, and improvement feedback. | Implement trading-specific verifiers and leakage attacks.; Harden holdout isolation and probing defenses. |
| PHASE-07 | `partial_reuse` | Package hashes, signed digests, artifact cards, visibility tiers, scorecard rendering, and agent listing metadata. | Proof manifest, Merkle commitment, reviewer grants, chain adapter, finality monitor, verification CLI, and minimal proof explorer.; Keep graph projection minimal and relational until PHASE-10. |
| PHASE-08 | `partial_reuse` | Agent/artifact cards, scorecard renderer, evaluation runner, package freezing, imported-agent support, outcome records, metering records, and privacy controls. | Season rules/state machine, submission freeze, private final, robust leaderboard, appeals, research credits, postmortems, and launch operations. |
| PHASE-09 | `runtime_pattern_reuse_domain_new` | Connector interface pattern, encrypted secrets, server-side invocation, health checks, permission events, fallback status, runtime telemetry, and default no wallet spend. | Venue adapter, live shadow portfolio, testnet order path, reconciliation, stale-data stops, emergency controls, and nonce/order lifecycle handling. |
| PHASE-10 | `mostly_new` | Routing outcome records, privacy controls, human-review capability, score provenance, reward-submission-preparer skill, and proof manifests. | Minimal graph schema/storage, review/conflict rules, replication, reputation events, fraud checks, reward gates, explainability, and checkpoint commitments.; Defer advanced centrality dashboards, DeSci views, and full graph analytics. |
| PHASE-11 | `partial_reuse` | Coding gym template, tool-use runner, repo-map/file-finder/test-selector/change-summary skills, imported agents, sandbox, scorecards, and proof registry. | Software-domain adapter, repository fixture, hidden tests, patch evidence, proof-card rendering, and cross-domain architecture enforcement. |
| PHASE-12 | `permission_reuse_execution_new` | Verified wallet/tool permission records, sub-wallet assignment, spending limits, revocation, denied events, secret isolation, and incident telemetry. | Legal review, isolated signer, dedicated venue account/subaccount, risk firewall, canary policy, withdrawals disabled, kill switches, monitoring, and postmortem. |

## Strongest verified-reuse candidates

- Verifier, RL gym, and suite package schemas are fully checked in the legacy tracker.
- The RL gym MVP and training-loop integration phases are fully checked; runtime and scoring have a small set of explicit hardening gaps.
- Forge training backend abstraction and evaluation requirements are fully checked; dashboard integration is partial.
- Hermes/OpenClaw runtime, harness, security, testing, and rollout sections are almost entirely checked.
- Skill schemas, core/Forge/graph skill packs, ingestion, and retrieval are checked; generic tool permissions and evaluation remain open.
- Router data models, index, classifier, outcome capture, and privacy controls are checked; routing filters, ranker, API, explanations, and training dataset remain open.

## Largest unimplemented or unverified areas

- Hyperliquid recorder and market-data quality operations.
- Perpetual trading ledger, fill model, fees/funding, margin, liquidation, and risk simulation.
- Trading-specific leakage, cost, execution, and robustness verifiers.
- Season/Arena state, robust ranking, appeals, and external launch metrics.
- Portable proof manifest, chain commitment adapter, verification CLI, and proof explorer.
- Minimal graph storage, reputation, review/replication, Sybil controls, and reward gates.
- Live shadow/testnet venue integration and all real-capital safeguards.

## Scope removed as MVP blockers

- Full specialist router completion.
- Full Graph OS and advanced graph analytics.
- Complete native training parity on every hardware profile.
- Paid marketplace checkout and rental flows.
- Custom blockchain and token.
- Real-capital trading.

## Required founder decision

Review and approve `PHASE-00`, then authorize `PHASE-00R`. No later phase should rely on the legacy tracker until the baseline audit is approved.
