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

Status: Completed skeleton. Phase 1+ commands are registered stubs until their implementation phases.

### RLVR-006: Add node feature flags

- [x] Add `FRACTAL_RLVR_ENABLED=false` default.
- [x] Add `FRACTAL_RLVR_CHAIN_COMMIT_ENABLED=false` default.
- [x] Add `FRACTAL_RLVR_RAW_DATA_ON_CHAIN=false` hard default.
- [x] Add node startup logging for RLVR state.

Done when:

- [x] The running node can report RLVR enabled/disabled status.

Status: Completed via `RpcChainConfig` fields and node startup logging.

## Phase 1: Trace Collection

### RLVR-007: Implement local trace store

- [ ] Create append-only JSONL trace storage.
- [ ] Store local path outside chain state by default.
- [ ] Add retention metadata.
- [ ] Add trace hash derivation.

Done when:

- [ ] A trace can be saved and read back locally.

### RLVR-008: Implement trace privacy tags

- [ ] Detect and tag emails.
- [ ] Detect and tag phone numbers.
- [ ] Detect and tag addresses.
- [ ] Detect and tag API keys.
- [ ] Detect and tag financial data.
- [ ] Detect and tag health data.
- [ ] Detect and tag legal data.
- [ ] Detect and tag private file references.

Done when:

- [ ] Private traces are marked `local_only`.
- [ ] Export is blocked unless explicit approval exists.

### RLVR-009: Add node trace logger integration

- [ ] Hook trace logging into node-facing route/proof APIs.
- [ ] Capture prompt hash.
- [ ] Capture selected model/tool/agent.
- [ ] Capture router reason.
- [ ] Capture route policy id/hash.
- [ ] Capture answer hash.
- [ ] Capture latency and cost estimate.
- [ ] Capture user correction/rating when available.

Done when:

- [ ] Every RLVR-enabled chat/route request produces one local trace row.

### RLVR-010: Add trace hash commitment helpers

- [ ] Hash raw trace locally.
- [ ] Hash redacted trace locally.
- [ ] Hash verifier outputs.
- [ ] Hash reward vector.
- [ ] Add tests proving raw content is not included in chain proof objects.

Done when:

- [ ] Proof object can reference `trace_hash` without exposing trace content.

## Phase 2: Rubric Generator

### RLVR-011: Build AskMind rubric generator

- [ ] Generate degraded/underspecified prompt.
- [ ] Generate missing-info checkpoints.
- [ ] Mark checkpoints that must resolve before final answer.
- [ ] Generate simulator answer-if-asked values.

Done when:

- [ ] 100 sample QA pairs produce valid AskMind rubrics.

### RLVR-012: Build AskOverconfidence rubric generator

- [ ] Inject or identify false premise.
- [ ] Generate false-premise checkpoint list.
- [ ] Mark correction-required checkpoints.
- [ ] Add expected correction criteria.

Done when:

- [ ] Verifier can identify the false claim that must be corrected.

### RLVR-013: Build RouteCorrectness rubric generator

- [ ] Read prompt, model/tool inventory, and route policy.
- [ ] Generate task classification checkpoint.
- [ ] Generate local-sufficiency checkpoint.
- [ ] Generate tool-required checkpoint.
- [ ] Generate external-escalation checkpoint.
- [ ] Generate privacy-protection checkpoint.
- [ ] Generate final-answer-acceptable checkpoint.

Done when:

- [ ] Router eval examples can be generated from real node traces.

### RLVR-014: Build ToolUse and CompressionLoss rubric modes

- [ ] ToolUse checks current info, file analysis, finance, law, weather, tracking, and pricing.
- [ ] CompressionLoss checks dropped facts, numeric fidelity, citations, and constraints.
- [ ] Add fixtures for both modes.

Done when:

- [ ] Both modes produce valid checkpoint lists from sample traces.

## Phase 3: Verifier Engine

### RLVR-015: Implement strict JSON verifier contract

- [ ] Define verifier JSON schema.
- [ ] Require fields for clarification question, final answer, tool call, route decision, premature answer, redundant question, false-premise correction, route validity, and reward.
- [ ] Add retry on invalid JSON.
- [ ] Log unparseable verifier outputs locally.
- [ ] Exclude unparseable outputs from training.

Done when:

- [ ] Invalid JSON retries automatically.
- [ ] Unparseable output is recorded but not used for rewards.

### RLVR-016: Implement checkpoint coverage scorer

- [ ] Output `targeted_checkpoints`.
- [ ] Output `resolved_checkpoints`.
- [ ] Output `missed_checkpoints`.
- [ ] Output `redundant_question`.
- [ ] Compute coverage score per trace.

Done when:

- [ ] Coverage score is deterministic for a fixed verifier output.

### RLVR-017: Implement final answer scorer

- [ ] Score answer correctness.
- [ ] Score rubric completion.
- [ ] Score reasoning failure.
- [ ] Score insufficient-information failure.
- [ ] Score route failure.
- [ ] Score tool failure.
- [ ] Return pass/fail explanation.

Done when:

- [ ] Final score explains why the answer passed or failed.

### RLVR-018: Add verifier panel support

- [ ] Support one local judge.
- [ ] Support multiple verifier outputs.
- [ ] Aggregate binary/checkpoint judgments.
- [ ] Flag verifier disagreement.

Done when:

- [ ] Panel mode can compare local verifier against stronger verifier output when configured.

### RLVR-019: Store verifier Q&A training records

- [ ] Store every verifier checklist question locally.
- [ ] Store the verifier answer and evidence fields.
- [ ] Store model id, verifier id, policy hash, task id, and trace hash.
- [ ] Keep raw prompt out of export by default.

Done when:

- [ ] Verifier Q&A can be replayed later for training/evaluation.

## Phase 4: User Simulator

### RLVR-020: Build local user simulator

- [ ] Accept hidden original query.
- [ ] Accept checkpoint list.
- [ ] Accept assistant clarification question.
- [ ] Reveal only information explicitly asked for.

Done when:

- [ ] Asking for voltage reveals voltage only.
- [ ] Vague clarification gets a vague simulated reply.

### RLVR-021: Add adversarial simulator mode

- [ ] Simulate partial answers.
- [ ] Simulate wrong answers.
- [ ] Simulate ambiguous answers.
- [ ] Simulate annoyed answers.
- [ ] Simulate contradictory answers.

Done when:

- [ ] Clean and messy simulator modes are both selectable.

### RLVR-022: Add simulator privacy guard

- [ ] Prevent simulator from revealing hidden fields not requested.
- [ ] Prevent simulator from leaking hidden original query wholesale.
- [ ] Add tests for overbroad assistant questions.

Done when:

- [ ] Hidden information only appears when explicitly targeted by a checkpoint.

### RLVR-023: Add multi-turn dialogue trace builder

- [ ] Build 3-turn trace loops.
- [ ] Attach route decisions to assistant turns.
- [ ] Attach verifier outputs to each turn.
- [ ] Attach final reward vector.

Done when:

- [ ] A simulated rollout produces a complete `DialogueTrace`.

## Phase 5: Reward Engine

### RLVR-024: Implement reward vector

- [ ] Implement correctness.
- [ ] Implement checkpoint coverage.
- [ ] Implement clarification quality.
- [ ] Implement false-premise detection.
- [ ] Implement route correctness.
- [ ] Implement tool correctness.
- [ ] Implement cost efficiency.
- [ ] Implement latency efficiency.
- [ ] Implement privacy compliance.
- [ ] Implement non-redundancy.

Done when:

- [ ] Each rollout emits `reward_vector.json`.

### RLVR-025: Implement configurable reward weights

- [ ] Add weights for router training.
- [ ] Add weights for assistant training.
- [ ] Add weights for critic training.
- [ ] Add weights for compressor training.
- [ ] Add weights for tool-use training.
- [ ] Load weights from YAML or TOML.

Done when:

- [ ] Changing reward config changes final reward without code edits.

### RLVR-026: Add MVP reward policy v0.1

- [ ] Add positive reward for correct final answer after required checkpoints.
- [ ] Add positive reward for correct route.
- [ ] Add positive reward for targeted clarification.
- [ ] Add positive reward for correcting false premise.
- [ ] Add positive reward for using cheap/local model when sufficient.
- [ ] Add penalty for redundant question.
- [ ] Add penalty for missing required tool.
- [ ] Add penalty for private-data external route.
- [ ] Add penalty for premature answer.
- [ ] Add penalty for wrong final answer.

Done when:

- [ ] Reward v0.1 matches the PRD weights.

### RLVR-027: Add anti-reward-hacking checks

- [ ] Detect asking every possible question.
- [ ] Detect never giving final answer.
- [ ] Detect overusing expensive models.
- [ ] Detect pretending checkpoints were resolved.
- [ ] Detect verbose uncertainty hiding.
- [ ] Detect self-verifier reward inflation.

Done when:

- [ ] Eval report flags suspicious reward gains.

## Phase 6: Local Rollout and Training

### RLVR-028: Implement rollout task sampler

- [ ] Sample by mode.
- [ ] Sample by difficulty.
- [ ] Sample by domain.
- [ ] Support user trace replay set.

Done when:

- [ ] Rollout runner can select a deterministic task batch by seed.

### RLVR-029: Implement actor runtime interface

- [ ] Support tiny assistant model.
- [ ] Support router model.
- [ ] Support clarification model.
- [ ] Support critic model.
- [ ] Support compressor model.
- [ ] Support tool-use policy model.

Done when:

- [ ] A local actor can be invoked through one interface.

### RLVR-030: Implement rollout runner

- [ ] Actor responds.
- [ ] Verifier scores turn.
- [ ] Simulator replies if needed.
- [ ] Actor continues.
- [ ] Terminal verifier scores final answer.
- [ ] Reward vector is computed.

Done when:

- [ ] `fractal-rlvr rollout --n 100` produces trace files.

### RLVR-031: Implement GRPO-style trainer interface

- [ ] Multiple rollouts per prompt.
- [ ] Group-relative reward normalization.
- [ ] Adapter-only update.
- [ ] Checkpoint saving.
- [ ] Eval before/after.

Done when:

- [ ] A tiny local model can train an adapter from verifier rewards.

### RLVR-032: Add fallback DPO/SFT path

- [ ] Convert high/low reward rollouts into preference pairs.
- [ ] Add DPO mode.
- [ ] Add SFT mode for high-quality rollouts.

Done when:

- [ ] `fractal-rlvr train --mode dpo` works on small machines.

### RLVR-033: Add training resource guard

- [ ] Detect available memory.
- [ ] Detect GPU/CPU mode.
- [ ] Limit batch size.
- [ ] Stop before local machine overload.

Done when:

- [ ] Training fails gracefully with a clear resource error.

## Phase 7: Adapter Registry and Evaluation

### RLVR-034: Implement adapter registry

- [ ] Register adapter id.
- [ ] Track base model.
- [ ] Track training mode.
- [ ] Track reward version.
- [ ] Track data-local-only flag.
- [ ] Track chain commit hash.

Done when:

- [ ] Adapter metadata can be listed locally.

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

- [ ] Schema tests.
- [ ] Privacy filter tests.
- [ ] Rubric generator tests.
- [ ] Verifier parser tests.
- [ ] Reward vector tests.
- [ ] Proof object tests.
- [ ] Node RPC tests.
- [ ] Block inclusion tests.

Done when:

- [ ] RLVR tests run in CI.

### RLVR-058: Add adversarial privacy tests

- [ ] Prompt with API key.
- [ ] Prompt with private file path.
- [ ] Prompt with medical data.
- [ ] Prompt with legal data.
- [ ] Prompt with financial data.
- [ ] Malicious proof object with raw prompt field.

Done when:

- [ ] No private data can be submitted into chain-committable proof payloads.

### RLVR-059: Add proof-of-route benchmark

- [ ] Measure proof submission throughput.
- [ ] Measure block inclusion latency.
- [ ] Measure proof verification time.
- [ ] Measure proof index query latency.
- [ ] Measure payload byte overhead.

Done when:

- [ ] Benchmark report shows RLVR proof overhead versus normal proof-ingestion blocks.

### RLVR-060: Define v0.1 release gate

- [ ] Local traces can be collected.
- [ ] Rubrics can be generated from traces.
- [ ] Strict JSON verifier scores turns.
- [ ] Rollout loop simulates multi-turn training.
- [ ] Reward engine produces vector rewards.
- [ ] Tiny router or assistant can train a LoRA adapter.
- [ ] Eval report shows before/after metrics.
- [ ] Adapter promotion gate works.
- [ ] Proof hash can be generated.
- [ ] Proof hash can be committed by the running Fractal Chain node.
- [ ] Raw user data never leaves the machine by default.

Done when:

- [ ] Fractal RLVR Harness v0.1 can run end-to-end in local-only mode and produce a chain-committed Proof of Route without exposing raw data.
