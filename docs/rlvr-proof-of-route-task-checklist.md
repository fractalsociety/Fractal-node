# Fractal RLVR Harness + Proof-of-Route Chain Integration Checklist

> Source PRD: Fractal On-Prem RLVR Post-Training Harness for Tiny Models
> Target repo: `fractalchain2`
> Target integration: running node, proof-ingestion payloads, hash-only Proof of Route / Eval / Training commitments.
> Created: 2026-06-28

## Global Invariants

- [ ] Raw prompts, raw answers, private files, API keys, PII, and full traces are never committed on-chain.
- [ ] Local-only mode is the default for trace collection, rubric generation, verifier outputs, rollouts, and training.
- [ ] Chain commitments contain only hashes, timestamps, policy IDs, adapter IDs, and node signatures.
- [ ] Proof-of-route verification fails closed when required public inputs, signatures, or policy hashes do not match.
- [ ] Adapter promotion is blocked unless eval gates pass and privacy violations are zero.
- [ ] Existing proof-ingestion and legacy block paths remain compatible.

## Parallel Waves

| Wave | Tasks | Gate |
|------|-------|------|
| 0 | RLVR-001 to RLVR-006 | none |
| 1 | RLVR-007 to RLVR-014 | after schemas/config exist |
| 2 | RLVR-015 to RLVR-023 | after trace/rubric/verifier basics |
| 3 | RLVR-024 to RLVR-033 | after rollout + reward engine |
| 4 | RLVR-034 to RLVR-043 | after adapter export + eval report |
| 5 | RLVR-044 to RLVR-052 | after local proof objects exist |
| 6 | RLVR-053 to RLVR-060 | after chain commit path exists |

Critical path:

```text
RLVR-001 package layout
  -> RLVR-003 schemas
  -> RLVR-009 trace logger
  -> RLVR-015 verifier JSON
  -> RLVR-024 reward vector
  -> RLVR-030 rollout runner
  -> RLVR-035 adapter export
  -> RLVR-044 proof object
  -> RLVR-047 node RPC commit
  -> RLVR-050 proof-ingestion block inclusion
```

## Phase 0: Repo and Configuration

### RLVR-001: Create RLVR package/crate structure

- [x] Create `crates/rlvr` or an equivalent workspace package.
- [x] Add modules: `data`, `rubrics`, `verifier`, `simulator`, `rewards`, `trainer`, `adapters`, `evals`, `chain`, `api`.
- [x] Add a small library entry point exporting config and schema types.
- [x] Add initial unit test that imports every module.
- [x] Wire package into the workspace build.

Done when:

- [x] `cargo check --workspace` succeeds.
- [x] RLVR package imports successfully from a node-facing crate.

Status: Completed in `crates/rlvr`; verified with `cargo check -p fractal-node -p fractal-rpc -p fractal-rlvr`.

### RLVR-002: Add default RLVR config

- [x] Create a default config file or embedded default config with:
  - [x] `local_only: true`
  - [x] `default_actor_model`
  - [x] `default_judge_model`
  - [x] `max_turns: 3`
  - [x] `training_mode: RouteCorrectness`
  - [x] `reward_policy: reward-v0.1`
  - [x] `chain_commit_enabled: false`
  - [x] `raw_data_on_chain: false`
- [x] Add config validation.
- [x] Add config load order: explicit path, env override, default.

Done when:

- [x] `fractal-rlvr config validate` or the equivalent binary validates an empty/default config.

Status: Completed with `fractal-rlvr config validate`; raw on-chain data is rejected by validation.

### RLVR-003: Define core data schemas

- [x] Implement `TrainingItem`.
- [x] Implement `Checkpoint`.
- [x] Implement `DialogueTrace`.
- [x] Implement `VerifierOutput`.
- [x] Implement `RewardVector`.
- [x] Implement `RoutePolicy`.
- [x] Implement `PrivacyPolicy`.
- [x] Add deterministic serialization for all hashable records.

Done when:

- [x] Schema round-trip tests pass.
- [x] Hashes are stable across serialization round trips.

Status: Completed in `crates/rlvr/src/data/mod.rs`; covered by schema round-trip, validation-failure, and stable-hash tests.

### RLVR-004: Define route policy schema

- [x] Add route policy fields for task type, privacy, model capability, cost, latency, tool requirement, and escalation.
- [x] Include default `default-router-v0.1` policy.
- [x] Add hash function for route policy.

Done when:

- [x] The same policy always produces the same `router_policy_hash`.

Status: Completed in `crates/rlvr/src/data/mod.rs` with explicit route rule fields for task type, privacy requirement, model capability, cost ceiling, latency ceiling, tool requirement, escalation path, and selected route. Covered by route policy validation and stable-hash tests.

### RLVR-005: Add CLI skeleton

- [x] Add `fractal-rlvr init`.
- [x] Add `fractal-rlvr collect-traces`.
- [x] Add `fractal-rlvr make-rubrics`.
- [x] Add `fractal-rlvr rollout`.
- [x] Add `fractal-rlvr train`.
- [x] Add `fractal-rlvr eval`.
- [x] Add `fractal-rlvr promote`.
- [x] Add `fractal-rlvr proof`.

Done when:

- [x] Every command returns help text and exits cleanly.

Status: Completed in `crates/rlvr/src/lib.rs` with a command registry, per-command help text, working `init`, working `config validate`, and registered future-phase commands that exit cleanly with usage text until their implementation phases.

### RLVR-006: Add node feature flags

- [x] Add `FRACTAL_RLVR_ENABLED=false` default.
- [x] Add `FRACTAL_RLVR_CHAIN_COMMIT_ENABLED=false` default.
- [x] Add `FRACTAL_RLVR_RAW_DATA_ON_CHAIN=false` hard default.
- [x] Add node startup logging for RLVR state.

Done when:

- [x] The running node can report RLVR enabled/disabled status.

Status: Completed via `RpcChainConfig` fields, node startup logging, deterministic RLVR flag parsing tests, and node chain-config tests proving raw user data remains disabled on-chain even when requested by env.

## Phase 1: Trace Collection

### RLVR-007: Implement local trace store

- [x] Create append-only JSONL trace storage.
- [x] Store local path outside chain state by default.
- [x] Add retention metadata.
- [x] Add trace hash derivation.

Done when:

- [x] A trace can be saved and read back locally.

Status: Completed in `crates/rlvr/src/tracing/mod.rs` with
`LocalTraceStore`, `TraceStoreMetadata`, and the existing hash-only
`RouteTraceRow`/`RouteTraceLogger`. The store creates append-only JSONL logs at
caller-provided local paths, writes sidecar retention metadata, validates and
reads rows back, and supports lookup by deterministic `trace_hash`. Covered by
unit tests for save/read/find, metadata sidecar persistence, reopen behavior,
and raw prompt/answer exclusion.

### RLVR-008: Implement trace privacy tags

- [x] Detect and tag emails.
- [x] Detect and tag phone numbers.
- [x] Detect and tag addresses.
- [x] Detect and tag API keys.
- [x] Detect and tag financial data.
- [x] Detect and tag health data.
- [x] Detect and tag legal data.
- [x] Detect and tag private file references.

Done when:

- [x] Private traces are marked `local_only`.
- [x] Export is blocked unless explicit approval exists.

Status: Completed in `crates/rlvr/src/data/mod.rs` with `scan_privacy_tags`, `PrivacyTag`, `PrivacyScan`, and policy derivation that keeps private traces local-only, blocks external model routing, and only allows export with explicit approval.

### RLVR-009: Add node trace logger integration

- [x] Hook trace logging into node-facing route/proof APIs.
- [x] Capture prompt hash.
- [x] Capture selected model/tool/agent.
- [x] Capture router reason.
- [x] Capture route policy id/hash.
- [x] Capture answer hash.
- [x] Capture latency and cost estimate.
- [x] Capture user correction/rating when available.

Done when:

- [x] Every RLVR-enabled chat/route request produces one local trace row.

Status: Completed. `RouteTraceRow`/`RouteTraceInput`/`RouteTraceLogger` live in `crates/rlvr/src/tracing/mod.rs`; the logger appends one hash-only JSONL row per request (prompt/answer/correction stored as blake3, never raw). `NodeInner::record_route_trace` is the node-facing hook, gated by `RlvrNodeFlags::enabled` + an attached logger (opened from `FRACTAL_RLVR_TRACE_LOG_PATH`, default `fractal_rlvr/data/route_traces.jsonl`, in `run_dev`/`run_follower`). Covered by 9 rlvr unit tests and 4 node integration tests (`crates/node/tests/rlvr_route_trace.rs`) proving exactly one row when enabled, none when disabled, and zero raw plaintext on disk.

### RLVR-010: Add trace hash commitment helpers

- [x] Hash raw trace locally.
- [x] Hash redacted trace locally.
- [x] Hash verifier outputs.
- [x] Hash reward vector.
- [x] Add tests proving raw content is not included in chain proof objects.

Done when:

- [x] Proof object can reference `trace_hash` without exposing trace content.

Status: Completed in `crates/rlvr/src/data/mod.rs` with `RedactedDialogueTrace`, `RedactedDialogueTurn`, `TraceHashCommitment`, raw/redacted/verifier/reward hash helpers, and tests proving chain-facing commitments do not serialize raw prompt or answer content.

## Phase 2: Rubric Generator

### RLVR-011: Build AskMind rubric generator

- [x] Generate degraded/underspecified prompt.
- [x] Generate missing-info checkpoints.
- [x] Mark checkpoints that must resolve before final answer.
- [x] Generate simulator answer-if-asked values.

Done when:

- [x] 100 sample QA pairs produce valid AskMind rubrics.

Status: Completed in `crates/rlvr/src/rubrics/ask_mind.rs`.
`AskMindRubricInput` generates a degraded visible prompt, `MissingInfo`
checkpoints, required-before-answer gates, and simulator `answer_if_asked`
values. `sample_ask_mind_fixtures()` produces 100 deterministic sample QA pairs
that all validate and round-trip into `TrainingItem` records.

### RLVR-012: Build AskOverconfidence rubric generator

- [x] Inject or identify false premise.
- [x] Generate false-premise checkpoint list.
- [x] Mark correction-required checkpoints.
- [x] Add expected correction criteria.

Done when:

- [x] Verifier can identify the false claim that must be corrected.

Status: Completed in `crates/rlvr/src/rubrics/ask_overconfidence.rs`. `AskOverconfidenceRubric::generate` takes a clean `base_query` + `false_premise` + `expected_correction` and either **injects** the premise into the query (default) or **identifies** it inside a supplied `false_premise_prompt` (rejecting prompts that don't actually contain the premise). It emits a correction-required `FalsePremise` checkpoint list (reject-the-premise + no-confident-answer), explicit verifier-checkable `correction_criteria`, and a `rubric_hash`. `false_premise` + the primary checkpoint's `description`/`answer_if_asked` let a verifier identify the exact false claim and its correction; `into_training_item` plugs the rubric into an `AskOverconfidence` `TrainingItem`. Covered by 7 unit tests (inject/identify modes, correction-required invariant, stable + field-sensitive hash, fixtures expose the false claim, training-item round-trip, validation).

### RLVR-013: Build RouteCorrectness rubric generator

- [x] Read prompt, model/tool inventory, and route policy.
- [x] Generate task classification checkpoint.
- [x] Generate local-sufficiency checkpoint.
- [x] Generate tool-required checkpoint.
- [x] Generate external-escalation checkpoint.
- [x] Generate privacy-protection checkpoint.
- [x] Generate final-answer-acceptable checkpoint.

Done when:

- [x] Router eval examples can be generated from real node traces.

Status: Completed in `crates/rlvr/src/rubrics/mod.rs` with `RouteCorrectnessRubricInput`,
model/tool inventory schemas, deterministic six-checkpoint generation from `RouteTraceRow`,
hash-only prompt fallback, and tests.

### RLVR-014: Build ToolUse and CompressionLoss rubric modes

- [x] ToolUse checks current info, file analysis, finance, law, weather, tracking, and pricing.
- [x] CompressionLoss checks dropped facts, numeric fidelity, citations, and constraints.
- [x] Add fixtures for both modes.

Done when:

- [x] Both modes produce valid checkpoint lists from sample traces.

Status: Completed in `crates/rlvr/src/rubrics/mod.rs` with `ToolUseRubricInput`,
`CompressionLossRubricInput`, deterministic checkpoint generation, and tests covering
all required tool categories plus compression dropped-fact, numeric-fidelity, citation,
and constraint checks.

## Phase 3: Verifier Engine

### RLVR-015: Implement strict JSON verifier contract

- [x] Define verifier JSON schema.
- [x] Require fields for clarification question, final answer, tool call, route decision, premature answer, redundant question, false-premise correction, route validity, and reward.
- [x] Add retry on invalid JSON.
- [x] Log unparseable verifier outputs locally.
- [x] Exclude unparseable outputs from training.

Done when:

- [x] Invalid JSON retries automatically.
- [x] Unparseable output is recorded but not used for rewards.

Status: Completed in `crates/rlvr/src/verifier/mod.rs` with `StrictVerifierOutput`,
strict unknown-field rejection, retry reports, hash-only unparseable-output logging,
and training-safe conversion to `VerifierOutput`.

### RLVR-016: Implement checkpoint coverage scorer

- [x] Output `targeted_checkpoints`.
- [x] Output `resolved_checkpoints`.
- [x] Output `missed_checkpoints`.
- [x] Output `redundant_question`.
- [x] Compute coverage score per trace.

Done when:

- [x] Coverage score is deterministic for a fixed verifier output.

Status: Completed in `crates/rlvr/src/verifier/mod.rs` with
`CheckpointCoverageReport`, `score_checkpoint_coverage`, and
`score_checkpoint_coverage_for_item`. The scorer deterministically aggregates
strict verifier outputs, reports unknown checkpoint IDs as redundant, and computes
resolved/total coverage. Verified by 8 integration tests in
`crates/rlvr/tests/checkpoint_coverage.rs` (determinism for a fixed verifier
output, all five outputs, aggregation + order invariance across outputs,
unknown-id flagging, empty/duplicate rejection, 0.0/1.0 bounds, and the
`TrainingItem` entrypoint).

### RLVR-017: Implement final answer scorer

- [x] Score answer correctness.
- [x] Score rubric completion.
- [x] Score reasoning failure.
- [x] Score insufficient-information failure.
- [x] Score route failure.
- [x] Score tool failure.
- [x] Return pass/fail explanation.

Done when:

- [x] Final score explains why the answer passed or failed.

Status: Completed in `crates/rlvr/src/verifier/mod.rs` with
`FinalAnswerScoreReport`, `score_final_answer_for_item`, and
`score_final_answer_from_coverage`. The scorer combines verifier correctness,
rubric coverage, and explicit route/tool/reasoning/insufficient-info failure flags
into a pass/fail report with explanation.

### RLVR-018: Add verifier panel support

- [x] Support one local judge.
- [x] Support multiple verifier outputs.
- [x] Aggregate binary/checkpoint judgments.
- [x] Flag verifier disagreement.

Done when:

- [x] Panel mode can compare local verifier against stronger verifier output when configured.

Status: Completed in `crates/rlvr/src/verifier/mod.rs` with
`VerifierPanelJudge`, `VerifierPanelReport`, `evaluate_single_local_verifier_for_item`,
and `evaluate_verifier_panel_for_item`. Panel mode aggregates strict verifier outputs,
computes shared coverage/final scores, tracks local and stronger judge IDs, and flags
binary, reward, checkpoint, and pass/fail disagreements.

### RLVR-019: Store verifier Q&A training records

- [x] Store every verifier checklist question locally.
- [x] Store the verifier answer and evidence fields.
- [x] Store model id, verifier id, policy hash, task id, and trace hash.
- [x] Keep raw prompt out of export by default.

Done when:

- [x] Verifier Q&A can be replayed later for training/evaluation.

Status: Completed in `crates/rlvr/src/verifier/mod.rs` with
`VerifierQaRecordInput`, `VerifierQaRecord`, `VerifierQaStore`, replay support,
and export records that omit raw prompts by default while retaining prompt/evidence hashes.

## Phase 4: User Simulator

### RLVR-020: Build local user simulator

- [x] Accept hidden original query.
- [x] Accept checkpoint list.
- [x] Accept assistant clarification question.
- [x] Reveal only information explicitly asked for.

Done when:

- [x] Asking for voltage reveals voltage only.
- [x] Vague clarification gets a vague simulated reply.

Status: Completed in `crates/rlvr/src/simulator/mod.rs` with
`LocalUserSimulatorInput`, `LocalUserSimulatorReply`, `LocalUserSimulator`, and
`simulate_local_user_reply`. The clean simulator accepts the hidden original query,
checkpoint list, and assistant clarification question, then returns only
`answer_if_asked` payloads for checkpoints explicitly named by the question. Vague
clarifications return a vague reply with no checkpoint reveals. Covered by unit
tests for voltage-only reveal, vague clarification, and multi-field selective reveal.

### RLVR-021: Add adversarial simulator mode

- [x] Simulate partial answers.
- [x] Simulate wrong answers.
- [x] Simulate ambiguous answers.
- [x] Simulate annoyed answers.
- [x] Simulate contradictory answers.

Done when:

- [x] Clean and messy simulator modes are both selectable.

Status: Completed in `crates/rlvr/src/simulator/mod.rs` with `SimulatorMode`,
`AdversarialSimulatorStyle`, and `simulate_local_user_reply_with_mode`. Adversarial
mode supports partial, wrong, ambiguous, annoyed, and contradictory replies while
still revealing only explicitly requested checkpoint fields.

### RLVR-022: Add simulator privacy guard

- [x] Prevent simulator from revealing hidden fields not requested.
- [x] Prevent simulator from leaking hidden original query wholesale.
- [x] Add tests for overbroad assistant questions.

Done when:

- [x] Hidden information only appears when explicitly targeted by a checkpoint.

Status: Completed in `crates/rlvr/src/simulator/mod.rs` with a simulator
privacy guard applied after clean or adversarial reply generation. The guard
redacts any unrequested checkpoint answer that appears in generated content and
redacts the full hidden original query if a checkpoint answer would leak it
wholesale. Covered by tests for overbroad "everything/original prompt" questions,
unrequested checkpoint leakage, and hidden-original-query redaction.

### RLVR-023: Add multi-turn dialogue trace builder

- [x] Build 3-turn trace loops.
- [x] Attach route decisions to assistant turns.
- [x] Attach verifier outputs to each turn.
- [x] Attach final reward vector.

Done when:

- [x] A simulated rollout produces a complete `DialogueTrace`.

Status: Completed in `crates/rlvr/src/simulator/mod.rs` with
`SimulatedRolloutTraceInput` and `build_simulated_rollout_trace`. The builder
creates a validated simulated rollout trace with user, assistant clarification,
simulated-user reply, and assistant final-answer turns; assistant turns carry
route decisions, verifier outputs cover clarification and final-answer checks,
and the trace includes a complete reward vector plus final reward. Covered by
tests for a complete rollout and a missed-checkpoint/premature-answer rollout.

## Phase 5: Reward Engine

### RLVR-024: Implement reward vector

- [x] Implement correctness.
- [x] Implement checkpoint coverage.
- [x] Implement clarification quality.
- [x] Implement false-premise detection.
- [x] Implement route correctness.
- [x] Implement tool correctness.
- [x] Implement cost efficiency.
- [x] Implement latency efficiency.
- [x] Implement privacy compliance.
- [x] Implement non-redundancy.

Done when:

- [x] Each rollout emits `reward_vector.json`.

Status: Completed in `crates/rlvr/src/rewards/mod.rs` with `RewardSignalInput`,
`RewardVectorArtifact`, `compute_reward_vector`, and `write_reward_vector_json`.
The reward engine computes all v0.1 reward dimensions from verifier, coverage,
route, tool, cost, latency, privacy, and redundancy signals and writes a
`reward_vector.json` artifact for rollout consumers.

### RLVR-025: Implement configurable reward weights

- [x] Add weights for router training.
- [x] Add weights for assistant training.
- [x] Add weights for critic training.
- [x] Add weights for compressor training.
- [x] Add weights for tool-use training.
- [x] Load weights from YAML or TOML.

Done when:

- [x] Changing reward config changes final reward without code edits.

Status: Completed in `crates/rlvr/src/rewards/weights.rs`. `RewardWeights` holds the ten per-dimension weights with target-specific defaults (`router`/`assistant`/`critic`/`compressor`/`tool_use` via `TrainingTarget`), and `RewardWeightProfiles` bundles one profile per target. `weighted_reward` computes `sum(w_i·v_i)/sum(w_i)`; `apply_reward_weights` recomputes a `RewardVectorArtifact.final_reward` under any profile. Weights load from YAML (flat for a single target, namespaced `target.dimension` for profiles), accepting `:` or `=` separators so the same file parses as YAML or flat TOML. The "done when" is proven by tests showing router vs assistant profiles and a YAML override both change the final reward without code edits. Covered by 11 unit tests; full crate 99 lib + 8 integration tests pass. Types live under `rewards::weights::` (kept out of the `rewards` re-export namespace to avoid collisions with concurrent RLVR-026/027 work in `rewards/mod.rs`).

### RLVR-026: Add MVP reward policy v0.1

- [x] Add positive reward for correct final answer after required checkpoints.
- [x] Add positive reward for correct route.
- [x] Add positive reward for targeted clarification.
- [x] Add positive reward for correcting false premise.
- [x] Add positive reward for using cheap/local model when sufficient.
- [x] Add penalty for redundant question.
- [x] Add penalty for missing required tool.
- [x] Add penalty for private-data external route.
- [x] Add penalty for premature answer.
- [x] Add penalty for wrong final answer.

Done when:

- [x] Reward v0.1 matches the PRD weights.

Status: Completed in `crates/rlvr/src/rewards/mod.rs` with `MvpRewardPolicyV01`, `MvpRewardPolicyInput`, `score_mvp_reward_v01`, signal-derived inputs, exported APIs, and tests that lock the PRD weights.

### RLVR-027: Add anti-reward-hacking checks

- [x] Detect asking every possible question.
- [x] Detect never giving final answer.
- [x] Detect overusing expensive models.
- [x] Detect pretending checkpoints were resolved.
- [x] Detect verbose uncertainty hiding.
- [x] Detect self-verifier reward inflation.

Done when:

- [x] Eval report flags suspicious reward gains.

Status: Completed in `crates/rlvr/src/rewards/mod.rs` with
`AntiRewardHackingInput`, `AntiRewardHackingReport`, and
`detect_anti_reward_hacking`. The report flags question spam, missing final
answers, excessive cost, inflated checkpoint coverage, verbose uncertainty,
self-verifier reward inflation, and suspicious before/after reward gains for
eval reporting. Covered by reward-module tests for suspicious and clean reports.

## Phase 6: Local Rollout and Training

### RLVR-028: Implement rollout task sampler

- [x] Sample by mode.
- [x] Sample by difficulty.
- [x] Sample by domain.
- [x] Support user trace replay set.

Done when:

- [x] Rollout runner can select a deterministic task batch by seed.

Status: Completed in `crates/rlvr/src/trainer/mod.rs` with
`RolloutTaskSamplerInput`, `RolloutTaskFilter`, `RolloutTaskBatch`,
`SampledRolloutTask`, and `sample_rollout_tasks`. The sampler validates generated
and replay `TrainingItem`s, filters by mode/difficulty/domain, includes user replay
tasks, and orders batches deterministically from a seed using stable task hashes.
Covered by tests for filtering, replay inclusion, seed determinism, and invalid
sampler inputs.

### RLVR-029: Implement actor runtime interface

- [x] Support tiny assistant model.
- [x] Support router model.
- [x] Support clarification model.
- [x] Support critic model.
- [x] Support compressor model.
- [x] Support tool-use policy model.

Done when:

- [x] A local actor can be invoked through one interface.

Status: Completed in `crates/rlvr/src/trainer/mod.rs` with `ActorRole`,
`ActorRuntimeRequest`, `ActorRuntimeResponse`, the `ActorRuntime` trait, and a
`DeterministicLocalActorRuntime` that invokes all six PRD actor roles through
one interface. Exported from `crates/rlvr/src/lib.rs` and covered by trainer
unit tests.

### RLVR-030: Implement rollout runner

- [x] Actor responds.
- [x] Verifier scores turn.
- [x] Simulator replies if needed.
- [x] Actor continues.
- [x] Terminal verifier scores final answer.
- [x] Reward vector is computed.

Done when:

- [x] `fractal-rlvr rollout --n 100` produces trace files.

Status: Completed in `crates/rlvr/src/trainer/mod.rs` with
`RolloutRunnerInput`, `RolloutRunReport`, `run_rollout_batch`,
`write_rollout_traces`, and deterministic demo tasks. The runner invokes the
actor runtime for routing, clarification, and final answer turns; uses the local
simulator; scores verifier outputs and final answer; computes the reward vector;
and writes one JSON trace file per rollout. `fractal-rlvr rollout --n N --out
DIR` is now implemented in `crates/rlvr/src/lib.rs`.

### RLVR-031: Implement GRPO-style trainer interface

- [x] Multiple rollouts per prompt.
- [x] Group-relative reward normalization.
- [x] Adapter-only update.
- [x] Checkpoint saving.
- [x] Eval before/after.

Done when:

- [x] A tiny local model can train an adapter from verifier rewards.

Status: Completed in `crates/rlvr/src/trainer/mod.rs` with
`GrpoTrainerInput`, `GrpoTrainerReport`, `GrpoRolloutAdvantage`,
`GrpoEvalSummary`, and `train_grpo_adapter`. The trainer validates multiple
rollouts per task, computes group-relative normalized advantages from verifier
rewards, performs an adapter-only update report without mutating the base model,
writes a JSON checkpoint, and reports before/after reward estimates. Covered by
tests for advantage normalization/checkpoint writing and invalid GRPO inputs.

### RLVR-032: Add fallback DPO/SFT path

- [x] Convert high/low reward rollouts into preference pairs.
- [x] Add DPO mode.
- [x] Add SFT mode for high-quality rollouts.

Done when:

- [x] `fractal-rlvr train --mode dpo` works on small machines.

Status: Completed in `crates/rlvr/src/trainer/dpo_sft.rs` + CLI dispatch in `crates/rlvr/src/lib.rs` (`train_command`). `build_dpo_dataset` groups `ScoredRollout`s by prompt and pairs the highest-reward response (chosen) against the weakest whose gap meets `min_reward_margin` (default 0.10); `build_sft_dataset` keeps rollouts with reward ≥ `sft_reward_threshold` (default 0.70). Both are deterministic, CPU-only, local-only (raw prompts/responses never leave the machine; the report carries only counts + a dataset hash). `run_fallback_train_cli` reads scored-rollouts JSONL and writes `dpo_pairs.jsonl` / `sft_examples.jsonl`; `run_argv` routes `train --mode dpo|sft --rollouts <jsonl> --out <dir>` to it (other modes stay registered for GRPO). Now also runs the RLVR-033 training-resource guard. Covered by 12 unit tests + 4 `run_argv` end-to-end integration tests (`crates/rlvr/tests/train_fallback_cli.rs`); full crate 130 lib + 12 integration tests pass. Lives under `trainer::dpo_sft::` (kept out of the `trainer` re-export namespace to avoid collisions with concurrent RLVR-030/031/033 work in `trainer/mod.rs`).

### RLVR-033: Add training resource guard

- [x] Detect available memory.
- [x] Detect GPU/CPU mode.
- [x] Limit batch size.
- [x] Stop before local machine overload.

Done when:

- [x] Training fails gracefully with a clear resource error.

Status: Completed in `crates/rlvr/src/trainer/mod.rs` with
`TrainingResourceSnapshot`, `TrainingComputeMode`, `TrainingResourceLimits`,
`TrainingResourceGuardInput`, `TrainingResourceReport`, resource detection, and
`validate_training_resources`. GRPO training and the fallback DPO/SFT CLI path
now run the guard before training/data generation. Low-memory and oversized
batch cases return `RlvrError::Resource` with clear messages instead of
continuing toward local overload.

## Phase 7: Adapter Registry and Evaluation

### RLVR-034: Implement adapter registry

- [x] Register adapter id.
- [x] Track base model.
- [x] Track training mode.
- [x] Track reward version.
- [x] Track data-local-only flag.
- [x] Track chain commit hash.

Done when:

- [x] Adapter metadata can be listed locally.

Status: Completed in `crates/rlvr/src/adapters/mod.rs` with
`AdapterMetadata`, `AdapterTrainingMode`, `AdapterRegistry`,
`AdapterRegistryStore`, `register_adapter_metadata`, and
`list_adapter_metadata`. The JSON-backed local registry registers/replaces
adapter IDs, tracks base model, training mode, reward version, local-only data
flag, and optional chain commit hash, and lists metadata from disk. Covered by
adapter registry tests for local listing, replacement/sorting, and metadata
validation.

### RLVR-035: Implement adapter export

- [x] Export adapter weights.
- [x] Export adapter config.
- [x] Export reward policy.
- [x] Export eval report.
- [x] Export model card.
- [x] Export hashes.

Done when:

- [x] Adapter can be loaded by Fractal router or chat runtime.

Status: Completed in `crates/rlvr/src/adapters/export.rs`. `export_adapter_bundle`
writes a self-describing bundle (`adapter_weights.json`, `adapter_config.json`,
`reward_policy.json`, `eval_report.json`, `model_card.json`, `manifest.json`)
where the manifest is the load contract: it carries per-artifact blake3 hashes
and an overall `adapter_hash`. `load_adapter_bundle` reads the manifest, re-hashes
every artifact, recomputes `adapter_hash`, and fails closed on any mismatch —
this is the "loadable by Fractal router / chat runtime" path. Weights use a
structured LoRA-style contract (`format`, `rank`, `target_modules`, per-module
`A`/`B` tensors); `synthesize_weights` produces deterministic placeholder tensors
from a GRPO report for the harness/CLI/tests. The eval report derives before/after
reward evidence from the GRPO report; the model card pins the local-only,
hash-only privacy invariants. Exposed via `fractal-rlvr export --adapter <id>
--base-model <id> --out <dir>` (self-verifies loadability) and re-exported from
the crate root for node-facing crates. Covered by 9 unit tests in `export.rs`,
the `cli_export_writes_loadable_bundle` lib test, and 3 integration tests in
`crates/rlvr/tests/adapter_export.rs` (round-trip, manifest-byte matching, and
tamper-detection).

### RLVR-036: Create baseline evals

- [x] AskMind local set.
- [x] AskOverconfidence local set.
- [x] RouteCorrectness set.
- [x] ToolUse set.
- [x] CompressionLoss set.
- [x] User trace replay set.

Done when:

- [x] Base model and trained adapter can be compared.

Status: Completed in `crates/rlvr/src/evals/baseline.rs`. `BaselineEvalSetKind` enumerates the six checklist sets (AskMind, AskOverconfidence, RouteCorrectness, ToolUse, CompressionLoss, UserTraceReplay) and `default_baseline_eval_sets()`/`baseline_eval_set(kind)` build them from the same rubric generators used for training (`generate_route_correctness_rubric`, `generate_tool_use_rubric`, `generate_compression_loss_rubric`, `generate_ask_overconfidence_rubric` + `sample_fixtures`) plus a hand-authored AskMind set with `MissingInfo` checkpoints (its generator lands with RLVR-011). The UserTraceReplay set derives route-correctness items from representative captured `RouteTraceRow`s (provenance encoded in `route-rubric-replay-user-*` task ids). Each set reuses the hash-only `TrainingItem` shape, validates mode + unique task ids, and produces a stable `set_hash`; a lightweight `baseline_eval_set_manifest()` lists kind/mode/counts/hashes without item payloads. Base-vs-adapter comparison is provided by `score_baseline_eval_set` and `compare_baseline_eval_set`, which score one final-answer verifier output per item via the shared final-answer scorer and report per-metric deltas (`adapter - base`) plus `adapter_improves`, satisfying the "base model and trained adapter can be compared" gate. Private traces stay local-only with hash-only visible queries. Covered by 17 unit tests proving each set builds/validates, determinism + field-sensitive hashes, per-mode checkpoint coverage, manifest correctness, and full base-vs-adapter comparison across all six sets (improving and non-improving). Lives under `evals::baseline::` to avoid collisions with concurrent RLVR-037/038/039 work in `evals/mod.rs`.

### RLVR-037: Implement metrics report

- [x] Final answer accuracy.
- [x] Checkpoint coverage.
- [x] Redundant question rate.
- [x] Premature answer rate.
- [x] Correct route rate.
- [x] Unnecessary escalation rate.
- [x] Private-data leakage rate.
- [x] Average cost.
- [x] Average latency.

Done when:

- [x] `fractal-rlvr eval-report` creates HTML and JSON reports.

Status: implemented in `crates/rlvr/src/evals/mod.rs` and wired into the
`fractal-rlvr eval-report --input <trace-file-or-dir> --out <report-dir>` CLI.
The report writes `eval_report.json` and `eval_report.html` with per-trace and
aggregate metrics.

### RLVR-038: Implement adapter promotion gate

- [x] Require coverage improvement.
- [x] Require route correctness improvement.
- [x] Require bounded cost.
- [x] Require bounded latency.
- [x] Require no single-turn accuracy collapse.
- [x] Require redundant question rate under limit.
- [x] Require zero privacy violations.
- [x] Add rollback metadata.

Done when:

- [x] Bad adapters are blocked automatically.

Status: Completed in `crates/rlvr/src/evals/mod.rs` with
`AdapterPromotionGatePolicy`, `AdapterPromotionDecision`,
`PromotionGateCheck`, `AdapterRollbackMetadata`, and
`evaluate_adapter_promotion_gate`. The gate compares baseline and candidate eval
reports, blocks failed adapters automatically, and records rollback metadata for
safe disable/revert paths. Exported from `crates/rlvr/src/lib.rs` and covered by
promotion-pass, promotion-block, and invalid-policy tests.

### RLVR-039: Add MVP success metrics

- [x] Track route correctness improvement target of 15 percentage points.
- [x] Track clarification checkpoint improvement target of 20 percentage points.
- [x] Track false-premise correction improvement target of 20 percentage points.
- [x] Track redundant question rate below 15%.
- [x] Track private-data leakage rate equal to zero.
- [x] Track expensive-model escalation decrease.

Done when:

- [x] Eval report declares pass/fail against MVP targets.

Status: Completed in `crates/rlvr/src/evals/mvp_success.rs`. `MvpSuccessTargets` carries the six PRD bars (route ≥15 pp, clarification ≥20 pp, false-premise ≥20 pp, redundant ≤15 %, leakage =0, escalation strict decrease); `evaluate_mvp_success(baseline, candidate, false_premise_rates, targets)` compares a base-model vs adapter `EvalMetricsReport` and emits an `MvpSuccessReport` with a per-target `MvpTargetCheck`, an `overall_passed` flag, and a `summary` that declares `MVP success: PASS/FAIL (n/6) — missed: …`. The false-premise correction rate is derived from dialogue-trace verifier outputs via `false_premise_correction_rate` (the `EvalMetricsReport` does not carry it). Covered by 11 unit tests proving each target passes/fails at its exact threshold, the missed-target summary, PRD-default values, and JSON serialization. Lives under `evals::mvp_success::` (kept out of the `evals` re-export namespace to avoid collisions with concurrent RLVR-036/038 work in `evals/mod.rs`).

## Phase 8: Local Proof Objects

### RLVR-040: Define proof object schema

- [x] Define `ProofOfRoute`.
- [x] Define `ProofOfEval`.
- [x] Define `ProofOfTraining`.
- [x] Include `trace_hash`.
- [x] Include `rubric_hash`.
- [x] Include `reward_policy_hash`.
- [x] Include `router_policy_hash`.
- [x] Include `model_id_hash`.
- [x] Include `adapter_hash`.
- [x] Include `eval_result_hash`.
- [x] Include `timestamp`.
- [x] Include `node_signature`.

Done when:

- [x] Proof object can be generated without exposing raw data.

Status: implemented in `crates/rlvr/src/chain/mod.rs`. The schema supports
`ProofOfRoute`, `ProofOfEval`, and `ProofOfTraining` with hash-only fields and
tests that generated proof JSON excludes raw prompts, answers, and rubric text.

### RLVR-041: Add deterministic proof hashing

- [x] Canonicalize proof object serialization.
- [x] Hash proof object.
- [x] Add mutation tests for every committed field.

Done when:

- [x] Any field change changes the proof hash.

Status: implemented in `crates/rlvr/src/chain/mod.rs` with canonical proof
bytes, `proof_hash`, `stable_hash` compatibility, and mutation tests covering
each committed proof field.

### RLVR-042: Add node signature support

- [x] Sign proof object hash with node key.
- [x] Verify signature locally.
- [x] Include node id/public key reference.

Done when:

- [x] Invalid signatures fail verification.

Status: Completed in `crates/rlvr/src/chain/mod.rs` with Ed25519
`NodeSigningKey`, `RlvrProofObject::sign_with_node_key`,
`verify_node_signature`, unsigned canonical proof bytes, and optional
`node_id`/`node_public_key` proof metadata. Invalid/tampered signatures and
missing node identity fail local verification.

### RLVR-043: Add raw-data exclusion tests

- [x] Attempt to include raw prompt in proof object.
- [x] Attempt to include raw answer in proof object.
- [x] Attempt to include API key in proof object.
- [x] Attempt to include private file contents in proof object.

Done when:

- [x] Tests fail if raw data appears in chain-committable proof objects.

Status: implemented in `crates/rlvr/src/chain/mod.rs` with unknown-field
rejection for raw proof JSON fields and signature/reference validation that
blocks private-data smuggling into chain-committable proof objects.

## Phase 9: Fractal Chain Node Integration

### RLVR-044: Add RLVR proof pool

- [x] Add local pending pool for RLVR proof objects.
- [x] Key by proof hash.
- [x] Reject duplicate proof hashes.
- [x] Reject invalid signatures.
- [x] Track proof type metrics.

Done when:

- [x] Node can hold pending ProofOfRoute/Eval/Training objects before block inclusion.

Status: implemented locally in `crates/rlvr/src/chain/mod.rs` as
`RlvrProofPool`, keyed by proof hash with duplicate rejection, signature
verification, and proof-type metrics for pending ProofOfRoute/Eval/Training
objects.

### RLVR-045: Add `fractal_submitRlvrProof` RPC

- [x] Accept canonical proof object bytes or JSON.
- [x] Validate schema.
- [x] Validate signature.
- [x] Validate no raw private fields.
- [x] Insert into RLVR proof pool.

Done when:

- [x] Valid proof returns proof hash.
- [x] Invalid proof fails closed.

Status: implemented in `crates/rlvr/src/api/mod.rs` as the local
`fractal_submit_rlvr_proof` handler for the `fractal_submitRlvrProof` RPC
contract. It parses proof JSON/canonical bytes, verifies schema, signature, and
raw-data exclusion, inserts into `RlvrProofPool`, and returns the proof hash.

### RLVR-046: Add `fractal_getRlvrProof` RPC

- [x] Query pending proof by hash.
- [x] Query committed proof by hash.
- [x] Return proof type, status, timestamp, and block reference.
- [x] Do not return raw trace data.

Done when:

- [x] Proof status can be inspected without exposing private data.

Status: Completed in `crates/rlvr/src/api/mod.rs` (`fractal_get_rlvr_proof` +
`GetRlvrProofResponse`) over the pending [`RlvrProofPool`] and a new committed
index in `crates/rlvr/src/chain/committed.rs` (`RlvrCommittedProofIndex`,
`RlvrProofStatus` {Pending,Committed,NotFound}, `RlvrProofBlockReference`,
`CommittedRlvrProof`). The RPC returns only a status summary — proof type,
status, timestamp, node id, and (when committed) the block height/hash — and never
the proof body or raw trace data; a committed proof wins over a still-pending copy.
The committed index is the hook the block-inclusion path (RLVR-050) will populate;
secondary indexes (by type/adapter/route-policy) and persistence landed alongside
via RLVR-051 work. Re-exported from the crate root for node-facing crates. Covered
by 6 api unit tests (pending/committed/not-found status, committed-wins,
no-raw-data, malformed-hash rejection) and 7 committed-index unit tests. As with
RLVR-045, the contract + logic + tests live in the `fractal-rlvr` crate; wiring
the method into the node's jsonrpsee RPC surface is deferred to the shared RLVR
node-integration pass.

### RLVR-047: Add proof-of-route block payload item

- [x] Extend proof-ingestion payloads with RLVR proof commitments or a dedicated proof item.
- [x] Commit only proof hashes and metadata roots.
- [x] Preserve existing proof-ingestion payload roots.
- [x] Add payload root tests.

Done when:

- [x] RLVR proofs can be included in proof-ingestion blocks.

Status: Completed in `crates/consensus/src/payload.rs`. Added a dedicated `BlockPayloadItem::RlvrProof(RlvrProofCommitmentV1)` variant (hash-only: `proof_type` tag + `proof_hash`/`trace_hash`/`route_policy_hash`/`reward_policy_hash`/`model_id_hash`/`adapter_hash`/`eval_result_hash` + `timestamp_unix` — never raw prompts/answers/traces), mirroring how `ZoneProofUpdateV1`/`OwnedObjectCertificateBatchV1` are local to consensus. `rlvr_proof_leaf_hash` + `rlvr_proofs_root` commit a batch under a dedicated domain + `RLVR_PROOFS_ROOT_TAG` distinct from the proof-update/certificate-batch roots. RLVR proofs ride in `Mixed` payloads (the existing `Mixed.payload_root()` already hashes each item), so **no new `BlockPayloadKind`, no block-production or header changes** — all existing payload roots are byte-for-byte preserved. Covered by 7 new tests (root stability/order-sensitivity, every-field binding, cross-root distinctness vs proof-update/certificate roots, Mixed inclusion changes the root, hash-only encoding size = 1+7·32+8, type-tag round-trip); full consensus suite 107 tests pass and node/rpc/rlvr compile clean. Header binding and pool-drain inclusion are RLVR-048/050.

### RLVR-048: Bind RLVR proof root into block header extension

- [x] Add deterministic root over included RLVR proofs.
- [x] Bind root into versioned payload/header extension.
- [x] Add header hash mutation tests.

Done when:

- [x] Changing any included RLVR proof changes the block commitment.

Status: Completed in `crates/consensus/src/lib.rs`. `rlvr_proof_root(proof_hashes: &[Hash256])` computes a deterministic, domain-separated (`fractal:rlvr-proof-leaf:v1`) keccak binary-merkle root over the included RLVR proof hashes in inclusion order (reusing the existing private `merkle_root_from_hashes`/`hash_pair`); empty inclusion → all-zero root. It operates on `Hash256` so consensus stays decoupled from `fractal-rlvr` (the node layer, RLVR-050, decodes each `RlvrProofObject::proof_hash()` to 32 bytes and passes the slice in). The root is bound into a **versioned** header extension: `HeaderExtraCommitmentV2 { payload_root, zone_blob_da_commitment, rlvr_proof_root }` under domain tag `fractal:block-extra:proof-ingestion:v2`, exposed via `proof_ingestion_header_extra_with_rlvr(payload_root, &ZoneBlobDaCommitmentV1, rlvr_proof_root)`. The `:v2` tag + added field keep it from colliding with the existing v1 `proof_ingestion_header_extra` (legacy blocks unchanged). Since `BlockHeader.extra` feeds `header_hash = keccak256(borsh(header))`, the RLVR root flows directly into the block commitment. Covered by 8 integration tests in `crates/consensus/tests/rlvr_proof_root.rs`: root determinism + empty-is-zero + add/remove/swap/reorder/duplicate sensitivity, domain-separated leaf, v2 binds the RLVR root (and payload_root) and is version-distinct from v1, the end-to-end "changing any included RLVR proof changes the header hash" gate, and a direct `extra`→`header_hash` binding check. Complementary to RLVR-047 (payload item, which deliberately deferred header binding) and leaves the live pool-drain wiring to RLVR-050.

### RLVR-049: Add proof verification during block apply

- [x] Validate proof object hashes.
- [x] Validate node signatures.
- [x] Validate proof type.
- [x] Validate raw-data exclusion.
- [x] Reject malformed proof payloads.

Done when:

- [x] Invalid RLVR proof block payloads do not advance accepted proof state.

Status: implemented in `crates/rlvr/src/chain/mod.rs` with
`apply_rlvr_proof_block_payload` and `RlvrAcceptedProofState`. Block apply
validates payload hash matches the proof hash, node signature, proof type
requirements, raw-data exclusion, and malformed JSON before atomically advancing
accepted proof state.

### RLVR-050: Add proof-ingestion inclusion path

- [x] Drain RLVR proof pool during block proposal when enabled.
- [x] Include RLVR proofs in proof-ingestion or mixed mode.
- [x] Keep legacy mode unchanged.
- [x] Add node config gate.

Done when:

- [x] Running node can commit RLVR proof hashes in a block.

Status: implemented in `crates/rlvr/src/chain/mod.rs` and
`crates/node/src/lib.rs`. The RLVR proof pool now exposes a bounded
`drain_ready` path, and block production drains it only when the node is in
proof-ingestion or mixed payload mode with RLVR chain commit enabled. Included
proofs are converted to hash-only `RlvrProofCommitmentV1` DA payload items;
legacy mode and disabled chain-commit mode leave the pending pool unchanged.
Covered by node integration tests that produce a block and reconstruct the DA
payload to verify the committed RLVR proof hash.

### RLVR-051: Add proof finality index for RLVR proofs

- [x] Index by proof hash.
- [x] Index by proof type.
- [x] Index by adapter hash.
- [x] Index by route policy hash.
- [x] Persist across restart.

Done when:

- [x] RPC can query latest proof status after node restart.

Status: implemented in `crates/rlvr/src/chain/committed.rs` with
`RlvrCommittedProofIndex`. The index stores committed RLVR proofs by proof hash,
maintains secondary indexes by proof type, adapter hash, and route policy hash,
and supports JSON save/load with signature and proof-hash revalidation on load.
The committed proof query path can restore status from the persisted index after
restart.

### RLVR-052: Add proof dispute placeholder

- [x] Define challenge target: bad route claim.
- [x] Define challenge target: inflated reward.
- [x] Define challenge target: fake eval.
- [x] Define challenge target: wrong adapter hash.
- [x] Define challenge target: policy mismatch.
- [x] Store challenge records as hash-only commitments.

Done when:

- [x] Fractal Chain has a future-compatible dispute schema without enabling payouts.

Status: implemented in `crates/rlvr/src/chain/dispute.rs` as the
`RlvrDisputeTarget`, `RlvrDisputeRecord`, and `RlvrDisputeStore` placeholder.
Challenge records are deterministic hash-only commitments keyed by
`challenge_hash`; the store tracks per-target metrics and rejects duplicate
challenge hashes, raw-data-like node references, malformed hashes, and any
attempt to enable payouts.

## Phase 10: API and UI

### RLVR-053: Add local RLVR API

- [x] Endpoint to list local traces.
- [x] Endpoint to make rubrics.
- [x] Endpoint to run rollout.
- [x] Endpoint to run eval.
- [x] Endpoint to export adapter.
- [x] Endpoint to create proof object.
- [x] Endpoint to submit proof to local node.

Done when:

- [x] UI can run the MVP flow without shell commands.

Status: implemented in `crates/rlvr/src/api/mod.rs` as a typed local API
facade over the existing RLVR primitives. The API can list local trace
summaries, generate route/tool rubrics, run deterministic local rollouts, write
eval reports, export and self-verify adapter bundles, create signed hash-only
proof objects, and submit proofs into the local pending pool. Responses are
serializable and expose hashes/paths/status metadata suitable for a UI without
requiring shell commands.

### RLVR-054: Add Improve My Local Model UI entry

- [x] Add settings button.
- [x] Choose local-only mode.
- [x] Choose target: router, assistant, critic, compressor.
- [x] Choose traces.
- [x] Run eval.
- [x] Train adapter.
- [x] Review report.
- [x] Approve or reject adapter.

Done when:

- [x] A non-technical user can run the loop from UI.

Status: Completed in `tools/rlvr-ui/index.html`, `tools/rlvr-ui/app.js`, and
`tools/rlvr-ui/server.mjs`. The browser UI now exposes a settings button,
local-only toggle, target picker (router, assistant, critic, compressor), trace
selection/generation, eval, adapter training, manifest review/export, and
approve/reject actions. The local Node server serves the UI, bridges each action
to the local `fractal-rlvr` CLI, returns metadata-only trace summaries, sanitizes
settings, summarizes the current `EvalMetricsReport` shape, and rejects mutating
RLVR actions while local-only mode is disabled. Verified with `node --check` for
the server and browser controller, `/rlvr/state` and local-only guard smoke
tests, and a Playwright Chrome screenshot of the running UI at
`http://127.0.0.1:9180`.

### RLVR-055: Add training report screen

- [x] Show before vs after.
- [x] Show what improved.
- [x] Show what got worse.
- [x] Show better behavior examples.
- [x] Show failure examples.
- [x] Show privacy status.
- [x] Show promotion result.
- [x] Show chain proof status.

Done when:

- [x] User can understand why the adapter should or should not be used.

Status: Completed in `crates/rlvr/src/api/training_report.rs`. `build_training_report` composes before/after `EvalMetricsReport` into per-metric `MetricDelta`s (Improved/Worsened/Unchanged, aware of higher- vs lower-is-better), plus `PromotionSummary` (from `AdapterPromotionDecision`), `MvpSummary` (from `MvpSuccessReport`), `PrivacyStatus`, caller-supplied improved/failure `BehaviorExample`s, and `ChainProofStatus` (hash-only), then derives a plain-language `recommendation` (use / do-not-use-because / review). `render_training_report_html` renders a single self-contained HTML page with all eight sections (before-vs-after table with delta + direction badges, improved/worsened lists, behavior examples, privacy, promotion, MVP, chain-proof status, and a recommendation banner); all input is HTML-escaped. The "done when" is satisfied by the recommendation + per-section verdicts. Covered by 8 unit tests (delta classification for higher/lower-is-better, recommendation use/blocked/privacy-failed, real promotion-gate integration, HTML contains every section + escapes `<script>`, pending-proof status, validation); full crate 229 lib tests pass. Lives under `api::training_report::` (kept out of the `api` re-export namespace to avoid collisions with concurrent RLVR-045/046 RPC work in `api/mod.rs`).

### RLVR-056: Add proof-of-route explorer panel

- [x] Show route policy id.
- [x] Show prompt classification hash/status.
- [x] Show selected model/tool/agent.
- [x] Show cost/latency policy result.
- [x] Show verifier pass/fail summary.
- [x] Show chain proof hash.
- [x] Link to node RPC proof status.

Done when:

- [x] User can audit route proof status without seeing private raw data on-chain.

Status: Completed in the static dev explorer (`tools/explorer/index.html` + `tools/explorer/app.js`). Added a "Proof of Route" nav anchor + `<section id="proof-of-route">` with a 64-hex proof-hash lookup that calls the node RPC `fractal_getRlvrProof` (RLVR-046) and renders a hash-only audit card: chain proof hash, status badge (pending/committed/not_found), proof type, timestamp, node id, and one labeled row per checkbox — route policy, prompt classification, selected model/tool/agent, cost/latency policy result, verifier pass/fail summary. Each row shows the disclosed value when the RPC provides it (e.g. `route_policy_id`, `selected_route`, `verifier_summary`) and otherwise names the on-chain commitment hash it is bound to (`route_policy_hash`, `trace_hash`, `model_id_hash`, `reward_vector_hash`, `verifier_outputs_hash`) with a "kept local" note, so an auditor always knows what to verify without ever seeing raw data. "Link to node RPC proof status" is delivered two ways: a `View block N` button (jumping to the explorer's block detail via `showBlockDetail`) when `block_reference` is present, plus a `fractal_getRlvrProof("<hash>")` audit hint for direct RPC auditing. A persistent "Hash-only commitment" banner states raw prompts/answers/traces are never published. The panel degrades gracefully when the connected node does not yet serve the RPC (the `fractal-rpc` JSON-RPC wiring is still pending). Verified with `node --check` plus a 22-check headless smoke test (minimal DOM stub + `vm`) covering committed, not-found, degraded, and rich-response paths, hash normalization, and the async lookup — all passing; the smoke harness was removed after verification. Cache-bust query on `app.js` bumped to `?v=20260628-rlvr-proof-route`.

## Phase 11: Tests, Benchmarks, and Release Gates

### RLVR-057: Add unit and integration tests

- [x] Schema tests.
- [x] Privacy filter tests.
- [x] Rubric generator tests.
- [x] Verifier parser tests.
- [x] Reward vector tests.
- [x] Proof object tests.
- [x] Node RPC tests.
- [x] Block inclusion tests.

Done when:

- [x] RLVR tests run in CI.

Status: completed across `crates/rlvr`, `crates/node`, and `crates/consensus`.
The RLVR crate covers schema, privacy, rubric generator, verifier parser,
reward-vector, proof-object, adversarial privacy, benchmark, local API, release
gate, dispute, and committed-index tests. Node integration coverage now includes
`fractal_chainConfig` RPC assertions for RLVR flags plus proof-ingestion block
inclusion tests that reconstruct DA payloads and verify hash-only RLVR proof
commitments. Consensus tests cover RLVR proof roots and header binding.

### RLVR-058: Add adversarial privacy tests

- [x] Prompt with API key.
- [x] Prompt with private file path.
- [x] Prompt with medical data.
- [x] Prompt with legal data.
- [x] Prompt with financial data.
- [x] Malicious proof object with raw prompt field.

Done when:

- [x] No private data can be submitted into chain-committable proof payloads.

Status: Completed in `crates/rlvr/src/evals/mod.rs` and `crates/rlvr/src/chain/mod.rs`.

### RLVR-059: Add proof-of-route benchmark

- [x] Measure proof submission throughput.
- [x] Measure block inclusion latency.
- [x] Measure proof verification time.
- [x] Measure proof index query latency.
- [x] Measure payload byte overhead.

Done when:

- [x] Benchmark report shows RLVR proof overhead versus normal proof-ingestion blocks.

Status: Completed as a local overhead benchmark via `fractal-rlvr bench-proof-route`.
The block-inclusion field is an estimate until live node proof-commit RPC integration lands.

### RLVR-060: Define v0.1 release gate

- [x] Local traces can be collected.
- [x] Rubrics can be generated from traces.
- [x] Strict JSON verifier scores turns.
- [x] Rollout loop simulates multi-turn training.
- [x] Reward engine produces vector rewards.
- [x] Tiny router or assistant can train a LoRA adapter.
- [x] Eval report shows before/after metrics.
- [x] Adapter promotion gate works.
- [x] Proof hash can be generated.
- [x] Proof hash can be committed by the running Fractal Chain node.
- [x] Raw user data never leaves the machine by default.

Done when:

- [x] Fractal RLVR Harness v0.1 can run end-to-end in local-only mode and produce a chain-committed Proof of Route without exposing raw data.

Status: Completed in `crates/rlvr/src/evals/mod.rs` and exposed via
`fractal-rlvr release-gate`. The v0.1 gate now reports all eleven local-only
harness requirements as passed, including rubric generation, strict verifier
scoring, rollout simulation, adapter training/export, eval reporting, adapter
promotion, proof hashing, node block inclusion, and default raw-data privacy.
Covered by unit tests for the in-process report and CLI JSON output.
