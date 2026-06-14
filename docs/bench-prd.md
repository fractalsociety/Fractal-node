# FractalWork Benchmark PRD

## Goal

Measure whether FractalWork can support a profitable, adversarially robust agent labor market before mainnet. The benchmarks must price outcomes in the same units the protocol cares about: payouts, inference cost, gas, verifier leakage, and churn.

## Bench 1: Economic Viability

Question: can a model be a profitable worker?

The benchmark generates tasks with objectively verifiable outcomes across difficulty tiers. Each task has a fixed payout and gas cost. A model attempt is scored by:

```text
profit = quality_score * payout - inference_cost - gas
inference_cost = input_tokens * input_token_price + output_tokens * output_token_price
```

Metrics:

- Pass rate by model and tier.
- Average quality by model and tier.
- Total and average profit by model and tier.
- Profit margin: `total_profit / total_available_payout`.
- Break-even payout per passed task.

Task families:

- `math`: deterministic arithmetic/proof-checkable answers.
- `data_extraction`: structured extraction from generated records with known answers.
- `code_hidden_tests`: generated programming tasks with hidden test cases and exact expected outputs.

Why procedural generation matters:

- Reduces training-set contamination.
- Gives the future network an infinite source of evaluation jobs.
- Allows payout curves to be swept without changing task supply.

Implementation status:

- Implemented as `fractal-economic-bench`.
- Supports deterministic built-in synthetic model profiles.
- Supports JSONL model-attempt input for external solver/model adapters.
- Produces JSON summaries suitable for plotting profitability curves.

Run:

```sh
cargo run -p fractal-bench --bin fractal-economic-bench -- --tasks 90
```

With external attempts:

```sh
cargo run -p fractal-bench --bin fractal-economic-bench -- --tasks 90 --attempts attempts.jsonl
```

Each JSONL attempt:

```json
{"taskId":"task-000001","model":"model-name","output":"42","inputTokens":1000,"outputTokens":100,"inputTokenPriceMicroFracPerMillion":2000,"outputTokenPriceMicroFracPerMillion":8000}
```

## Bench 2: Verifier Quality

Question: can verifier models cheaply and accurately judge work?

Fixed corpus of `(task, submission)` pairs with ground-truth labels:

- Fully correct work.
- Edge-case bugs.
- Confident wrong answers.
- Plagiarized or near-duplicate work.
- Format-valid but semantically bad work.

Metrics:

- ROC/AUC.
- False-accept rate priced as economic leakage.
- False-reject rate priced as honest-worker churn.
- Brier score / calibration error.

Protocol usage:

- Set verifier reward curves.
- Set challenge thresholds.
- Set stake and slash parameters.
- Choose escalation policy from measured confidence calibration.

Implementation status:

- Implemented as `fractal-verifier-bench`.
- Ships a deterministic fixed corpus with correct submissions, edge-case failures, confidently wrong answers, plagiarized/duplicate work, and format-valid semantic failures.
- Supports JSONL verifier judgments from external model adapters.
- Reports ROC/AUC, priced false accepts, priced false rejects, and Brier calibration.

Run:

```sh
cargo run -p fractal-bench --bin fractal-verifier-bench
```

With external judgments:

```sh
cargo run -p fractal-bench --bin fractal-verifier-bench -- --judgments verifier-judgments.jsonl
```

Each JSONL judgment:

```json
{"caseId":"verify-000001","verifier":"model-name","accept":true,"confidenceMilli":875,"scoreMilli":920,"inputTokens":1200,"outputTokens":80,"inputTokenPriceMicroFracPerMillion":2000,"outputTokenPriceMicroFracPerMillion":8000}
```

## Bench 3: Adversarial Exploit

Question: does verification remain easier than generation when workers attack the verifier?

Run a worker/verifier matrix where workers are explicitly optimized to extract payout while minimizing real work:

- Output padding.
- Confident hand-waving.
- Test-case gaming.
- Sycophancy toward judge models.
- Prompt injections embedded in submissions.

Metrics:

- Dollars extracted per attack attempt.
- Leakage rate under attack versus baseline.
- Minimum stake-to-payout ratio implied by measured leakage.
- Worker-model by verifier-model exploit matrix.

This is the benchmark that should drive mainnet economic safety parameters.
