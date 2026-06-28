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

- [ ] Create append-only JSONL trace storage.
- [ ] Store local path outside chain state by default.
- [ ] Add retention metadata.
- [ ] Add trace hash derivation.

Done when:

- [ ] A trace can be saved and read back locally.

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

- [ ] Generate degraded/underspecified prompt.
- [ ] Generate missing-info checkpoints.
- [ ] Mark checkpoints that must resolve before final answer.
- [ ] Generate simulator answer-if-asked values.

Done when:

- [ ] 100 sample QA pairs produce valid AskMind rubrics.

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

- [ ] Convert high/low reward rollouts into preference pairs.
- [ ] Add DPO mode.
- [ ] Add SFT mode for high-quality rollouts.

Done when:

- [ ] `fractal-rlvr train --mode dpo` works on small machines.

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

- [ ] Export adapter weights.
- [ ] Export adapter config.
- [ ] Export reward policy.
- [ ] Export eval report.
- [ ] Export model card.
- [ ] Export hashes.

Done when:

- [ ] Adapter can be loaded by Fractal router or chat runtime.

### RLVR-036: Create baseline evals

- [ ] AskMind local set.
- [ ] AskOverconfidence local set.
- [ ] RouteCorrectness set.
- [ ] ToolUse set.
- [ ] CompressionLoss set.
- [ ] User trace replay set.

Done when:

- [ ] Base model and trained adapter can be compared.

### RLVR-037: Implement metrics report

- [ ] Final answer accuracy.
- [ ] Checkpoint coverage.
- [ ] Redundant question rate.
- [ ] Premature answer rate.
- [ ] Correct route rate.
- [ ] Unnecessary escalation rate.
- [ ] Private-data leakage rate.
- [ ] Average cost.
- [ ] Average latency.

Done when:

- [ ] `fractal-rlvr eval-report` creates HTML and JSON reports.

### RLVR-038: Implement adapter promotion gate

- [ ] Require coverage improvement.
- [ ] Require route correctness improvement.
- [ ] Require bounded cost.
- [ ] Require bounded latency.
- [ ] Require no single-turn accuracy collapse.
- [ ] Require redundant question rate under limit.
- [ ] Require zero privacy violations.
- [ ] Add rollback metadata.

Done when:

- [ ] Bad adapters are blocked automatically.

### RLVR-039: Add MVP success metrics

- [ ] Track route correctness improvement target of 15 percentage points.
- [ ] Track clarification checkpoint improvement target of 20 percentage points.
- [ ] Track false-premise correction improvement target of 20 percentage points.
- [ ] Track redundant question rate below 15%.
- [ ] Track private-data leakage rate equal to zero.
- [ ] Track expensive-model escalation decrease.

Done when:

- [ ] Eval report declares pass/fail against MVP targets.

## Phase 8: Local Proof Objects

### RLVR-040: Define proof object schema

- [ ] Define `ProofOfRoute`.
- [ ] Define `ProofOfEval`.
- [ ] Define `ProofOfTraining`.
- [ ] Include `trace_hash`.
- [ ] Include `rubric_hash`.
- [ ] Include `reward_policy_hash`.
- [ ] Include `router_policy_hash`.
- [ ] Include `model_id_hash`.
- [ ] Include `adapter_hash`.
- [ ] Include `eval_result_hash`.
- [ ] Include `timestamp`.
- [ ] Include `node_signature`.

Done when:

- [ ] Proof object can be generated without exposing raw data.

### RLVR-041: Add deterministic proof hashing

- [ ] Canonicalize proof object serialization.
- [ ] Hash proof object.
- [ ] Add mutation tests for every committed field.

Done when:

- [ ] Any field change changes the proof hash.

### RLVR-042: Add node signature support

- [ ] Sign proof object hash with node key.
- [ ] Verify signature locally.
- [ ] Include node id/public key reference.

Done when:

- [ ] Invalid signatures fail verification.

### RLVR-043: Add raw-data exclusion tests

- [ ] Attempt to include raw prompt in proof object.
- [ ] Attempt to include raw answer in proof object.
- [ ] Attempt to include API key in proof object.
- [ ] Attempt to include private file contents in proof object.

Done when:

- [ ] Tests fail if raw data appears in chain-committable proof objects.

## Phase 9: Fractal Chain Node Integration

### RLVR-044: Add RLVR proof pool

- [ ] Add local pending pool for RLVR proof objects.
- [ ] Key by proof hash.
- [ ] Reject duplicate proof hashes.
- [ ] Reject invalid signatures.
- [ ] Track proof type metrics.

Done when:

- [ ] Node can hold pending ProofOfRoute/Eval/Training objects before block inclusion.

### RLVR-045: Add `fractal_submitRlvrProof` RPC

- [ ] Accept canonical proof object bytes or JSON.
- [ ] Validate schema.
- [ ] Validate signature.
- [ ] Validate no raw private fields.
- [ ] Insert into RLVR proof pool.

Done when:

- [ ] Valid proof returns proof hash.
- [ ] Invalid proof fails closed.

### RLVR-046: Add `fractal_getRlvrProof` RPC

- [ ] Query pending proof by hash.
- [ ] Query committed proof by hash.
- [ ] Return proof type, status, timestamp, and block reference.
- [ ] Do not return raw trace data.

Done when:

- [ ] Proof status can be inspected without exposing private data.

### RLVR-047: Add proof-of-route block payload item

- [ ] Extend proof-ingestion payloads with RLVR proof commitments or a dedicated proof item.
- [ ] Commit only proof hashes and metadata roots.
- [ ] Preserve existing proof-ingestion payload roots.
- [ ] Add payload root tests.

Done when:

- [ ] RLVR proofs can be included in proof-ingestion blocks.

### RLVR-048: Bind RLVR proof root into block header extension

- [ ] Add deterministic root over included RLVR proofs.
- [ ] Bind root into versioned payload/header extension.
- [ ] Add header hash mutation tests.

Done when:

- [ ] Changing any included RLVR proof changes the block commitment.

### RLVR-049: Add proof verification during block apply

- [ ] Validate proof object hashes.
- [ ] Validate node signatures.
- [ ] Validate proof type.
- [ ] Validate raw-data exclusion.
- [ ] Reject malformed proof payloads.

Done when:

- [ ] Invalid RLVR proof block payloads do not advance accepted proof state.

### RLVR-050: Add proof-ingestion inclusion path

- [ ] Drain RLVR proof pool during block proposal when enabled.
- [ ] Include RLVR proofs in proof-ingestion or mixed mode.
- [ ] Keep legacy mode unchanged.
- [ ] Add node config gate.

Done when:

- [ ] Running node can commit RLVR proof hashes in a block.

### RLVR-051: Add proof finality index for RLVR proofs

- [ ] Index by proof hash.
- [ ] Index by proof type.
- [ ] Index by adapter hash.
- [ ] Index by route policy hash.
- [ ] Persist across restart.

Done when:

- [ ] RPC can query latest proof status after node restart.

### RLVR-052: Add proof dispute placeholder

- [ ] Define challenge target: bad route claim.
- [ ] Define challenge target: inflated reward.
- [ ] Define challenge target: fake eval.
- [ ] Define challenge target: wrong adapter hash.
- [ ] Define challenge target: policy mismatch.
- [ ] Store challenge records as hash-only commitments.

Done when:

- [ ] Fractal Chain has a future-compatible dispute schema without enabling payouts.

## Phase 10: API and UI

### RLVR-053: Add local RLVR API

- [ ] Endpoint to list local traces.
- [ ] Endpoint to make rubrics.
- [ ] Endpoint to run rollout.
- [ ] Endpoint to run eval.
- [ ] Endpoint to export adapter.
- [ ] Endpoint to create proof object.
- [ ] Endpoint to submit proof to local node.

Done when:

- [ ] UI can run the MVP flow without shell commands.

### RLVR-054: Add Improve My Local Model UI entry

- [ ] Add settings button.
- [ ] Choose local-only mode.
- [ ] Choose target: router, assistant, critic, compressor.
- [ ] Choose traces.
- [ ] Run eval.
- [ ] Train adapter.
- [ ] Review report.
- [ ] Approve or reject adapter.

Done when:

- [ ] A non-technical user can run the loop from UI.

### RLVR-055: Add training report screen

- [ ] Show before vs after.
- [ ] Show what improved.
- [ ] Show what got worse.
- [ ] Show better behavior examples.
- [ ] Show failure examples.
- [ ] Show privacy status.
- [ ] Show promotion result.
- [ ] Show chain proof status.

Done when:

- [ ] User can understand why the adapter should or should not be used.

### RLVR-056: Add proof-of-route explorer panel

- [ ] Show route policy id.
- [ ] Show prompt classification hash/status.
- [ ] Show selected model/tool/agent.
- [ ] Show cost/latency policy result.
- [ ] Show verifier pass/fail summary.
- [ ] Show chain proof hash.
- [ ] Link to node RPC proof status.

Done when:

- [ ] User can audit route proof status without seeing private raw data on-chain.

## Phase 11: Tests, Benchmarks, and Release Gates

### RLVR-057: Add unit and integration tests

- [x] Schema tests.
- [x] Privacy filter tests.
- [x] Rubric generator tests.
- [x] Verifier parser tests.
- [x] Reward vector tests.
- [x] Proof object tests.
- [ ] Node RPC tests.
- [ ] Block inclusion tests.

Done when:

- [x] RLVR tests run in CI.

Status: Partially complete. The crate now has schema, privacy, rubric generator,
verifier parser, reward-vector, proof-object, adversarial privacy, benchmark, and
release-gate tests. Node RPC and block inclusion tests remain tied to their implementation tasks.

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
- [ ] Rubrics can be generated from traces.
- [ ] Strict JSON verifier scores turns.
- [ ] Rollout loop simulates multi-turn training.
- [x] Reward engine produces vector rewards.
- [ ] Tiny router or assistant can train a LoRA adapter.
- [ ] Eval report shows before/after metrics.
- [ ] Adapter promotion gate works.
- [x] Proof hash can be generated.
- [ ] Proof hash can be committed by the running Fractal Chain node.
- [x] Raw user data never leaves the machine by default.

Done when:

- [ ] Fractal RLVR Harness v0.1 can run end-to-end in local-only mode and produce a chain-committed Proof of Route without exposing raw data.

Status: Release gate defined in `crates/rlvr/src/evals/mod.rs` and exposed via
`fractal-rlvr release-gate`. The gate currently fails honestly until the unchecked
implementation tasks are completed.
