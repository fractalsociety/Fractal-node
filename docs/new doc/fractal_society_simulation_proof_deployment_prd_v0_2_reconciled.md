# Fractal Society PRD

## Train in Simulation. Prove in Public. Deploy with Confidence.

**Document type:** Focused execution PRD
**Version:** 0.2 — Legacy-Reconciled
**Status:** Reconciled Draft — Founder Review Required
**Date:** 2026-06-19
**Primary owner:** Founder
**Initial domain:** Simulated crypto-perpetual trading
**Long-term product:** A domain-neutral, decentralized research protocol for humans and AI agents
**Parent document:** Fractal Society Master PRD (2026-06-19)
**Legacy implementation baseline:** `mater-june-19th.md` (1,666 tracked items; 650 checked; evidence revalidation required)

### Revision 0.2 summary

This revision reconciles the execution plan with the June 19 master implementation tracker instead of treating Fractal Society as a greenfield build.

- Existing Forge, RL gym/verifier, Hermes/OpenClaw, skill, sandbox, scorecard, and package capabilities are classified as **reuse candidates**.
- A new `PHASE-00R` converts old checklist claims into current code, test, runtime, and security evidence.
- No legacy checkbox automatically passes a new phase gate.
- Trading-specific work is concentrated in market data, portfolio simulation, trading verifiers, Arena rules, and venue shadow/testnet adapters.
- Full router completion, full Graph OS analytics, complete local-training parity, marketplace checkout, a custom chain, and real-capital deployment are not MVP blockers.
- Every phase ends with a founder evidence review and an explicit approve/request-changes/reject decision.

---

## 1. Executive summary

Fractal Society will let anyone build or import an AI agent, train and test it with simulated resources, prove its capabilities through reproducible evaluations, publish cryptographically verifiable results, earn reputation from useful work, and eventually qualify for tightly controlled real-world deployment.

The first use case is trading because markets provide fast feedback, machine-readable observations, objective outcomes, abundant public data, and strong user motivation. The initial product will advertise a simple entry point:

> **Start with simulated capital. Train an agent. Prove that it works.**

Trading is the first **domain adapter**, not the core architecture. The reusable core is a research pipeline:

```text
Question
→ Protocol
→ Dataset snapshot
→ Environment
→ Agent and skills
→ Experiment run
→ Evidence
→ Verification
→ Public commitment
→ Review and replication
→ Reputation and reward
→ Guarded deployment
```

The product should become a “research blockchain” by making verified experiments, not financial transactions, its fundamental unit of value. The blockchain records authorship, timestamps, commitments, reviews, challenge outcomes, reputation checkpoints, permissions, and reward settlement. Raw data, model weights, private prompts, full traces, and proprietary strategies remain off-chain and content-addressed.

The MVP ends at public proof and the fake-money Agent Arena. Live shadow and testnet connectivity are the next operational release. The MVP does **not** require real-money trading, pooled funds, a token, or a new layer-one blockchain. Real-capital deployment is a later, separately approved phase.

---

## 2. Product vision

### 2.1 Vision statement

Create an open research network where intelligence is accessible, experiments are reproducible, claims are challengeable, and useful human or agent contributions compound into shared knowledge.

### 2.2 Product promise

A user should be able to:

1. State a research question or choose a challenge.
2. Create, import, or compose an agent from reusable skills.
3. Train and test the agent using simulated resources.
4. Evaluate it against public benchmarks and private holdouts.
5. Freeze the agent, environment, and evaluation versions.
6. Publish a proof card without revealing private intellectual property.
7. Invite independent review or replication.
8. Earn explainable reputation and rewards.
9. Graduate to increasingly realistic environments.
10. Reach real deployment only after explicit risk and human approval gates.

### 2.3 Positioning

Fractal Society is not positioned as “another trading bot.”

It is positioned as:

> **The open proving ground for AI agents.**

Trading supplies the first fast, measurable proving ground. The same protocol must later support software engineering, forecasting, scientific literature research, robotics simulation, security research, and other domains.

### 2.4 Landing-page message for the first wedge

**Headline**
Train in Simulation. Prove in Public. Deploy with Confidence.

**Subheadline**
Build AI trading agents with simulated capital, test them against hidden market periods, publish verifiable results, and earn reputation before risking real money.

**Primary CTA**
Start with $100,000 simulated

**Secondary CTA**
Explore verified agents

**Trust statement**
Simulation results are not live trading results. Every scorecard exposes assumptions, fees, slippage, evaluation periods, and confidence limits.

---

## 3. Strategic thesis

### 3.1 Why trading first

Trading is a useful initial domain because it has:

- Fast and frequent feedback.
- Objective actions and outcomes.
- Public market observations.
- Existing communities of builders and competitors.
- Clear risk and execution constraints.
- Natural progression from historical replay to live shadow mode.
- Strong incentives to improve agents, verifiers, and simulations.

### 3.2 Why trading is not the final moat

Market quotes and common historical bars are widely available. The defensible asset is the **verified research graph**, containing:

- Precisely versioned questions and protocols.
- Dataset and environment commitments.
- Agent, model, prompt, skill, and tool versions.
- Decisions and actions.
- Verifier outputs.
- Failure modes.
- Independent reviews and replications.
- Downstream reuse.
- Proven contribution paths.

The network improves when every run creates structured evidence that can improve future environments, verifiers, training datasets, risk rules, and agent routing.

### 3.3 Flywheel

```text
More users and builders
→ more hypotheses, agents, skills, and edge cases
→ more structured experiment traces
→ stronger simulations, verifiers, and benchmarks
→ more credible scorecards and safer agents
→ more public proof, trust, and useful outcomes
→ more users, reviewers, sponsors, and capital
```

A second flywheel rewards reusable infrastructure:

```text
More verifier and skill authors
→ better reusable components
→ lower cost to build credible agents
→ more experiments and deployments
→ more downstream reuse evidence
→ stronger reputation and rewards for authors
```

### 3.4 Strategic constraint

More users do not automatically create better intelligence. Data is valuable only when it is:

- Structured.
- Consented for its intended use.
- Versioned.
- Evaluated out-of-sample.
- Weighted by reliability.
- Protected against leakage and gaming.
- Linked to outcomes and counterfactuals.

---

## 4. Product principles

1. **Simulation first; real capital last.** New users and agents begin with deterministic simulations and cannot skip graduation gates.
2. **Proof over screenshots.** Every result must reveal its assumptions, versions, data boundaries, and verifier evidence.
3. **Generic core, domain adapters.** No trading-specific logic may enter the research kernel.
4. **Public proof does not require public IP.** Users choose disclosure tiers and may commit hashes while sharing details only with approved reviewers.
5. **The chain is a proof layer, not a data warehouse.** Store commitments and settlement on-chain; keep rich artifacts off-chain.
6. **Reputation must be explainable.** Reputation comes from verified contributions, independent replication, useful reviews, and downstream reuse—not token balance.
7. **Failures are first-class research outputs.** A failed hypothesis can earn reputation when it exposes a robust counterexample, simulator defect, or useful verifier test.
8. **Training is automatic; promotion is gated.** An agent may propose a challenger version but may not promote or deploy itself.
9. **Human review is mandatory at phase boundaries.** No implementation phase is complete until automated checks pass and the founder explicitly approves the evidence packet.
10. **Portability is required.** Users own exportable agents, artifacts, proof manifests, and reputation evidence.
11. **Local and open models are supported.** The system should not depend on one centralized model provider.
12. **No performance promises.** Simulation and hypothetical results must be presented with visible limitations and realistic costs.

---

## 5. Goals and non-goals

### 5.1 Product goals

| ID | Goal | Target evidence |
|---|---|---|
| G-01 | A new user can create and run a simulated trading agent | First valid run in under 20 minutes |
| G-02 | Every published result is reproducible or explicitly labeled otherwise | Reproduction status on every proof card |
| G-03 | Public claims are backed by cryptographic commitments | Tamper detection and chain receipt |
| G-04 | Agent evaluation resists basic backtest gaming | Hidden holdouts, leakage checks, walk-forward tests |
| G-05 | The core pipeline remains domain-neutral | Deterministic reference adapter in CI and a second-domain pilot |
| G-06 | Useful contributions improve future agents | Reuse and downstream-protection metrics |
| G-07 | Users can retain private strategy IP | Configurable disclosure tiers and encrypted reviewer access |
| G-08 | No model can directly bypass deployment policy | Independent policy engine and signer separation |
| G-09 | Every implementation phase is testable | Machine-readable gate reports and evidence packets |
| G-10 | Founder review is built into delivery | Explicit approval required for each phase transition |

### 5.2 MVP goals

The MVP includes:

- A generic research-project and experiment schema.
- A deterministic simulation kernel.
- A reference non-trading environment used in continuous integration.
- A Hyperliquid perpetual-market data adapter.
- Historical replay for a limited initial market set.
- Simulated portfolios and realistic transaction-cost accounting.
- Agent templates and a sandboxed runtime.
- Public and private verifier suites.
- Version freezing and signed proof manifests.
- Public proof cards and a research explorer.
- A seasonal fake-money Agent Arena.
- Reputation events and non-transferable research credits.

### 5.3 Non-goals for the MVP

- Custody of user funds.
- Pooled investment products.
- Guaranteed returns or financial advice.
- Fully autonomous live trading.
- Performance fees.
- A speculative token launch.
- A custom layer-one blockchain.
- High-frequency or colocated execution.
- Storing raw datasets, prompts, weights, or wallet secrets on-chain.
- Unrestricted internet access for evaluated agents.
- Self-authorized model or policy updates.
- Claiming that simulation performance predicts live performance.

---

### 5.4 Legacy implementation baseline

The June 19 tracker header reports **1,666 implementation items**, of which **650 are checked** and **1,016 remain open**. The markdown file contains **1,682 checkbox lines** because it adds **16 manual-review placeholders** for source PRDs that had no native checkbox checklist. A checkmark is treated as a historical implementation claim until PHASE-00R verifies code, tests, runtime behavior, and evidence freshness.

| Legacy source | Checked | Total | Interpretation for this PRD |
|---|---:|---:|---|
| Fractal Website Blockchain Integration PRD | 0 | 7 | Manual review only; zero checkbox evidence does not prove absence or completion. |
| Repository Feature Graph PRD | 0 | 6 | Manual review only; verify code before reuse. |
| FractalWork Core MVP Implementation Plan | 0 | 1 | Manual review only; verify signed events, artifacts, timeouts, and persistence. |
| PRD: Forge - Train, Deploy, and Rent Specialist Agents | 0 | 1 | Manual review only; marketplace is not an MVP blocker. |
| PRD: Forge Router - Route Work to Specialized Monetized Agents | 0 | 1 | Manual review only; use detailed router checklist for evidence. |
| Implementation Checklist: Forge Specialist Agent Router | 32 | 189 | Partial base: models/index/classifier/outcomes exist; filters/ranker/API/explanations remain open. |
| PRD Checklist: RL Gyms and Verifiers Marketplace | 238 | 343 | Strongest reusable substrate: schemas, runner, scorecards, sandbox, and training loop are substantially checked. |
| PRD: Fractal Forge Local Engine, Headless CLI, and Dashboard Integration | 112 | 365 | Useful partial base: dashboard/training/eval pieces exist; full CLI/data/privacy/production acceptance is incomplete. |
| PRD: Hermes and OpenClaw Agent Integration for Forge and FractalWork | 173 | 185 | Highly reusable agent-runtime, permission, connector, evaluation, and marketplace scaffold; revalidate real endpoints. |
| PRD: Digital Employee Skills, RAG Memory, and Codex Offload Layer | 95 | 241 | Reusable skill schemas, packs, ingestion, and retrieval; generic tool-permission/eval/dashboard work remains open. |
| PRD: Fractal Society Graph Operating System | 0 | 343 | Treat as unimplemented for planning: graph, reputation, fraud, and settlement remain new work. |

### 5.5 What this revision reuses

- RL gym, verifier, and suite package schemas; seeded episode execution; attached verifiers; scorecards; immutable package digests; sandbox controls; and training-loop feedback.
- Forge dashboard, curated model registry, training backend abstraction, structured events, base-vs-adapter evaluation, and artifact export where PHASE-00R verifies them.
- Hermes and OpenClaw runtime adapters, agent harness mapping, health checks, permission controls, encrypted secrets, eval/gym integration, fallback behavior, and imported-agent cards.
- Skill package ids/versions, core repo and Forge skills, RAG ingestion/retrieval, context bundles, and PRD/API/dashboard auditing.
- Router data models, marketplace index, request classifier, outcome records, and privacy controls—but not its unfinished eligibility filters, deterministic ranker, route API, explanations, or learned router.

### 5.6 Scope adaptations after reconciliation

The first trading product should **compose** existing systems rather than complete every old roadmap first:

1. Fine-tuning remains optional. A user can enter with code, prompts, rules, a local model, or an imported agent.
2. The first Arena uses user-selected agents; the unfinished general-purpose router is not a launch blocker.
3. The first proof layer wraps existing signed digests and scorecards; the full Graph OS follows after the Arena.
4. The first graph is a minimal relational/event projection for proofs, reviews, replications, and reputation—not the complete 343-item Graph OS.
5. Marketplace checkout and paid agent rental are deferred; research credits and sponsored bounties are sufficient for the first season.
6. A custom blockchain and token remain deferred. The MVP uses an adapter to an established chain for commitments only.
7. Real-capital execution is isolated in PHASE-12 and cannot be implied by approval of simulation, proof, Arena, shadow, or testnet work.


## 6. Personas

### 6.1 Learner / aspiring agent builder

Wants to experiment without risking money or setting up complex infrastructure.

**Core job:** “Help me turn an idea into a tested agent and teach me why it passed or failed.”

### 6.2 Quantitative or AI builder

Wants data, reproducible environments, distribution, benchmarking, and monetization.

**Core job:** “Give me credible infrastructure and a public track record that serious users can trust.”

### 6.3 Trader

Wants to understand behavior, test automation, and reduce risk before granting execution permission.

**Core job:** “Show me whether an agent adds value beyond my baseline after realistic costs.”

### 6.4 Verifier author / reviewer

Builds tests that detect leakage, weak assumptions, unsafe behavior, or false claims.

**Core job:** “Let my verification work earn reputation when it catches failures or protects downstream users.”

### 6.5 Replicator

Independently reruns an experiment or challenges a claim.

**Core job:** “Give me enough controlled access to confirm or falsify a result.”

### 6.6 Sponsor / research funder

Funds challenges, datasets, verifiers, or agent research.

**Core job:** “Release rewards only when predefined evidence and verification gates pass.”

### 6.7 Future allocator

Considers granting bounded capital to a verified agent.

**Core job:** “Show me risk, uncertainty, capacity, operational history, and policy compliance—not just return.”

### 6.8 Founder / product approver

Reviews phase evidence, risk decisions, and user experience before development advances.

**Core job:** “Show me exactly what passed, what failed, what changed, and what I am approving.”

---

## 7. Core user journeys

### 7.1 First-run journey: simulated trading agent

1. User creates an account or uses a wallet-based pseudonymous identity.
2. User chooses “Build a trading agent.”
3. Product explains that results are simulated and not predictive of live returns.
4. User selects a starter template or imports an agent package.
5. System creates a research project with a protocol draft.
6. User chooses a training window and an evaluation objective.
7. System validates data availability and prevents overlap with hidden evaluation windows.
8. Agent receives $100,000 in simulated capital.
9. Agent runs in historical replay.
10. Verifiers check accounting, leakage, risk, costs, and reproducibility.
11. User sees a scorecard with baselines and confidence warnings.
12. User may revise the agent and rerun during the development phase.
13. When ready, user freezes a candidate version.
14. Frozen candidate is evaluated on private holdouts and/or live shadow data.
15. User publishes a proof card at a chosen disclosure level.
16. Proof commitment is signed and anchored on-chain.
17. Other users may review, reproduce, challenge, or reuse approved components.
18. Reputation updates only after verifier, review, or replication events.

### 7.2 Arena journey

1. User joins a season with a fixed objective, rules, market universe, and development dataset.
2. User develops privately against the public training environment.
3. Submission deadline freezes all agent and dependency hashes.
4. Platform runs private historical holdouts.
5. Passing agents enter live shadow mode.
6. Scoreboards show robust, risk-adjusted, cost-adjusted results with uncertainty.
7. Final results include postmortems, disqualifications, and verifier findings.
8. Rewards go to strong agents, useful verifiers, meaningful replications, and high-value failure discoveries.

### 7.3 Verifier journey

1. Reviewer creates a verifier package with input schema, scoring logic, calibration examples, and safety policy.
2. Verifier runs against known-positive, known-negative, and adversarial fixtures.
3. The package is versioned and signed.
4. Research protocols can require the verifier.
5. Every downstream run records the exact verifier version.
6. When the verifier catches a valid issue or protects a deployment, downstream-impact edges increase the author’s reputation.

### 7.4 Founder review journey

1. Engineering runs `fractal gate run <phase-id>`.
2. The system assembles test results, screenshots, metrics, risks, and known failures.
3. The phase becomes `AWAITING_FOUNDER_REVIEW` only after mandatory automated checks pass.
4. The founder sees a concise summary plus drill-down evidence.
5. The founder chooses `APPROVE`, `REQUEST_CHANGES`, or `REJECT` and adds notes.
6. The signed decision becomes part of the project audit history.
7. The next phase cannot begin under the official roadmap until approval.

---

## 8. Generic research pipeline

### 8.1 Research stages

| Stage | Generic purpose | Trading implementation |
|---|---|---|
| R0 Question | Define the claim or objective | “Can this agent improve risk-adjusted BTC/ETH returns?” |
| R1 Protocol | Predeclare data, method, metrics, and pass criteria | Train/eval windows, cost model, assets, max drawdown |
| R2 Dataset | Create an immutable snapshot or stream commitment | Market events, funding, metadata, optional wallet history |
| R3 Environment | Define observations, actions, transition rules, and constraints | Exchange simulator, portfolio, fills, margin, funding |
| R4 Agent | Package model, prompts, code, tools, skills, and permissions | Signal model, position sizing, execution policy |
| R5 Experiment | Execute a deterministic or streaming run | Historical replay, walk-forward, live shadow |
| R6 Evidence | Produce structured traces and outcomes | Decisions, intended orders, fills, PnL, risk state |
| R7 Verification | Apply reusable tests and scorecards | Leakage, slippage, drawdown, policy, reproducibility |
| R8 Commitment | Sign and anchor hashes and claims | Proof manifest and score commitment |
| R9 Review | Independent audit, challenge, or replication | Rerun in controlled environment |
| R10 Reputation | Attribute useful contributions | Builder/verifier/replicator reputation events |
| R11 Deployment | Apply promotion and permissions policy | Shadow, testnet, bounded canary, later production |

### 8.2 Required stage artifacts

Every stage emits a typed artifact:

- `ResearchQuestion`
- `ResearchProtocol`
- `DatasetManifest`
- `EnvironmentManifest`
- `AgentManifest`
- `RunManifest`
- `DecisionTrace`
- `EvidenceBundle`
- `VerifierReport`
- `ProofManifest`
- `ReviewRecord`
- `ReplicationRecord`
- `ReputationEvent`
- `DeploymentCandidate`
- `DeploymentPolicy`

No stage may rely only on unstructured chat history.

### 8.3 Domain adapter contract

The research kernel must expose a stable adapter interface. A domain adapter provides domain meaning without changing the kernel.

```ts
interface DomainAdapter<Observation, Action, Outcome> {
  id: string;
  version: string;
  capabilityManifest(): CapabilityManifest;
  validateProtocol(protocol: ResearchProtocol): ValidationReport;
  resolveDataset(manifest: DatasetManifest): DatasetHandle;
  createEnvironment(config: EnvironmentConfig): Environment;
  normalizeObservation(raw: unknown): Observation;
  validateAction(action: Action, state: RuntimeState): PolicyDecision;
  step(action: Action): Promise<StepResult<Observation, Outcome>>;
  score(evidence: EvidenceBundle): Promise<MetricSet>;
  buildPublicEvidence(evidence: EvidenceBundle): RedactedEvidenceBundle;
  terminalConditions(): TerminalCondition[];
}
```

### 8.4 Genericity rules

- Core packages may import adapter interfaces but may not import trading classes.
- Domain-specific schemas live under versioned adapter namespaces.
- CI runs the entire pipeline with a deterministic reference environment.
- Every generic feature requires at least one reference-adapter test.
- Before “general research network” is used in marketing, a second real domain adapter must complete an end-to-end proof.
- Generic metrics use namespaced semantic definitions rather than hard-coded PnL fields.

### 8.5 Deterministic reference adapter

A small finite-state or multi-armed-bandit environment will be maintained solely for testing the generic pipeline. It must:

- Run in milliseconds.
- Produce known outcomes for fixed seeds.
- Support valid and invalid actions.
- Trigger success, failure, timeout, and policy-violation paths.
- Exercise commitments, reviews, replications, and reward events.

This adapter prevents trading complexity from hiding defects in the research kernel.

---

## 9. Research protocol specification

### 9.1 Protocol requirements

Every publishable experiment must predeclare:

- Research question and falsifiable claim.
- Domain adapter and version.
- Agent and dependency versions.
- Allowed observations and tools.
- Dataset boundaries.
- Development, validation, and evaluation windows.
- Random seeds or stream rules.
- Baselines.
- Primary and secondary metrics.
- Cost and resource assumptions.
- Safety and permission policy.
- Required verifiers.
- Promotion criteria.
- Disclosure level.
- Review policy.
- Reward policy, if any.

### 9.2 Example protocol manifest

```yaml
protocol_id: rp_trading_btc_eth_001
version: 1.0.0
domain_adapter: trading.hyperliquid.perps@1.0.0
question: Can the candidate improve net risk-adjusted return over declared baselines?
claim_type: comparative_performance
agent_ref: sha256:AGENT_HASH
datasets:
  development: sha256:DEV_DATA_HASH
  public_validation: sha256:VALIDATION_DATA_HASH
  private_evaluation: private:holdout_set_2026_q2
resources:
  starting_simulated_equity: 100000
  currency: USDC
  max_runtime_minutes: 60
  max_memory_gb: 8
market_scope:
  assets: [BTC, ETH]
  product: linear_perpetual
  maximum_leverage: 2
cost_model:
  fee_schedule_ref: fee-model-v1
  latency_ms: 250
  slippage_model_ref: l2-replay-v1
primary_metrics:
  - net_return
  - maximum_drawdown
  - cvar_95
required_verifiers:
  - accounting-integrity@1
  - temporal-leakage@1
  - cost-completeness@1
  - reproducibility@1
promotion_policy:
  require_all_verifiers: true
  max_drawdown_lte: 0.10
  policy_violations_eq: 0
visibility: committed_private_artifacts
```

### 9.3 Protocol amendments

Once evaluation begins, a protocol is immutable. Any material change creates a new version and invalidates comparison with prior frozen results unless the scorecard explicitly presents them as separate experiments.

---

## 10. Trading domain adapter

### 10.1 Initial scope

MVP trading scope:

- Venue model: Hyperliquid perpetuals.
- Initial assets: BTC and ETH.
- Base currency: simulated USDC.
- Position mode: one-way position per asset.
- Maximum initial leverage: 2x.
- Allowed actions: hold, place order, cancel order, modify order, reduce position.
- Initial order types: marketable IOC, limit GTC, reduce-only, stop trigger where supported by simulator.
- No withdrawals, transfers, bridging, spot deployment, or arbitrary chain calls.
- No real order submission in MVP.

### 10.2 Why the platform needs its own recorder

The adapter must maintain a first-party, continuously monitored market recorder. Official historical archives can be delayed or incomplete, and not every required dataset is provided. The recorder must capture enough synchronized data to support replay, cost modeling, and live shadow evaluation.

### 10.3 Market-data inputs

Required normalized streams:

- Exchange metadata and instrument precision.
- Best bid and ask.
- L2 order-book snapshots and deltas where available.
- Trades.
- Mark prices.
- Oracle prices.
- Funding rates and funding payments.
- Open interest and relevant asset context.
- Liquidation events when available.
- Exchange status and data-quality events.
- User fills and orders only when explicitly authorized.

### 10.4 Data integrity requirements

- All events receive source time, receive time, sequence context, source identity, and checksum.
- Clock drift is monitored.
- Duplicates are idempotently removed.
- Gaps are detected and explicitly represented; they are never silently interpolated for high-confidence proof tiers.
- Raw records are immutable.
- Normalized records preserve pointers to raw evidence.
- Data corrections create new versions rather than rewriting history.
- Dataset manifests state missingness and quality scores.

### 10.5 Simulation tiers

| Tier | Mode | Purpose | Proof ceiling |
|---|---|---|---|
| S0 | Deterministic synthetic fixtures | Unit and accounting tests | Internal only |
| S1 | Candle/bar replay | Fast ideation | Preliminary |
| S2 | Top-of-book replay | Better spread and latency modeling | Development proof |
| S3 | L2 event replay | Execution-aware evaluation | Auditable proof |
| S4 | Walk-forward randomized holdouts | Generalization testing | Verified proof |
| S5 | Live shadow market | Real-time behavior without orders | Deployment candidate |
| S6 | Venue testnet | Connector and operational testing | Operational candidate |
| S7 | Bounded live canary | Later, real financial risk | Live verified |

An agent may not be labeled “deployment-ready” based only on S1 or S2.

### 10.6 Simulation engine requirements

The engine must model:

- Starting equity and collateral.
- Position accounting.
- Realized and unrealized PnL.
- Fees.
- Funding.
- Spread.
- Configurable latency.
- Partial fills.
- Order expiry and cancellation.
- Tick and lot-size validation.
- Minimum trade notional.
- Reduce-only behavior.
- Margin and liquidation rules.
- Data outages.
- Rejected actions.
- No-op behavior.
- Deterministic replay for a fixed dataset, configuration, agent hash, and seed.

### 10.7 Fill-model honesty

The platform must not assume every strategy fills at the most favorable observed price. Every proof card declares:

- Price source.
- Fill algorithm.
- Latency.
- Queue assumptions.
- Slippage model.
- Fee model.
- Funding treatment.
- Missing-data treatment.

Optimistic fill models are labeled preliminary and excluded from verified leaderboards.

### 10.8 Agent action schema

Agents propose structured intents. They do not invoke arbitrary exchange methods.

```json
{
  "action": "PLACE_ORDER",
  "asset": "BTC",
  "side": "BUY",
  "order_type": "LIMIT",
  "quantity": "0.01",
  "limit_price": "65000",
  "reduce_only": false,
  "time_in_force": "GTC",
  "reason_code": "SIGNAL_ENTRY",
  "confidence": 0.63,
  "expires_at": "2026-06-19T15:30:00Z"
}
```

Every action passes through schema validation and an independent risk-policy engine.

### 10.9 Risk policy

MVP hard limits:

- Maximum 2x leverage.
- Maximum 10% gross equity exposure per new order unless protocol is stricter.
- Maximum 25% aggregate notional exposure per asset.
- Maximum 50% total gross notional.
- Daily simulated loss stop.
- Maximum open-order count.
- Maximum order frequency.
- No action during invalid or stale market state.
- No ability for the agent to edit its own limits.
- Emergency stop available to the user and platform.

Exact defaults are configuration values and require founder approval before release.

### 10.10 Baseline agents

Every evaluation compares the candidate against relevant baselines:

- Hold cash.
- Buy-and-hold with matched leverage.
- Periodic rebalance.
- Simple moving-average trend.
- Simple mean-reversion baseline.
- Random policy with the same action budget.
- User’s own historical behavior, when explicitly authorized and technically valid.

### 10.11 Trading scorecard

A scorecard includes:

- Gross return.
- Net return after fees, funding, and modeled slippage.
- Maximum drawdown.
- Volatility.
- Sharpe and Sortino ratios with stated sampling assumptions.
- CVaR or expected shortfall.
- Worst day and worst rolling period.
- Turnover.
- Trade count.
- Win/loss distribution.
- Exposure and leverage distribution.
- Liquidation proximity.
- Policy violations.
- Performance by regime.
- Performance by asset.
- Stability across windows and seeds.
- Baseline comparison.
- Confidence intervals or bootstrap intervals.
- Capacity estimate where possible.
- Simulation tier and proof level.

Raw PnL is never the sole ranking metric.

### 10.12 Anti-overfitting and anti-cheating controls

- Development and evaluation windows are separated.
- Private holdouts remain inaccessible to builders.
- Submission hashes freeze code, model, prompts, tools, and dependencies.
- Evaluated containers have restricted network access.
- Temporal leakage verifiers inspect feature timestamps and dataset boundaries.
- Evaluations use multiple windows and market regimes.
- Parameter-sensitivity tests detect brittle one-point tuning.
- Baseline and cost-model versions are fixed before a season.
- Repeated submissions are rate-limited or penalized to reduce leaderboard probing.
- Public results show the number of attempts and material protocol changes.

---

## 11. Agent package and skill system

### 11.1 Agent manifest

An agent package includes:

- Stable ID and semantic version.
- Author identity and signatures.
- Model and adapter references.
- System and task prompt policies.
- Code package hash.
- Tool allowlist.
- Skill dependencies.
- Observation and action schemas.
- Resource limits.
- Network policy.
- Data policy.
- License.
- Eval suite.
- Rollback policy.
- Known limitations.

### 11.2 Trading skill decomposition

Recommended reusable skills:

- Market-data validation.
- Regime classification.
- Feature construction.
- Hypothesis generation.
- Position sizing.
- Portfolio risk.
- Order selection.
- Execution.
- Exit management.
- Anomaly detection.
- Trade explanation.
- Postmortem generation.

Each skill has independent tests and versioned outputs.

### 11.3 Runtime architecture

```text
Research model or policy
→ structured intent
→ schema validator
→ deterministic risk policy
→ simulation/execution adapter
→ evidence recorder
→ verifiers
```

The LLM or model cannot:

- Access signing keys.
- Alter hard risk limits.
- Modify frozen protocol state.
- Call unapproved tools.
- Send arbitrary blockchain transactions.
- Promote its own version.

### 11.4 Sandboxing

Evaluated agents run with:

- Read-only package mount.
- Ephemeral writable workspace.
- CPU, memory, and time quotas.
- Denied network access by default.
- Explicitly proxied tool calls.
- Redacted secrets.
- Process and syscall restrictions appropriate to the runtime.
- Full action and tool-call logging.

---

## 12. Evaluation, verifiers, and proof levels

### 12.1 Verifier package

A verifier package includes:

- ID and version.
- Input and output schemas.
- Deterministic or statistical scoring logic.
- Calibration fixtures.
- Known false-positive and false-negative risks.
- Required evidence.
- Resource budget.
- Safety policy.
- Author and reviewer signatures.

### 12.2 Required MVP verifiers

1. Schema validity.
2. Dataset hash integrity.
3. Temporal leakage.
4. Accounting integrity.
5. Cost completeness.
6. Risk-policy compliance.
7. Reproducibility.
8. Baseline correctness.
9. Missing-data handling.
10. Agent dependency integrity.
11. Sandbox policy compliance.
12. Scorecard calculation integrity.

### 12.3 Proof levels

| Level | Label | Requirements |
|---|---|---|
| P0 | Private draft | Local artifacts; no public commitment |
| P1 | Committed | Signed manifest and timestamped hash |
| P2 | Auditable | Approved reviewers can access enough artifacts to inspect claims |
| P3 | Reproducible | Independent rerun meets declared tolerance |
| P4 | Replicated | Independent implementation or materially independent environment confirms the claim |
| P5 | Operational | Live shadow/testnet history and operational controls pass |
| P6 | Live verified | Later: bounded real deployment with complete policy and incident history |

### 12.4 Public proof card

Every proof card must show:

- Claim.
- Proof level.
- Agent and protocol versions.
- Evaluation mode and dates.
- Dataset disclosure level.
- Primary metrics.
- Baselines.
- Cost assumptions.
- Risk outcomes.
- Verifier pass/fail summary.
- Reproduction status.
- Challenges and disputes.
- Number of material development attempts when available.
- Chain commitment receipt.
- Limitations and simulation disclaimer.

### 12.5 Private IP and disclosure tiers

- **Private:** no public proof.
- **Committed private:** hashes are public; artifacts remain private.
- **Reviewer access:** encrypted access granted to approved reviewers.
- **Partial public:** selected method and evidence fields are public.
- **Open reproduction:** code, protocol, and data references are public where licensing permits.

A proof card must not imply reproducibility if reviewers cannot access sufficient evidence.

### 12.6 Challenge flow

1. Challenger selects a claim and challenge type.
2. Challenger states falsifiable grounds.
3. Optional challenge bond prevents spam.
4. Claim owner may provide reviewer access or a rebuttal.
5. Independent verifier or review panel evaluates the dispute.
6. Outcome becomes a signed graph event.
7. Reputation adjusts for valid challenges, honest corrections, or malicious claims.

---

## 13. Research ledger and blockchain design

### 13.1 Core rule

The blockchain stores **proofs about research**, not the research corpus itself.

### 13.2 On-chain objects

MVP on-chain records may include:

- Identity/wallet association or pseudonymous signer key.
- Research-protocol commitment.
- Agent-package commitment.
- Dataset-manifest commitment.
- Run and evidence Merkle roots.
- Verifier-report commitments.
- Review and challenge outcomes.
- Reward escrow and release.
- Reputation checkpoints.
- Permission grants and revocations for later deployments.

### 13.3 Off-chain objects

Remain off-chain:

- Raw market data.
- Full order books.
- Model weights and adapters.
- Private prompts.
- Source code not intentionally open-sourced.
- Raw wallet histories.
- Full decision traces.
- Private reviewer comments.
- Personal information and secrets.

### 13.4 Proof manifest

A `ProofManifest` includes:

- Canonical manifest version.
- Claim ID.
- Protocol hash.
- Agent hash.
- Dataset-manifest hash.
- Environment hash.
- Run trace Merkle root.
- Verifier-set hash.
- Scorecard hash.
- Disclosure policy.
- Author signature.
- Platform attestation, if applicable.
- Independent reviewer signatures, if applicable.
- Chain/network/transaction reference.

### 13.5 Chain strategy

The MVP should use a chain-adapter interface and anchor commitments to an established low-cost network. It should **not** begin by building a custom chain.

Progressive path:

1. Central orchestration with signed, exportable artifacts.
2. Public chain commitments and reward settlement.
3. Independent verifier runners.
4. Federated compute and data providers.
5. Permissionless research nodes and portable reputation proofs.
6. Consider a dedicated appchain only if throughput, economics, or governance demonstrably require it.

### 13.6 Why this can become the “best research blockchain”

The protocol optimizes for:

- Reproducibility.
- Provenance.
- Falsifiability.
- Review and challenge.
- Independent replication.
- Consent and privacy.
- Attribution of downstream impact.
- Rewarding useful failures.
- Portable agent and human reputation.

The chain’s value comes from the quality and reuse of committed research objects, not transaction volume.

---

## 14. Reputation and reward design

### 14.1 Reputation dimensions

Reputation is multidimensional:

- Builder quality.
- Verifier quality.
- Replication quality.
- Data quality.
- Reviewer reliability.
- Operational reliability.
- Safety record.
- Domain-specific expertise.
- Downstream reuse impact.

### 14.2 Reputation event rules

- Reputation events link to verifiable graph paths.
- Reputation cannot be directly purchased.
- Self-review and circular review have no or reduced weight.
- Independent confirmations receive higher weight.
- Reputation may decay when evidence becomes stale.
- Disputes and successful corrections remain visible.
- A failed experiment can contribute positively when honestly reported and reused.

### 14.3 Initial rewards

Use non-transferable research credits and fixed bounties before a token.

Credits may pay for:

- Simulation compute.
- Hidden evaluation runs.
- Storage.
- Reviewer access.
- Agent runtime.
- Marketplace trials.

Rewardable contributions:

- Winning a robust research season.
- Creating a reused skill.
- Creating a verifier that catches a valid defect.
- Replicating a result.
- Publishing a useful negative result.
- Contributing a high-quality dataset under an appropriate license.
- Discovering a simulator or protocol vulnerability.

Do not reward raw trading volume.

---

## 15. Product surfaces

### 15.1 Home / onboarding

- Clear simulated-capital message.
- Explain proof levels.
- Start with template or import agent.
- No wallet required for first local or hosted simulation unless abuse controls require an account.

### 15.2 Research Studio

- Question and protocol builder.
- Dataset selection.
- Agent and skill composition.
- Run configuration.
- Protocol validation.
- Version history.

### 15.3 Simulation Lab

- Historical replay.
- Live run status.
- Portfolio and risk state.
- Decision timeline.
- Fill and cost breakdown.
- Baseline comparison.
- Reproducibility controls.

### 15.4 Verifier Workshop

- Create and test verifier packages.
- Calibration fixtures.
- False-positive/negative tracking.
- Dependency and version management.

### 15.5 Proof Explorer

- Search proof cards.
- Filter by domain, proof level, metric, risk, author, verifier, and replication.
- View graph paths from claim to evidence and downstream reuse.

### 15.6 Agent Arena

- Seasonal challenges.
- Rules and datasets.
- Submission freeze.
- Live shadow leaderboard.
- Postmortems and challenge reports.

### 15.7 Reputation profile

- Contribution graph.
- Domain-specific scores.
- Verifier and replication history.
- Reuse paths.
- Dispute history.
- Exportable signed reputation bundle.

### 15.8 Deployment Center

MVP shows simulation and shadow status. Later it manages:

- Testnet connectors.
- Dedicated subaccounts.
- Agent-wallet permissions.
- Risk policies.
- Canary capital.
- Kill switches.
- Incident reports.

### 15.9 Founder Gate Dashboard

- Phase status.
- Automated check summary.
- Evidence packet.
- Known failures.
- Risk changes.
- UX screenshots/video.
- Approval decision and notes.
- Immutable review history.

---

## 16. System architecture

```text
Fractal Society Web / CLI
├── Research Studio
├── Simulation Lab
├── Verifier Workshop
├── Proof Explorer
├── Agent Arena
├── Reputation Profile
├── Deployment Center
└── Founder Gate Dashboard

Research Kernel
├── Protocol Registry
├── Artifact Registry
├── Experiment Orchestrator
├── Generic Environment Runtime
├── Agent Runtime and SkillOps
├── Verifier Engine
├── Scorecard Engine
├── Review / Challenge Service
├── Reputation Engine
└── Gate and Evidence Service

Domain Adapters
├── Deterministic Reference Adapter
├── Trading / Hyperliquid Adapter
└── Future adapters: software, forecasting, science, robotics

Data Plane
├── Market Recorder
├── Dataset Builder
├── Content-addressed Object Store
├── Metadata / Event Database
├── Time-series Analytics Store
└── Graph Projection

Trust and Settlement
├── Signatures
├── Commitment Adapter
├── Wallet and Permission Service
├── Reward Escrow
└── Reputation Checkpoints
```

### 16.1 Recommended storage split

- PostgreSQL: canonical metadata, permissions, phase gates, reviews.
- Content-addressed object storage: datasets, traces, reports, model artifacts.
- Parquet and/or analytical store: high-volume market events and run analytics.
- Append-only event log: state transitions and audit history.
- Graph projection: relationships and downstream impact; may begin in PostgreSQL before a dedicated graph database.
- Chain: commitments, signatures, reward state, and checkpoints.

### 16.2 Core services

#### Protocol Registry

Validates and versions research protocols.

#### Artifact Registry

Hashes, signs, stores, licenses, and resolves immutable artifacts.

#### Experiment Orchestrator

Schedules runs, freezes dependencies, allocates resources, and records lifecycle events.

#### Agent Runtime

Executes approved packages in a sandbox.

#### Verifier Engine

Runs deterministic and statistical verifier packages and records calibration.

#### Evidence Service

Builds immutable run bundles and public redacted views.

#### Commitment Service

Creates Merkle roots, gathers signatures, submits chain commitments, and monitors finality.

#### Gate Service

Runs phase acceptance checks and assembles founder-review packets.

### 16.3 Event model

Important events include:

- `protocol.created`
- `protocol.frozen`
- `dataset.committed`
- `agent.packaged`
- `run.queued`
- `run.started`
- `run.action_proposed`
- `run.action_rejected`
- `run.action_executed`
- `run.completed`
- `verifier.started`
- `verifier.completed`
- `proof.committed`
- `review.submitted`
- `challenge.opened`
- `challenge.resolved`
- `replication.completed`
- `reputation.updated`
- `reward.released`
- `gate.auto_passed`
- `gate.review_requested`
- `gate.approved`
- `gate.changes_requested`

Every write endpoint must be idempotent.

---

## 17. Self-improvement system

### 17.1 Structured decision record

Every meaningful agent decision produces:

- Research and protocol IDs.
- Dataset and market-state references.
- Agent, model, prompt, skill, and tool versions.
- Observation hash and authorized features.
- Proposed action.
- Risk-policy decision.
- Simulated or shadow execution outcome.
- Outcome over declared horizons.
- Counterfactual outcomes where valid.
- Verifier scores.
- Human feedback.
- Consent and reuse policy.

### 17.2 Counterfactual generation

For eligible decisions, the replay engine evaluates alternatives such as:

- No action.
- Half size.
- Different order type.
- Delayed entry.
- Earlier exit.
- Risk-policy rejection.

Counterfactuals are labeled simulated and never conflated with actual decisions.

### 17.3 Training-data admission

A trace enters a reusable training dataset only when:

- Consent allows the intended use.
- Secrets and personal information are removed.
- Data quality passes.
- Agent and environment versions are known.
- Leakage checks pass.
- Labels and outcome horizons are complete.
- The trace is weighted by reliability and novelty.

### 17.4 Champion/challenger promotion

```text
Approved traces
→ train challenger
→ public validation
→ private holdouts
→ adversarial verifiers
→ live shadow
→ human review
→ bounded promotion
```

The challenger cannot replace the champion based solely on training loss or simulated return.

---

## 18. Security, privacy, and abuse prevention

### 18.1 Threat model

- Malicious agent code.
- Prompt injection through data or tools.
- Secret extraction.
- Unauthorized tool or wallet use.
- Dataset poisoning.
- Backtest leakage.
- Score manipulation.
- Falsified proof manifests.
- Sybil accounts and collusive reviews.
- Denial of service and compute abuse.
- Inference of private strategy from public traces.
- Replay or nonce errors in later exchange integrations.
- Compromised verifier packages.

### 18.2 Required controls

- Default-deny permissions.
- Sandboxed execution.
- Egress controls.
- Signed and hashed artifacts.
- Reproducible builds where practical.
- Secret manager separated from agent processes.
- Independent policy engine.
- Immutable audit logs.
- Rate limits and quotas.
- Dependency scanning and software bills of materials.
- Reviewer conflict-of-interest declarations.
- Sybil and circular-review analysis.
- Challenge and correction process.
- Redacted public evidence.
- User-controlled deletion where legally and technically applicable, while preserving non-personal chain commitments.

### 18.3 Later live-deployment controls

- Dedicated subaccount with bounded capital.
- Dedicated API/agent wallet for one process.
- Signer isolated from the model runtime.
- Asset allowlist.
- Maximum leverage and notional.
- Daily loss limit.
- Order-rate limit.
- Expiring authorization.
- User and platform kill switches.
- Automatic pause on stale data or venue degradation.
- No withdrawal or transfer permission.
- Incident response and rollback runbook.

### 18.4 Privacy levels

- Local/private by default.
- Explicit opt-in for hosted compute.
- Separate consent for public proof, reviewer access, model training, and marketplace analytics.
- Wallet addresses treated as pseudonymous identifiers, not consent to deanonymize users.
- Raw prompts and private strategy traces hidden from marketplace buyers by default.

---

## 19. Compliance and communication requirements

This PRD is not legal advice. Before offering real-money deployment or marketing performance to regulated users, specialized counsel must review the product, jurisdiction, user flows, and compensation model.

MVP communication requirements:

- Clearly label simulated and hypothetical results.
- Display limitations near performance results, not only in terms of service.
- State material assumptions, including starting capital, fees, funding, slippage, latency, and reinvestment.
- Present actual and simulated results separately if actual results are later supported.
- Do not state or imply guaranteed returns.
- Do not rank solely by PnL.
- Do not market the platform as personalized financial advice.
- Prevent minors or restricted users from reaching later real-capital flows where applicable.

---

## 20. Growth and user-acquisition strategy

### 20.1 Initial hook

> **Can you train an agent to manage $100,000 of simulated capital?**

This is accessible to non-traders, creates a game-like entry point, and does not require a deposit.

### 20.2 Activation loop

```text
Choose a template
→ run with fake capital
→ get a verified scorecard
→ improve the agent
→ freeze a candidate
→ publish a proof card
→ invite a challenge
→ earn reputation and credits
```

### 20.3 First 100 users

- Recruit 20 active traders for private protocol and scorecard interviews.
- Recruit 20 AI-agent builders for SDK and sandbox feedback.
- Recruit 10 quant or computer-science clubs for a closed season.
- Recruit 10 verifier/security contributors.
- Fill remaining seats through public build-in-public content and referrals.

### 20.4 Season 0

Suggested challenge:

> Build an agent for BTC and ETH that survives multiple market regimes, stays below 10% maximum drawdown, and beats declared baselines after modeled fees, funding, and slippage.

Season structure:

- Two-week development period.
- Frozen submission.
- Private historical holdout.
- Two-week live shadow final.
- Public postmortems.
- Rewards for performance, robustness, useful verifiers, and valid failure discoveries.

### 20.5 Shareable growth objects

- Proof cards.
- Reproduction badges.
- Verifier-caught badges.
- “Beat your baseline” reports.
- Public postmortems.
- Agent cards with risk profile.
- Contributor reputation paths.

### 20.6 Referral reward

Reward referrals with simulation compute, hidden-evaluation credits, or additional private runs—not speculative tokens.

### 20.7 Builder distribution

- Open SDK and schemas.
- Starter agents.
- Dataset and verifier bounties.
- One-command local runner.
- Marketplace eligibility after proof gates.

---

## 21. Monetization strategy

### 21.1 Early revenue

- Hosted simulation subscription.
- Compute and storage credits.
- Private hidden-evaluation runs.
- Team workspaces.
- Sponsored research seasons.
- Premium data-quality and replay tiers.
- Private verifier and review services.

### 21.2 Later revenue

- Agent marketplace take rate.
- Skill and verifier marketplace take rate.
- Enterprise/private research network licensing.
- Guarded deployment subscription.
- Transparent venue builder fees where legally and operationally appropriate.

### 21.3 Avoid initially

- Performance fees.
- Pooled capital.
- Token emissions for activity.
- Paying for raw volume.
- Selling private user traces without explicit consent.

---

## 22. Metrics

### 22.1 North-star metric

**Weekly verified research contributions that create a reusable artifact or protect a downstream agent.**

### 22.2 Activation metrics

- Time to first valid simulation.
- Percentage of signups completing a run.
- Percentage of runs producing a valid scorecard.
- Percentage of users freezing a candidate.
- Percentage publishing a proof card.

### 22.3 Research-quality metrics

- Reproducibility rate.
- Required-verifier pass rate.
- Leakage detection rate.
- Percentage of proof cards with complete assumptions.
- Independent replication count.
- Useful negative-result count.
- Percentage of failures converted into verifier fixtures.

### 22.4 Network metrics

- Weekly active builders.
- Weekly active verifier authors.
- Skill reuse count.
- Verifier reuse count.
- Downstream-protection events.
- Challenge resolution time.
- Circular-review and Sybil detection precision.

### 22.5 Trading-domain metrics

- Agents surviving private holdout.
- Agents surviving live shadow.
- Cost-adjusted baseline outperformance.
- Policy-violation rate.
- Simulated liquidation rate.
- Score stability across regimes.
- Capacity and correlation concentration.

### 22.6 Platform metrics

- Run success rate.
- Replay determinism rate.
- Data-gap rate.
- Queue latency.
- Cost per run.
- Secret leakage incidents: target zero.
- Unauthorized action incidents: target zero.
- Proof commitment failure rate.

### 22.7 Genericity metrics

- Percentage of kernel tests run through reference adapter.
- Number of domain-specific imports in core: target zero.
- Time required to add a second adapter.
- Percentage of UI components using generic schemas.

---

## 23. Stage-gate delivery system

### 23.1 Purpose

Every implementation phase must be independently testable and must ask the founder for review before being considered complete.

### 23.2 Gate state machine

```text
DRAFT
→ IN_PROGRESS
→ AUTO_CHECKS_FAILED
or AUTO_CHECKS_PASSED
→ AWAITING_FOUNDER_REVIEW
→ APPROVED
or CHANGES_REQUESTED
or REJECTED
→ COMPLETE only after APPROVED
```

### 23.3 Gate CLI

Required commands:

```bash
fractal gate list
fractal gate run PHASE-03
fractal gate report PHASE-03 --format markdown
fractal gate request-review PHASE-03
fractal gate approve PHASE-03 --note "Approved for next phase"
fractal gate request-changes PHASE-03 --note "Fix replay gap handling"
```

Approval commands require founder authentication and a signature.

### 23.4 Evidence packet

Each phase produces:

```text
evidence/gates/PHASE-XX/<timestamp>/
├── gate-manifest.json
├── requirements-traceability.json
├── automated-checks.json
├── test-results.xml
├── coverage.json
├── metrics.json
├── security-report.json
├── screenshots/
├── demo-notes.md
├── known-issues.md
├── risk-delta.md
├── founder-review.md
└── signatures.json
```

### 23.5 Checklist item schema

Each checklist item includes:

- Stable ID.
- Requirement link.
- Description.
- Test type.
- Test command or manual procedure.
- Expected result.
- Evidence path.
- Severity.
- Owner.
- Status.
- Waiver policy.

### 23.6 Founder review prompt

The UI must ask:

> Automated checks for **[phase]** passed **[X/Y]** mandatory requirements. The major risks are **[summary]**. The demo and evidence packet are ready. Do you approve progression to **[next phase]**?

Choices:

- Approve.
- Request changes.
- Reject.

No default approval and no auto-expiry.

---

## 24. Reconciled phased implementation plan and testable acceptance gates


### 24.1 Gate semantics

- **REUSE-VERIFY:** The old tracker claims this capability exists. Do not rebuild it, but do not count it as complete until its regression command and runtime evidence pass.
- **EXTEND:** Keep the existing implementation and add a missing contract, hardening rule, integration, or domain behavior.
- **NEW:** No trustworthy implementation evidence was found in the legacy tracker; build and test it.
- **FOUNDER-DECISION:** Product or risk judgment that must be explicitly signed.
- **DISABLED-GUARD:** Work remains blocked until a separate future authorization changes its state.

A phase may enter `awaiting_founder_review` only when all mandatory automated and manual checks are `passed`. Legacy `verification_required` checks are not passes. A founder approval cannot waive a failed critical security, privacy, integrity, or unauthorized-spend check.

### 24.2 Delivery sequence

```text
PHASE-00  Founder scope approval
    ↓
PHASE-00R Verify the legacy baseline and lock reuse
    ↓
PHASE-01  Unify existing artifacts into the research contract
    ↓
PHASE-02  Adapt existing RL gyms into the generic kernel
    ↓
PHASE-03  Record trustworthy market data
    ↓
PHASE-04  Add the trading simulator domain adapter
    ↓
PHASE-05  Compose existing Forge/agent/skill systems into Agent Studio
    ↓
PHASE-06  Add trading-specific verifiers and hidden-eval hardening
    ↓
PHASE-07  Wrap existing digests in portable public proofs
    ↓
PHASE-08  Launch the fake-money Agent Arena
    ↓
PHASE-09  Add live shadow and testnet connectivity
    ↓
PHASE-10  Build the minimal research graph and reputation loop
    ↓
PHASE-11  Prove portability with software research
    ↓
PHASE-12  Real-capital canary, disabled until separately authorized
```

## PHASE-00 — Founder alignment and reconciled product constitution

### Objective

Approve the trading wedge, domain-neutral architecture, legacy-evidence policy, MVP boundary, and founder review process before implementation changes begin.

### Legacy assessment

**Reuse level:** `decision_only`

The master PRDs already define the broad mission, safety principles, Forge/FractalWork roles, and simulation-first direction. These are product claims, not implementation evidence.

### Reuse without rebuilding

- Existing mission, product thesis, privacy principles, and simulation-first non-goals.

### Remaining implementation

- Approve the reconciled scope and the rule that legacy checkmarks require regression evidence.
- Freeze the narrow first market and non-token reward policy.

### Mandatory gate checklist

- [ ] **P00-01 [FOUNDER-DECISION]** — Approve “Train in Simulation. Prove in Public. Deploy with Confidence.” as the first-use-case promise.
- [ ] **P00-02 [FOUNDER-DECISION]** — Approve an MVP that excludes real-money order submission and custody.
- [ ] **P00-03 [FOUNDER-DECISION]** — Approve the generic research kernel plus domain-adapter architecture.
- [ ] **P00-04 [FOUNDER-DECISION]** — Approve BTC and ETH perpetuals, $100,000 simulated USDC, and a default 2x leverage ceiling as initial defaults.
- [ ] **P00-05 [FOUNDER-DECISION]** — Approve using an existing low-cost chain adapter and deferring a custom chain.
- [ ] **P00-06 [FOUNDER-DECISION]** — Approve non-transferable compute/evaluation credits before any token.
- [ ] **P00-07 [FOUNDER-DECISION]** — Approve that the 650 checked legacy items are reuse claims only until code, tests, and runtime evidence pass PHASE-00R. Legacy refs: `mater-june-19th.md`.
- [ ] **P00-08 [FOUNDER-DECISION]** — Approve mandatory founder review after every phase and separate APPROVE_LIVE_CANARY authorization for PHASE-12.

### Automated commands

No automated command may substitute for the required founder decision. For PHASE-12, execution remains disabled.

### Founder demo

Review the revised scope map, legacy source summary, deferred work, and phase-gate state machine.

### Founder review questions

- Do you approve the reconciled scope and the distinction between legacy claims and verified completion?
- Are any legacy systems required as MVP blockers that this revision has deferred?

### Exit criteria

All P00 decisions are signed and the baseline audit is authorized.

---

## PHASE-00R — Legacy baseline verification and gap lock

### Objective

Convert the old checklist into trustworthy evidence, identify stale or false checkmarks, and freeze exactly what will be reused rather than rebuilt.

### Legacy assessment

**Reuse level:** `audit_all_existing`

The legacy tracker reports 1,666 items: 650 checked and 1,016 open. Completion is concentrated in RL gyms/verifiers and Hermes/OpenClaw; graph and blockchain layers are not evidenced by the checkbox tracker.

### Reuse without rebuilding

- All checked legacy capabilities that can be tied to current code, tests, and an executable runtime.

### Remaining implementation

- Build a machine-readable capability inventory.
- Downgrade unsupported checkmarks.
- Review manual PRDs that had no checkbox-formatted implementation evidence.

### Mandatory gate checklist

- [ ] **P00R-01 [REUSE-VERIFY]** — Tracker parser reproduces the legacy header totals (1,666 implementation items, 650 checked, 1,016 open) and also reports 1,682 parsed checkbox lines, including 16 manual-review placeholders. Legacy refs: `mater-june-19th.md`.
- [ ] **P00R-02 [REUSE-VERIFY]** — Every checked legacy item is mapped to a code path, test, migration, route, UI surface, or an explicit stale/false classification. Legacy refs: `mater-june-19th.md`.
- [ ] **P00R-03 [REUSE-VERIFY]** — RL gym/verifier smoke tests prove package validation, seeded replay, attached verifiers, scorecards, signed digests, and sandbox execution. Legacy refs: `docs/plans/2026-06-15-rl-gyms-and-verifiers-prd-checklist.md:64-84`; `docs/plans/2026-06-15-rl-gyms-and-verifiers-prd-checklist.md:88-111`; `docs/plans/2026-06-15-rl-gyms-and-verifiers-prd-checklist.md:219-231`; `docs/plans/2026-06-15-rl-gyms-and-verifiers-prd-checklist.md:417-426`.
- [ ] **P00R-04 [REUSE-VERIFY]** — Forge smoke tests prove dashboard access, model registry, trainer abstraction, evaluation, event streaming, and artifact export where claimed. Legacy refs: `docs/plans/2026-06-17-fractal-forge-local-engine-prd.md:22-32`; `docs/plans/2026-06-17-fractal-forge-local-engine-prd.md:195-208`; `docs/plans/2026-06-17-fractal-forge-local-engine-prd.md:353-381`; `docs/plans/2026-06-17-fractal-forge-local-engine-prd.md:387-405`.
- [ ] **P00R-05 [REUSE-VERIFY]** — Hermes/OpenClaw smoke tests prove connection, health checks, permission denial, verifier/gym execution, fallback, and secret isolation. Legacy refs: `docs/plans/2026-06-17-hermes-openclaw-agent-integration-prd.md:27-36`; `docs/plans/2026-06-17-hermes-openclaw-agent-integration-prd.md:279-290`; `docs/plans/2026-06-17-hermes-openclaw-agent-integration-prd.md:296-323`.
- [ ] **P00R-06 [REUSE-VERIFY]** — Skill/RAG smoke tests prove stable skill schemas, ingestion, retrieval, context bundles, and PRD/API/dashboard auditing where claimed. Legacy refs: `docs/plans/2026-06-19-digital-employee-skills-rag-prd.md:76-102`; `docs/plans/2026-06-19-digital-employee-skills-rag-prd.md:125-130`; `docs/plans/2026-06-19-digital-employee-skills-rag-prd.md:151-243`.
- [ ] **P00R-07 [REUSE-VERIFY]** — Router smoke tests prove only the claimed data model, index, classifier, outcome capture, and privacy controls; open filters, ranker, route API, and explanations remain open. Legacy refs: `docs/plans/2026-06-14-forge-specialist-agent-router-checklist.md:27-105`; `docs/plans/2026-06-14-forge-specialist-agent-router-checklist.md:109-188`; `docs/plans/2026-06-14-forge-specialist-agent-router-checklist.md:253-283`.
- [ ] **P00R-08 [EXTEND]** — FractalWork core, blockchain integration, repository feature graph, and marketplace PRDs receive a manual code-evidence review because their source documents did not expose reliable checkbox status. Legacy refs: `mater-june-19th.md`.
- [ ] **P00R-09 [NEW]** — A capability-inventory.json records capability id, implementation path, test command, runtime owner, evidence freshness, and status.
- [ ] **P00R-10 [NEW]** — Duplicate, superseded, demo-only, scaffold-only, and production-ready states are distinguished explicitly.
- [ ] **P00R-11 [EXTEND]** — Critical secret, permission, and sandbox regressions are fixed or block reuse.
- [ ] **P00R-12 [FOUNDER-DECISION]** — Founder signs the locked reuse map and remaining-gap map.

### Automated commands

```bash
pnpm fractal:audit:legacy -- --source mater-june-19th.md
pnpm test:legacy:rl-gym-smoke
pnpm test:legacy:forge-smoke
pnpm test:legacy:external-agent-smoke
pnpm test:legacy:skills-smoke
pnpm test:legacy:router-smoke
pnpm test:legacy:security-scan
```

### Founder demo

Open capability-inventory.json, select one checked item from each legacy source, navigate to its code and test evidence, then show one item downgraded because evidence was insufficient.

### Founder review questions

- Do you accept the verified baseline as the only source of implementation truth for later phases?
- Should any downgraded capability be repaired now rather than deferred?

### Exit criteria

Every inherited capability is verified, downgraded, or explicitly excluded; no phase relies on an ambiguous legacy checkbox.

---

## PHASE-01 — Canonical research schema and artifact ledger — extend existing packages

### Objective

Unify existing Forge, RL gym, verifier, suite, run, and agent artifacts into one domain-neutral research contract without rebuilding their working implementations.

### Legacy assessment

**Reuse level:** `substantial_reuse`

Verifier, gym, and suite package schemas are fully checked in the legacy tracker; versioning, signed package digests, immutable versions, run manifests, and scorecards are substantially present.

### Reuse without rebuilding

- VerifierPackage, RLGymPackage, SuitePackage, run manifests, package digests, visibility controls, scorecard objects, and agent harness metadata.

### Remaining implementation

- Add generic research objects and cross-artifact relations.
- Standardize canonical serialization, audit events, privacy contracts, and project export/import.
- Add changelog and migration rules that were still open.

### Mandatory gate checklist

- [ ] **P01-R01 [REUSE-VERIFY]** — Existing verifier, gym, and suite package fixtures validate without schema forks. Legacy refs: `docs/plans/2026-06-15-rl-gyms-and-verifiers-prd-checklist.md:64-84`; `docs/plans/2026-06-15-rl-gyms-and-verifiers-prd-checklist.md:88-111`; `docs/plans/2026-06-15-rl-gyms-and-verifiers-prd-checklist.md:115-127`.
- [ ] **P01-R02 [REUSE-VERIFY]** — Existing run manifests persist model, prompt, adapter, harness, package versions, and package digests. Legacy refs: `docs/plans/2026-06-15-rl-gyms-and-verifiers-prd-checklist.md:219-231`.
- [ ] **P01-R03 [REUSE-VERIFY]** — Existing immutable versions and signed package digests pass tamper and mutation regression tests. Legacy refs: `docs/plans/2026-06-15-rl-gyms-and-verifiers-prd-checklist.md:163-172`.
- [ ] **P01-R04 [REUSE-VERIFY]** — Existing public, unlisted, organization-only, and private visibility paths do not leak private payloads. Legacy refs: `docs/plans/2026-06-15-rl-gyms-and-verifiers-prd-checklist.md:163-172`.
- [ ] **P01-N01 [NEW]** — Canonical schemas exist for ResearchProject, Protocol, DatasetSnapshot, Environment, AgentPackage, SkillPackage, ExperimentRun, EvidenceBundle, VerifierRun, Review, Replication, and ProofManifest.
- [ ] **P01-N02 [NEW]** — Canonical serialization creates identical hashes across supported runtimes.
- [ ] **P01-N03 [NEW]** — One-byte tampering changes the artifact hash and fails signature verification.
- [ ] **P01-N04 [NEW]** — State transitions are idempotent and audit events identify actor, action, time, artifact, tenant, and request id.
- [ ] **P01-N05 [NEW]** — An exported research project bundle imports without semantic changes.
- [ ] **P01-N06 [EXTEND]** — Private artifacts never appear in public API, event, log, or scorecard fixtures.
- [ ] **P01-N07 [EXTEND]** — Every immutable package version requires a changelog and compatible migration metadata. Legacy refs: `docs/plans/2026-06-15-rl-gyms-and-verifiers-prd-checklist.md:163-172`.
- [ ] **P01-N08 [NEW]** — Requirements traceability maps every canonical schema and state transition to PRD check ids.

### Automated commands

```bash
pnpm test:schema
pnpm test:canonical-hash
pnpm test:signatures
pnpm test:artifact-registry
pnpm test:privacy-contracts
pnpm test:e2e:project-export-import
```

### Founder demo

Import a legacy gym and verifier package, wrap them in a ResearchProject, export it, alter one byte, and show that verification fails while the unmodified bundle round-trips.

### Founder review questions

- Does the unified artifact model preserve existing Forge/RL behavior while remaining clear enough for non-trading research?

### Exit criteria

The unified schema is backward-compatible with verified legacy artifacts and all new canonical integrity tests pass.

---

## PHASE-02 — Generic simulation kernel — adapt the existing RL Gym runner

### Objective

Turn the verified RL Gym runtime into the domain-neutral simulation kernel and prove it works without trading-specific imports.

### Legacy assessment

**Reuse level:** `substantial_reuse`

The old tracker marks the gym package, episode runner, seeded replay, reward trace viewer, attached verifiers, sandbox limits, run manifests, and scorecards as implemented. Several hardening items remained open.

### Reuse without rebuilding

- Episode runner, seeded replay, reward traces, attached verifiers, run manifests, sandbox execution, resource limits, and egress denial.

### Remaining implementation

- Create the domain adapter contract and deterministic reference adapter.
- Close raw-trace policy, flaky-run detection, invalid-action penalties, and safety-hard-failure gaps.

### Mandatory gate checklist

- [ ] **P02-R01 [REUSE-VERIFY]** — Existing episode runner, seeded replay, attached verifier, and reward-trace behavior passes regression tests. Legacy refs: `docs/plans/2026-06-15-rl-gyms-and-verifiers-prd-checklist.md:417-426`.
- [ ] **P02-R02 [REUSE-VERIFY]** — Existing sandbox enforces CPU, memory, wall-clock, storage, network, and explicit tool-permission limits. Legacy refs: `docs/plans/2026-06-15-rl-gyms-and-verifiers-prd-checklist.md:219-231`; `docs/plans/2026-06-15-rl-gyms-and-verifiers-prd-checklist.md:249-263`.
- [ ] **P02-R03 [REUSE-VERIFY]** — Existing run manifests and package digests reproduce a completed episode. Legacy refs: `docs/plans/2026-06-15-rl-gyms-and-verifiers-prd-checklist.md:219-231`.
- [ ] **P02-N01 [NEW]** — A versioned DomainAdapter contract defines observation, action, state transition, reward, termination, dataset, and verifier hooks.
- [ ] **P02-N02 [NEW]** — A deterministic non-trading reference adapter runs through the same public API as trading.
- [ ] **P02-N03 [NEW]** — Same agent, dataset, environment, configuration, and seed produce byte-equivalent critical outputs across 100 runs.
- [ ] **P02-N04 [NEW]** — Different seeds create controlled variation within declared tolerances.
- [ ] **P02-N05 [EXTEND]** — Invalid actions receive explicit penalties or hard failures according to protocol. Legacy refs: `docs/plans/2026-06-15-rl-gyms-and-verifiers-prd-checklist.md:235-245`.
- [ ] **P02-N06 [EXTEND]** — Raw traces are persisted or discarded exactly according to the selected privacy policy. Legacy refs: `docs/plans/2026-06-15-rl-gyms-and-verifiers-prd-checklist.md:219-231`.
- [ ] **P02-N07 [EXTEND]** — Repeated-seed tests identify flaky verifier or environment behavior and block verified status. Legacy refs: `docs/plans/2026-06-15-rl-gyms-and-verifiers-prd-checklist.md:219-231`.
- [ ] **P02-N08 [EXTEND]** — Safety violations can be configured as hard failures independent of reward score. Legacy refs: `docs/plans/2026-06-15-rl-gyms-and-verifiers-prd-checklist.md:235-245`.
- [ ] **P02-N09 [NEW]** — Architecture tests fail if trading or venue code enters the generic kernel.
- [ ] **P02-N10 [NEW]** — A completed run exports and replays on a clean environment from its manifest.

### Automated commands

```bash
pnpm test:kernel
pnpm test:determinism --repeat 100
pnpm test:sandbox-limits
pnpm test:reference-adapter:e2e
pnpm test:architecture-boundaries
pnpm test:flaky-environment-detection
```

### Founder demo

Run the deterministic reference environment twice from a legacy gym package, reproduce it from an exported manifest, then show an architecture test rejecting a trading import in the kernel.

### Founder review questions

- Does this phase convincingly prove that the research pipeline is generic before trading is added?

### Exit criteria

The existing gym runner has become the generic kernel, hardening gaps are closed, and the reference adapter passes all determinism tests.

---

## PHASE-03 — Hyperliquid market-data recorder

### Objective

Create a trustworthy, continuously monitored trading dataset adapter without changing the generic research kernel.

### Legacy assessment

**Reuse level:** `mostly_new`

No Hyperliquid recorder or market-dataset implementation is evidenced in the legacy checklist.

### Reuse without rebuilding

- Generic artifact manifests, run event formats, secret handling, observability conventions, and sandbox/network policy.

### Remaining implementation

- Implement the venue adapter, normalized market schema, gap detection, dataset manifests, rate-limit budgeting, and recorder health operations.

### Mandatory gate checklist

- [ ] **P03-N01 [NEW]** — Recorded trade, book, mark, funding, status, and instrument events validate against normalized schemas.
- [ ] **P03-N02 [NEW]** — The recorder reconnects after forced disconnect without silent loss or duplicate critical events.
- [ ] **P03-N03 [NEW]** — Duplicate upstream messages do not create duplicate normalized records.
- [ ] **P03-N04 [NEW]** — Sequence or time gaps create explicit data-quality events.
- [ ] **P03-N05 [NEW]** — Every normalized record can trace back to raw source evidence.
- [ ] **P03-N06 [NEW]** — Dataset manifests include source, time range, schema version, missingness, transformations, and content hash.
- [ ] **P03-N07 [NEW]** — REST and WebSocket use remains below configured safety margins.
- [ ] **P03-N08 [NEW]** — Seven continuous days of recording meet the founder-approved completeness threshold.
- [ ] **P03-N09 [NEW]** — Secrets and private user streams are absent from public market datasets.
- [ ] **P03-N10 [NEW]** — Recorder health, lag, gap, reconnect, throughput, and storage metrics alert correctly.

### Automated commands

```bash
pnpm test:hyperliquid:fixtures
pnpm test:recorder:reconnect
pnpm test:recorder:idempotency
pnpm test:data-gap-detection
pnpm test:dataset-manifest
pnpm test:rate-limit-budget
```

### Founder demo

Inspect a forced data gap, trace a normalized event to raw evidence, and generate a signed dataset manifest from a continuous capture window.

### Founder review questions

- Is the captured dataset honest enough about gaps and transformations to support public proof?

### Exit criteria

The recorder passes functional tests and the seven-day soak evidence is approved.

---

## PHASE-04 — Trading simulator and portfolio accounting on the generic kernel

### Objective

Implement realistic fake-money perpetual trading as a domain adapter using the generic episode runner, evidence model, and verifier hooks.

### Legacy assessment

**Reuse level:** `generic_runtime_reuse_domain_new`

The generic gym runner and score/reward traces can be reused, but trading accounting, fills, funding, margin, liquidation, and execution realism are new.

### Reuse without rebuilding

- Generic episode lifecycle, seed control, reward traces, artifact manifests, verifier hooks, and sandboxing.

### Remaining implementation

- Trading state/action schemas, portfolio ledger, fill models, fees, funding, margin, liquidation, baselines, and simulation disclosures.

### Mandatory gate checklist

- [ ] **P04-R01 [REUSE-VERIFY]** — Trading episodes run through the generic DomainAdapter and existing episode runner rather than a parallel orchestrator. Legacy refs: `docs/plans/2026-06-15-rl-gyms-and-verifiers-prd-checklist.md:417-426`.
- [ ] **P04-R02 [REUSE-VERIFY]** — Trading runs emit the same canonical run manifest, reward trace, and evidence-bundle structures as the reference adapter. Legacy refs: `docs/plans/2026-06-15-rl-gyms-and-verifiers-prd-checklist.md:219-231`.
- [ ] **P04-N01 [NEW]** — Cash, collateral, positions, realized PnL, unrealized PnL, fees, and funding reconcile at every step.
- [ ] **P04-N02 [NEW]** — Tick, lot, minimum-notional, leverage, margin, and reduce-only constraints are enforced.
- [ ] **P04-N03 [NEW]** — No fill occurs at a price unavailable under the selected fill model.
- [ ] **P04-N04 [NEW]** — Partial fills, cancellations, and order expiry behave deterministically.
- [ ] **P04-N05 [NEW]** — Liquidation and margin fixtures match hand-calculated expected outcomes.
- [ ] **P04-N06 [NEW]** — Data outages pause or reject actions according to protocol.
- [ ] **P04-N07 [NEW]** — Cost-free and cost-inclusive results differ as expected for golden scenarios.
- [ ] **P04-N08 [NEW]** — Cash, buy-and-hold, random, and simple moving-average baselines reproduce from frozen manifests.
- [ ] **P04-N09 [NEW]** — Candidates cannot change the fill, fee, data-quality, or risk model during evaluation.
- [ ] **P04-N10 [NEW]** — Scorecards label simulation tier, capital, leverage, fees, funding, slippage, latency, and data quality.
- [ ] **P04-N11 [NEW]** — Simulation and hypothetical-result disclosures appear beside every performance view.

### Automated commands

```bash
pnpm test:portfolio-ledger
pnpm test:orders
pnpm test:funding-fees
pnpm test:liquidation
pnpm test:fill-model
pnpm test:golden-scenarios
pnpm test:trading-replay:e2e
```

### Founder demo

Run a hand-calculated BTC scenario, inspect every ledger entry, then switch to a more conservative fill model and show the score change.

### Founder review questions

- Are the trading assumptions conservative and understandable enough for public competition?

### Exit criteria

Golden accounting, execution, and liquidation tests pass and the founder approves the default simulation assumptions.

---

## PHASE-05 — Trading Agent Studio on existing Forge, skills, and agent runtimes

### Objective

Compose the already-built Forge, imported-agent, skill, package, and sandbox capabilities into a simple trading-agent workflow instead of rebuilding an agent platform.

### Legacy assessment

**Reuse level:** `substantial_reuse`

Forge dashboard/model/training/eval surfaces, Hermes/OpenClaw adapters, normalized agent harnesses, secret controls, marketplace gates, skill schemas, and many sandbox controls are strongly represented as complete in the legacy tracker.

### Reuse without rebuilding

- Forge dashboard and packaging, model registry, trainer abstraction, base-vs-adapter evaluation, imported-agent harnesses, connector health, permission denial, secret isolation, skill registry, and sandbox runtime.

### Remaining implementation

- Add the trading agent manifest/action contract and starter templates.
- Unify native/imported-agent permissions with skill tool scopes.
- Complete a first-run UX and evidence-compatible local/hosted execution path.
- Do not make the unfinished generic router a blocker.

### Mandatory gate checklist

- [ ] **P05-R01 [REUSE-VERIFY]** — Verified Forge dashboard, model registry, training backend, evaluation, and export capabilities pass regression smoke tests. Legacy refs: `docs/plans/2026-06-17-fractal-forge-local-engine-prd.md:22-32`; `docs/plans/2026-06-17-fractal-forge-local-engine-prd.md:195-208`; `docs/plans/2026-06-17-fractal-forge-local-engine-prd.md:353-381`; `docs/plans/2026-06-17-fractal-forge-local-engine-prd.md:387-405`.
- [ ] **P05-R02 [REUSE-VERIFY]** — Verified Hermes/OpenClaw agent registration, harness mapping, permission denial, verifier/gym execution, and fallback pass regression tests. Legacy refs: `docs/plans/2026-06-17-hermes-openclaw-agent-integration-prd.md:27-36`; `docs/plans/2026-06-17-hermes-openclaw-agent-integration-prd.md:137-190`; `docs/plans/2026-06-17-hermes-openclaw-agent-integration-prd.md:279-290`.
- [ ] **P05-R03 [REUSE-VERIFY]** — Verified skill schemas, skill packs, ingestion, and retrieval work without trading-specific forks. Legacy refs: `docs/plans/2026-06-19-digital-employee-skills-rag-prd.md:76-102`; `docs/plans/2026-06-19-digital-employee-skills-rag-prd.md:125-130`; `docs/plans/2026-06-19-digital-employee-skills-rag-prd.md:151-243`.
- [ ] **P05-R04 [REUSE-VERIFY]** — Verified sandbox defaults deny network and undeclared side effects while enforcing quotas. Legacy refs: `docs/plans/2026-06-15-rl-gyms-and-verifiers-prd-checklist.md:219-231`; `docs/plans/2026-06-15-rl-gyms-and-verifiers-prd-checklist.md:249-263`.
- [ ] **P05-N01 [EXTEND]** — TradingAgentManifest extends the canonical AgentPackage/AgentHarness instead of defining a duplicate agent model.
- [ ] **P05-N02 [NEW]** — Agents can emit only the declared trading action schema and invalid fields are rejected before simulation.
- [ ] **P05-N03 [NEW]** — At least three starter agents run: no-trade, moving-average, and risk-guardian.
- [ ] **P05-N04 [EXTEND]** — Native, Hermes, OpenClaw, and local-model agents use one permission decision format and one denied-event schema. Legacy refs: `docs/plans/2026-06-17-hermes-openclaw-agent-integration-prd.md:194-203`; `docs/plans/2026-06-19-digital-employee-skills-rag-prd.md:294-325`.
- [ ] **P05-N05 [EXTEND]** — Local and hosted runs produce evidence bundles that are semantically compatible.
- [ ] **P05-N06 [NEW]** — Agent, prompt, model, skill, tool, dataset, and dependency versions freeze at candidate submission.
- [ ] **P05-N07 [NEW]** — A malicious fixture cannot modify protocol, verifier, risk policy, or hidden evaluation state.
- [ ] **P05-N08 [NEW]** — A first-time user modifies and runs a starter agent, then interprets its scorecard in under 20 minutes without staff control.
- [ ] **P05-N09 [FOUNDER-DECISION]** — Full specialist-router filters, ranker, route API, and explanations are explicitly deferred from the first trading thin slice unless a user journey requires them. Legacy refs: `docs/plans/2026-06-14-forge-specialist-agent-router-checklist.md:109-188`.

### Automated commands

```bash
pnpm test:agent-manifest
pnpm test:action-schema
pnpm test:sandbox-security
pnpm test:permissions
pnpm test:local-hosted-compatibility
pnpm test:malicious-agent-fixtures
pnpm test:trading-agent-starter:e2e
```

### Founder demo

Import one external agent and one native starter, deny an undeclared tool, run both through the same simulation contract, and export comparable evidence bundles.

### Founder review questions

- Is agent creation simple enough while clearly reusing the existing Forge and agent-runtime systems?
- Do you approve deferring the unfinished general router from the trading MVP?

### Exit criteria

The first-run trading-agent journey works for native and imported agents and all permission/sandbox regressions pass.

---

## PHASE-06 — Trading verifier pack and hidden evaluation on the existing verifier engine

### Objective

Reuse the existing verifier/gym/scorecard infrastructure and add only the trading-specific adversarial evaluations required for credible public proof.

### Legacy assessment

**Reuse level:** `substantial_reuse_hardening_required`

Verifier packages, suites, scorecards, baseline runs, public/private controls, hidden holdout definitions, score provenance, and training-loop use are substantially checked. Launch acceptance, isolation, dry-run UX, flaky detection, and several hard-failure controls remain open.

### Reuse without rebuilding

- Verifier runtime, suite packaging, baseline comparisons, scorecards, public/private disclosure, hidden episode metadata, calibration, score provenance, batch runs, and improvement feedback.

### Remaining implementation

- Implement trading-specific verifiers and leakage attacks.
- Harden holdout isolation and probing defenses.
- Close launch-level privacy and sandbox acceptance criteria.

### Mandatory gate checklist

- [ ] **P06-R01 [REUSE-VERIFY]** — Existing verifier packages, suite execution, baseline runs, scorecards, and score provenance pass regression tests. Legacy refs: `docs/plans/2026-06-15-rl-gyms-and-verifiers-prd-checklist.md:64-84`; `docs/plans/2026-06-15-rl-gyms-and-verifiers-prd-checklist.md:115-127`; `docs/plans/2026-06-15-rl-gyms-and-verifiers-prd-checklist.md:235-245`.
- [ ] **P06-R02 [REUSE-VERIFY]** — Existing public/private scorecard controls and immutable package versions pass disclosure regression tests. Legacy refs: `docs/plans/2026-06-15-rl-gyms-and-verifiers-prd-checklist.md:163-172`.
- [ ] **P06-R03 [REUSE-VERIFY]** — Existing hidden holdout episode metadata remains hidden from the candidate and seller interfaces. Legacy refs: `docs/plans/2026-06-15-rl-gyms-and-verifiers-prd-checklist.md:88-111`.
- [ ] **P06-R04 [REUSE-VERIFY]** — Existing base-vs-adapter and agent-version comparison flows can compare trading-agent versions without special-case UI. Legacy refs: `docs/plans/2026-06-17-fractal-forge-local-engine-prd.md:387-405`; `docs/plans/2026-06-15-rl-gyms-and-verifiers-prd-checklist.md:430-436`.
- [ ] **P06-N01 [NEW]** — Every required trading verifier has known-positive, known-negative, and adversarial fixtures.
- [ ] **P06-N02 [NEW]** — Look-ahead, survivorship, label, overlap, and parameter-selection leakage fixtures are detected.
- [ ] **P06-N03 [NEW]** — Accounting corruption, impossible fills, omitted fees/funding, and stale-data trading fail verified status.
- [ ] **P06-N04 [NEW]** — Private evaluation datasets and answers are inaccessible to candidate code, logs, errors, and timing channels within the threat model.
- [ ] **P06-N05 [EXTEND]** — Scorecard calculations reproduce from the evidence bundle and pinned verifier versions.
- [ ] **P06-N06 [NEW]** — Multiple windows and regimes, confidence limits, sample size, turnover, drawdown, and capacity warnings are visible.
- [ ] **P06-N07 [NEW]** — Leaderboard ranking uses risk, robustness, costs, uncertainty, and violations rather than raw PnL alone.
- [ ] **P06-N08 [NEW]** — Submission quotas, cooldowns, attempt history, and delayed feedback reduce holdout probing.
- [ ] **P06-N09 [NEW]** — A deliberately overfit strategy passes public training data but fails private promotion.
- [ ] **P06-N10 [EXTEND]** — Internal adversarial tests report zero sandbox escapes before public launch. Legacy refs: `docs/plans/2026-06-15-rl-gyms-and-verifiers-prd-checklist.md:440-447`.
- [ ] **P06-N11 [EXTEND]** — Public scorecards include versions, manifests, cost, latency, privacy status, assumptions, and no private traces or hidden answers. Legacy refs: `docs/plans/2026-06-15-rl-gyms-and-verifiers-prd-checklist.md:440-447`.

### Automated commands

```bash
pnpm test:verifiers
pnpm test:leakage-adversarial
pnpm test:hidden-holdout-isolation
pnpm test:scorecard-reproduction
pnpm test:ranking
pnpm test:overfit-strategy
pnpm test:public-scorecard-privacy
```

### Founder demo

Submit a profitable but leaked strategy, show its preliminary score, then show the hidden-evaluation rejection and the exact non-secret reason.

### Founder review questions

- Would a skeptical builder understand and be able to challenge why an agent passed or failed?

### Exit criteria

Trading verifier and holdout hardening tests pass, public-card privacy passes, and founder accepts the ranking policy.

---

## PHASE-07 — Proof registry and minimal chain commitments around existing digests

### Objective

Wrap existing immutable package digests and artifact cards in a portable research proof manifest and minimal chain adapter, without implementing the full Graph OS or a custom chain.

### Legacy assessment

**Reuse level:** `partial_reuse`

Signed package digests, immutable versions, artifact cards/catalog, public/private controls, and scorecard attachments are present. The graph, proof registry, chain commitments, reward settlement, and explorer are not evidenced as implemented.

### Reuse without rebuilding

- Package hashes, signed digests, artifact cards, visibility tiers, scorecard rendering, and agent listing metadata.

### Remaining implementation

- Proof manifest, Merkle commitment, reviewer grants, chain adapter, finality monitor, verification CLI, and minimal proof explorer.
- Keep graph projection minimal and relational until PHASE-10.

### Mandatory gate checklist

- [ ] **P07-R01 [REUSE-VERIFY]** — Existing package digests, immutable versions, artifact cards, and public/private controls pass regression tests. Legacy refs: `docs/plans/2026-06-15-rl-gyms-and-verifiers-prd-checklist.md:163-172`.
- [ ] **P07-N01 [NEW]** — Every public proof resolves to a signed ProofManifest that references immutable evidence and verifier versions.
- [ ] **P07-N02 [NEW]** — Changing any committed artifact is detected by the independent verification CLI.
- [ ] **P07-N03 [NEW]** — A chain receipt maps to the correct proof manifest and network/finality metadata.
- [ ] **P07-N04 [NEW]** — Private artifacts are not leaked through metadata, URLs, logs, events, cache keys, or error messages.
- [ ] **P07-N05 [NEW]** — Reviewer-access grants can be issued, audited, expired, and revoked without altering the original commitment.
- [ ] **P07-N06 [NEW]** — Proof level is calculated from evidence and review state rather than chosen by the author.
- [ ] **P07-N07 [EXTEND]** — Public proof cards show assumptions, limitations, disclosure tier, and reproduction status.
- [ ] **P07-N08 [NEW]** — Explorer shows protocol, run, verifier, proof, review, challenge, replication, and version relationships using a minimal projection.
- [ ] **P07-N09 [NEW]** — A proof can be exported and verified without trusting the web UI or hosted API.
- [ ] **P07-N10 [NEW]** — Chain outages queue commitments without losing signed local evidence or changing proof order.
- [ ] **P07-D01 [FOUNDER-DECISION]** — A custom layer-one chain remains out of scope; only an adapter to an established chain is authorized.

### Automated commands

```bash
pnpm test:proof-manifest
pnpm test:merkle-integrity
pnpm test:chain-adapter
pnpm test:disclosure-boundaries
pnpm test:proof-cli:e2e
pnpm test:chain-outage-recovery
```

### Founder demo

Publish a committed-private proof, verify it locally, grant and revoke reviewer access, then queue a commitment during a simulated chain outage.

### Founder review questions

- Does public proof create verifiability without forcing builders to reveal proprietary strategies?

### Exit criteria

Portable proof verification works, disclosure tests pass, and founder approves the commitment chain and proof-card defaults.

---

## PHASE-08 — Agent Arena and first-user launch using existing cards and scorecards

### Objective

Compose existing agent cards, scorecards, package freezes, and evaluation runs into a credible fake-money season that attracts real builders.

### Legacy assessment

**Reuse level:** `partial_reuse`

Artifact catalogs, cards, scorecard attachment, sandbox trials, imported-agent listing gates, outcome records, and privacy controls exist. Season state, robust ranking, external launch acceptance, and contribution rewards are new.

### Reuse without rebuilding

- Agent/artifact cards, scorecard renderer, evaluation runner, package freezing, imported-agent support, outcome records, metering records, and privacy controls.

### Remaining implementation

- Season rules/state machine, submission freeze, private final, robust leaderboard, appeals, research credits, postmortems, and launch operations.

### Mandatory gate checklist

- [ ] **P08-R01 [REUSE-VERIFY]** — Existing agent/artifact cards render verified suite scorecards for both native and imported agents. Legacy refs: `docs/plans/2026-06-15-rl-gyms-and-verifiers-prd-checklist.md:163-172`; `docs/plans/2026-06-17-hermes-openclaw-agent-integration-prd.md:296-323`.
- [ ] **P08-R02 [REUSE-VERIFY]** — Existing outcome and privacy records attach to arena submissions without retaining raw private prompts by default. Legacy refs: `docs/plans/2026-06-14-forge-specialist-agent-router-checklist.md:253-283`.
- [ ] **P08-N01 [NEW]** — Season rules are immutable after submission opens except through a visible, signed emergency amendment.
- [ ] **P08-N02 [NEW]** — Candidate submissions freeze all material dependencies and attempt history.
- [ ] **P08-N03 [NEW]** — Private holdout access is isolated and audited.
- [ ] **P08-N04 [NEW]** — Leaderboard exposes return, drawdown, tail risk, costs, robustness, uncertainty, capacity, and policy violations.
- [ ] **P08-N05 [NEW]** — Disqualified agents receive a non-secret reason and appeal path.
- [ ] **P08-N06 [NEW]** — Rewards can go to agent, verifier, replication, security, dataset, and useful negative-result contributions.
- [ ] **P08-N07 [NEW]** — At least 20 external builders submit a valid candidate.
- [ ] **P08-N08 [NEW]** — At least 70% of accepted submissions complete automated evaluation.
- [ ] **P08-N09 [NEW]** — At least five users publish a proof card or postmortem.
- [ ] **P08-N10 [NEW]** — No unresolved critical security or data-integrity incident exists at launch review.
- [ ] **P08-N11 [NEW]** — Moderated interviews show users view the scorecard as more credible than a PnL screenshot.
- [ ] **P08-N12 [NEW]** — Simulation limitations are visible in every shareable performance view.

### Automated commands

```bash
pnpm test:season-state-machine
pnpm test:submission-freeze
pnpm test:leaderboard
pnpm test:reward-allocation
pnpm test:share-card-disclosures
pnpm test:arena:e2e
```

### Founder demo

Run a miniature season from creation through frozen submission, hidden evaluation, robust ranking, proof publication, appeal, and credit allocation.

### Founder review questions

- Is the Arena credible enough to attract serious builders without encouraging reckless or misleading behavior?

### Exit criteria

Season 0 acceptance metrics pass and founder approves public launch.

---

## PHASE-09 — Live shadow mode and Hyperliquid testnet connector using the existing runtime-adapter pattern

### Objective

Reuse the normalized external-runtime, health, secret, permission, and fallback patterns to add venue connectivity without exposing order submission in shadow mode.

### Legacy assessment

**Reuse level:** `runtime_pattern_reuse_domain_new`

External agent connector contracts, secret encryption, read-only health checks, permission denial, fallback, and default no-wallet-spend behavior are strongly checked. Hyperliquid market/account reconciliation and shadow/testnet execution are new.

### Reuse without rebuilding

- Connector interface pattern, encrypted secrets, server-side invocation, health checks, permission events, fallback status, runtime telemetry, and default no wallet spend.

### Remaining implementation

- Venue adapter, live shadow portfolio, testnet order path, reconciliation, stale-data stops, emergency controls, and nonce/order lifecycle handling.

### Mandatory gate checklist

- [ ] **P09-R01 [REUSE-VERIFY]** — Venue connector uses the verified runtime-adapter health, secret, permission, and fallback conventions rather than a separate secret path. Legacy refs: `docs/plans/2026-06-17-hermes-openclaw-agent-integration-prd.md:137-190`; `docs/plans/2026-06-17-hermes-openclaw-agent-integration-prd.md:194-203`; `docs/plans/2026-06-17-hermes-openclaw-agent-integration-prd.md:279-290`.
- [ ] **P09-R02 [REUSE-VERIFY]** — Shadow mode retains the verified default of no wallet spend and no state-changing venue calls. Legacy refs: `docs/plans/2026-06-17-hermes-openclaw-agent-integration-prd.md:194-203`.
- [ ] **P09-N01 [NEW]** — Shadow mode consumes live public market data but cannot submit, sign, or simulate submitting a mainnet order.
- [ ] **P09-N02 [NEW]** — Shadow decisions record observation, proposed action, rejection reason, simulated fill, and market outcome.
- [ ] **P09-N03 [NEW]** — Stale, gapped, or degraded data pauses decision and execution paths.
- [ ] **P09-N04 [NEW]** — Testnet credentials and signing material never reach browser or model processes.
- [ ] **P09-N05 [NEW]** — Testnet order create, cancel, reduce-only, and status flows pass fixtures and integration tests.
- [ ] **P09-N06 [NEW]** — Local state reconciles with venue account and open-order state after reconnect and restart.
- [ ] **P09-N07 [NEW]** — Rate-limit, nonce, retry, duplicate, and rejected-order behavior is explicit and idempotent.
- [ ] **P09-N08 [NEW]** — Emergency stop prevents new actions and cancels eligible outstanding test orders.
- [ ] **P09-N09 [NEW]** — Shadow/testnet scorecards remain separate from historical simulation and any future live results.
- [ ] **P09-N10 [NEW]** — Seven continuous days of shadow operation complete without an unauthorized state-changing call.
- [ ] **P09-N11 [NEW]** — Incident and rollback runbooks are tested in a game day.

### Automated commands

```bash
pnpm test:shadow-mainnet-isolation
pnpm test:stale-data-stop
pnpm test:connector-secrets
pnpm test:testnet-orders
pnpm test:reconciliation
pnpm test:emergency-stop
```

### Founder demo

Run live shadow decisions, induce stale data, show the pause, then execute and reconcile a bounded testnet order while proving mainnet submission is impossible.

### Founder review questions

- Is the connector operationally trustworthy enough to proceed to full research-network features, while still prohibiting real capital?

### Exit criteria

Shadow soak, testnet integration, reconciliation, secret, and emergency-stop tests pass.

---

## PHASE-10 — Minimal research graph, review, replication, reputation, and rewards

### Objective

Build the smallest domain-neutral graph and reputation layer needed to turn proofs, reviews, replications, and downstream reuse into explainable reputation.

### Legacy assessment

**Reuse level:** `mostly_new`

The Graph Operating System tracker is 0/343 checked. Some supporting outcome records, human adjudication, privacy controls, and reward-submission skills exist and can be reused, but graph storage, reputation, fraud analysis, replication, and on-chain checkpoints are new.

### Reuse without rebuilding

- Routing outcome records, privacy controls, human-review capability, score provenance, reward-submission-preparer skill, and proof manifests.

### Remaining implementation

- Minimal graph schema/storage, review/conflict rules, replication, reputation events, fraud checks, reward gates, explainability, and checkpoint commitments.
- Defer advanced centrality dashboards, DeSci views, and full graph analytics.

### Mandatory gate checklist

- [ ] **P10-R01 [REUSE-VERIFY]** — Verified run/routing outcome records can link agent, request, run, verifier, and delayed outcome without exposing raw private requests. Legacy refs: `docs/plans/2026-06-14-forge-specialist-agent-router-checklist.md:253-283`.
- [ ] **P10-R02 [REUSE-VERIFY]** — Verified reward-submission and human-adjudication components can package evidence for review. Legacy refs: `docs/plans/2026-06-19-digital-employee-skills-rag-prd.md:76-102`; `docs/plans/2026-06-15-rl-gyms-and-verifiers-prd-checklist.md:235-245`.
- [ ] **P10-N01 [NEW]** — Minimal graph nodes cover Person, Agent, Skill, Dataset, Protocol, Run, Evidence, VerifierRun, Proof, Review, Replication, Reward, and ReputationCheckpoint. Legacy refs: `docs/plans/2026-06-19-fractal-society-graph-operating-system-prd.md:26-569`.
- [ ] **P10-N02 [NEW]** — Minimal graph edges cover created, used, trained_on, evaluated_by, verified_by, reviewed_by, challenged_by, replicated_by, reused_by, rewarded, and signed_by. Legacy refs: `docs/plans/2026-06-19-fractal-society-graph-operating-system-prd.md:26-569`.
- [ ] **P10-N03 [NEW]** — Graph writes are idempotent, tenant-scoped, auditable, and projected from canonical events.
- [ ] **P10-N04 [NEW]** — Review conflict-of-interest rules reject self-review, direct financial conflict, and configured collusion patterns.
- [ ] **P10-N05 [NEW]** — A replication references the exact protocol, datasets, environment, agent, and verifier versions it reproduces.
- [ ] **P10-N06 [NEW]** — Reputation is event-based, explainable by graph paths, reversible after successful challenges, and independent of token balance.
- [ ] **P10-N07 [NEW]** — Sybil, circular-review, repeated-collaborator, and suspicious reward-loop fixtures trigger warnings or blocks.
- [ ] **P10-N08 [NEW]** — Rewards release only after configured verifier, review, and challenge-window conditions pass.
- [ ] **P10-N09 [NEW]** — Useful failed experiments, verifier catches, replications, and downstream protections can earn bounded credit.
- [ ] **P10-N10 [NEW]** — A public reputation profile reveals evidence paths without leaking private graph content.
- [ ] **P10-N11 [NEW]** — Graph/reputation checkpoint commitments verify against the off-chain graph snapshot.
- [ ] **P10-D01 [FOUNDER-DECISION]** — Advanced graph analytics, DeSci claim views, and a full learned router are deferred until the minimal graph proves useful. Legacy refs: `docs/plans/2026-06-19-fractal-society-graph-operating-system-prd.md:26-569`.

### Automated commands

```bash
pnpm test:reputation-events
pnpm test:review-conflicts
pnpm test:sybil-fixtures
pnpm test:replication
pnpm test:reward-gates
pnpm test:reputation-explainability
pnpm test:graph-minimal:e2e
```

### Founder demo

Trace a proof through review and replication to a reputation update and credit release, then reverse it after a successful challenge and show the explanation path.

### Founder review questions

- Is the minimal graph understandable and useful enough to justify expanding toward the full research blockchain?

### Exit criteria

Minimal graph, review, replication, reputation, fraud, and reward gates pass; founder approves deferred/full graph scope.

---

## PHASE-11 — Second-domain portability proof using existing coding gyms and repo skills

### Objective

Prove the pipeline is genuinely general by running software issue repair through the same kernel, artifacts, verifiers, proofs, reviews, and reputation path.

### Legacy assessment

**Reuse level:** `partial_reuse`

Coding/tool-use gym templates, external-agent evaluation, core repo skills, PRD/API/dashboard auditors, and context-building components are checked. The end-to-end second-domain proof and repository graph implementation are not fully evidenced.

### Reuse without rebuilding

- Coding gym template, tool-use runner, repo-map/file-finder/test-selector/change-summary skills, imported agents, sandbox, scorecards, and proof registry.

### Remaining implementation

- Software-domain adapter, repository fixture, hidden tests, patch evidence, proof-card rendering, and cross-domain architecture enforcement.

### Mandatory gate checklist

- [ ] **P11-R01 [REUSE-VERIFY]** — Existing coding/tool-use gym templates and seeded runner pass regression tests. Legacy refs: `docs/plans/2026-06-15-rl-gyms-and-verifiers-prd-checklist.md:417-426`.
- [ ] **P11-R02 [REUSE-VERIFY]** — Existing repo skills and imported-agent evaluation paths solve a simple fixture without trading dependencies. Legacy refs: `docs/plans/2026-06-19-digital-employee-skills-rag-prd.md:76-102`; `docs/plans/2026-06-17-hermes-openclaw-agent-integration-prd.md:27-36`.
- [ ] **P11-N01 [NEW]** — Software adapter uses the same DomainAdapter, Protocol, Run, EvidenceBundle, VerifierRun, ProofManifest, Review, and Replication contracts.
- [ ] **P11-N02 [NEW]** — Architecture tests fail if trading types leak into generic schemas, services, UI components, or gate logic.
- [ ] **P11-N03 [NEW]** — A seeded repository issue can be solved in a sandbox and checked with public plus hidden tests.
- [ ] **P11-N04 [NEW]** — Software scorecard reports correctness, test coverage, regression risk, cost, latency, reproducibility, and permission violations.
- [ ] **P11-N05 [NEW]** — A software proof card publishes and verifies through the same proof registry and chain adapter.
- [ ] **P11-N06 [NEW]** — A reviewer can challenge and independently replicate the software result.
- [ ] **P11-N07 [NEW]** — At least one software result creates a valid reputation and downstream-reuse event.
- [ ] **P11-N08 [NEW]** — A second domain is added through adapter/package registration without modifying the generic kernel.

### Automated commands

```bash
pnpm test:software-adapter
pnpm test:cross-domain-kernel
pnpm test:generic-ui-rendering
pnpm test:software-proof:e2e
pnpm test:coding-gym-regression
```

### Founder demo

Run a hidden-test software issue through an imported agent, publish a proof, replicate it, and show that no trading service was invoked.

### Founder review questions

- Does this demonstration justify claiming Fractal Society is a general research pipeline rather than a trading product?

### Exit criteria

The software adapter passes the same proof path and architecture boundaries as trading.

---

## PHASE-12 — Guarded real-capital canary — disabled until separately authorized

### Objective

Permit a tightly bounded live-capital experiment only after a separate signed decision, legal/security review, and all safety preconditions pass.

### Legacy assessment

**Reuse level:** `permission_reuse_execution_new`

Imported-agent sub-wallet, spend-limit, revocation, permission-denial, and secret-encryption features are claimed complete. Live exchange signing, independent risk firewalls, canary capital controls, and operational authorization are new and remain disabled.

### Reuse without rebuilding

- Verified wallet/tool permission records, sub-wallet assignment, spending limits, revocation, denied events, secret isolation, and incident telemetry.

### Remaining implementation

- Legal review, isolated signer, dedicated venue account/subaccount, risk firewall, canary policy, withdrawals disabled, kill switches, monitoring, and postmortem.

### Mandatory gate checklist

- [ ] **P12-R01 [REUSE-VERIFY]** — Existing sub-wallet, spend-limit, revocation, denial-event, and secret-isolation behavior passes a fresh security regression. Legacy refs: `docs/plans/2026-06-17-hermes-openclaw-agent-integration-prd.md:194-203`; `docs/plans/2026-06-17-hermes-openclaw-agent-integration-prd.md:279-290`.
- [ ] **P12-D01 [DISABLED-GUARD]** — Founder signs APPROVE_LIVE_CANARY for an exact agent, account, venue, jurisdiction, capital limit, time window, and risk policy.
- [ ] **P12-N01 [DISABLED-GUARD]** — Specialized legal and compliance review approves the exact product and user flow.
- [ ] **P12-N02 [DISABLED-GUARD]** — The model runtime cannot access private keys or submit arbitrary transactions.
- [ ] **P12-N03 [DISABLED-GUARD]** — An independent deterministic risk firewall enforces asset allowlist, notional, leverage, daily loss, order rate, and expiry.
- [ ] **P12-N04 [DISABLED-GUARD]** — The canary uses a dedicated bounded account or subaccount with no withdrawal or transfer permission.
- [ ] **P12-N05 [DISABLED-GUARD]** — User and platform kill switches work during a game day.
- [ ] **P12-N06 [DISABLED-GUARD]** — Stale data, venue degradation, reconciliation failure, or policy mismatch pauses all new orders.
- [ ] **P12-N07 [DISABLED-GUARD]** — Every signed intent, policy decision, order, fill, rejection, state transition, and incident is auditable.
- [ ] **P12-N08 [DISABLED-GUARD]** — Champion/challenger promotion cannot change live code or policy without a new gate.
- [ ] **P12-N09 [DISABLED-GUARD]** — Live results are displayed separately from simulation, holdout, shadow, and testnet results.
- [ ] **P12-N10 [DISABLED-GUARD]** — A post-canary review decides stop, repeat, expand, or roll back; expansion is never automatic.

### Automated commands

No automated command may substitute for the required founder decision. For PHASE-12, execution remains disabled.

### Founder demo

Disabled. A future authorized demo must show isolated signing, forced policy rejection, kill switch, reconciliation, and complete audit replay.

### Founder review questions

- Do not review for approval until every precondition is evidenced and the exact live canary is specified.

### Exit criteria

No exit is possible without the separate APPROVE_LIVE_CANARY decision and all blocked checks becoming passed.

---


## 25. MVP release definition

The first public MVP is complete only after `PHASE-00`, `PHASE-00R`, and `PHASE-01` through `PHASE-08` are approved. It does **not** require completion of every legacy PRD.

A user must be able to:

1. Create a generic research project from the unified artifact model.
2. Build, import, or select a trading agent using existing Forge and runtime capabilities.
3. Run the agent with simulated capital through the generic simulation kernel.
4. Compare results against frozen baselines after declared costs.
5. Pass or fail trading-specific verifiers and private holdouts.
6. Freeze the agent, protocol, dataset, environment, and verifier versions.
7. Publish a signed proof card backed by an independently verifiable proof manifest.
8. Enter a fake-money Agent Arena and receive a transparent ranking.
9. Earn non-transferable research credits and provisional contribution events.
10. Export the agent, run, evidence, and proof artifacts.

The MVP explicitly does **not** require:

- Real-money orders or custody.
- The full specialist router.
- The full 343-item Graph OS.
- Full native fine-tuning support on every hardware profile.
- Paid marketplace checkout.
- A token or custom blockchain.

`PHASE-09` is the next operational release for live shadow and testnet. `PHASE-10` turns provisional contribution events into full explainable graph reputation. `PHASE-11` is required before marketing the product as a general research pipeline. `PHASE-12` remains disabled.

---

## 26. First thin vertical slice

Before implementing the full roadmap, build this single flow:

```text
Create account
→ choose moving-average starter agent
→ receive $100,000 simulated USDC
→ run 90-day BTC replay
→ compare against cash and buy-and-hold
→ run accounting, leakage, cost, and reproducibility verifiers
→ freeze manifest
→ create proof card
→ commit proof hash
→ independently verify with CLI
→ request founder review
```

### Thin-slice pass criteria

- Completion in under 20 minutes for a new user.
- Repeated run produces identical critical outputs.
- Tampering is detected.
- Costs materially affect the result.
- Public card clearly says simulated.
- Private code remains private.
- Founder can approve or request changes from an evidence packet.

---


## 27. Reuse map against the existing Fractal Society architecture

This PRD is an integration and domain-adapter program, not a parallel rebuild.

| Existing subsystem | Reuse now | Extend for trading/research | Explicitly defer |
|---|---|---|---|
| Forge | Dashboard, package metadata, model registry, training/eval abstraction, artifact export | Trading-agent templates, unified run/evidence contract, first-run workflow | Full hardware/backend parity as an MVP blocker |
| RL gyms/verifiers | Package schemas, episode runner, seeds, scorecards, sandbox, attached verifiers | Generic DomainAdapter, trace policy, flaky detection, trading verifiers, holdout hardening | Distributed RL platform |
| Hermes/OpenClaw | Connectors, harness mapping, permissions, health, eval/gym, fallback | Trading action adapter and evidence compatibility | Rewriting either runtime |
| Digital employee/skills/RAG | Skill ids/versions, repo/Forge skills, ingestion, retrieval, context bundles | Trading skills, unified permission records, research memory namespaces | Completing every dashboard/CLI skill surface before launch |
| Router | Agent data model, index, classifier, outcome/privacy records | Optional Arena discovery and later reputation-aware routing | Eligibility/ranker/API/learned router as a first-thin-slice blocker |
| FractalWork | Signed work/evidence concepts, task/review patterns, permission events where verified | Phase gates, research reviews, replication, reward evidence | Completing unrelated repository-work workflows first |
| Graph | Product model and node/edge vocabulary | Minimal proof/review/replication/reputation projection in PHASE-10 | Full analytics, DeSci views, and global graph optimization before proof of demand |
| Blockchain/wallet | Identity, signatures, permission concepts where verified | Minimal proof commitments and later bounded canary permissions | Custom chain, token, pooled capital, automatic live deployment |
| Marketplace | Agent/artifact cards, eval cards, listing metadata where verified | Arena discovery and later verified-agent distribution | Checkout and full paid rental before user proof loop works |

Every reused row remains conditional on PHASE-00R evidence. When evidence fails, the item is downgraded to `EXTEND` or `NEW`; it is never silently assumed complete.

---

## 28. Key risks and mitigations

| Risk | Consequence | Mitigation |
|---|---|---|
| Backtest overfitting | Misleading agents and loss of trust | Private holdouts, walk-forward tests, attempt history, sensitivity analysis |
| Optimistic fills | Inflated performance | Tiered simulation, L2 replay, explicit cost assumptions |
| Data gaps | Invalid experiments | Gap events, quality scores, proof-tier limits |
| Trading scope consumes the whole platform | No general research product | Adapter boundaries, reference adapter, second-domain gate |
| Blockchain complexity delays value | Slow launch | Existing chain adapter; no custom L1 in MVP |
| Token attracts mercenary users | Spam and gaming | Credits and bounties first |
| Public proof leaks strategy | Builder rejection | Disclosure tiers, commitments, reviewer encryption |
| Agent code is malicious | System compromise | Sandboxing, egress limits, quotas, signed packages |
| Reputation is gamed | Low trust | Independent weight, graph fraud detection, challenge process |
| Simulation marketed as guaranteed performance | Legal and trust risk | Prominent limitations and assumptions |
| Agent self-improves unsafely | Unauthorized behavior | Champion/challenger gates and human promotion |
| User growth creates strategy crowding later | Performance degradation | Capacity estimates and diversified routing |
| Central operator becomes trust bottleneck | Conflicts with mission | Exportable proofs, independent verifiers, progressive decentralization |

---


## 29. Open decisions for founder review

| ID | Decision | Recommended default | Required by |
|---|---|---|---|
| D-01 | MVP live scope | Simulation and public proof; no real orders | PHASE-00 |
| D-02 | Legacy evidence policy | Old checkmarks become `verification_required`, never automatic passes | PHASE-00 |
| D-03 | Initial assets | BTC and ETH perpetuals | PHASE-00 |
| D-04 | Starting simulated capital | $100,000 USDC | PHASE-00 |
| D-05 | Default leverage cap | 2x | PHASE-00 / PHASE-04 |
| D-06 | First commitment chain | Existing low-cost chain through an adapter | PHASE-07 |
| D-07 | Credits | Non-transferable compute/evaluation credits | PHASE-08 |
| D-08 | Season 0 objective | Robustness and drawdown-constrained baseline outperformance | PHASE-08 |
| D-09 | Second domain | Software-engineering issue repair | PHASE-11 |
| D-10 | Public proof default | Committed-private artifacts with public scorecard | PHASE-07 |
| D-11 | Generic router | Defer unfinished filters/ranker/API from first trading thin slice | PHASE-05 |
| D-12 | Graph scope | Minimal proof/review/reputation projection before full Graph OS | PHASE-10 |
| D-13 | Fine-tuning requirement | Optional for MVP; code/rules/prompts/imported agents are valid | PHASE-05 |
| D-14 | Custom chain/token | Defer until proven protocol demand | Post-MVP |
| D-15 | Live capital | Separate exact-scope `APPROVE_LIVE_CANARY`; never implicit | PHASE-12 |

---

## 30. Founder-review template

### Phase

`PHASE-__`

### Automated status

- Mandatory checks passed: `__/__`
- Optional checks passed: `__/__`
- Critical failures: `__`
- High-severity known issues: `__`

### What changed

- Product behavior:
- Architecture:
- Security/privacy:
- User-facing claims:
- Costs and operational burden:

### Evidence

- Demo:
- Test report:
- Metrics:
- Screenshots:
- Security report:
- Known issues:
- Risk delta:

### Questions requiring founder judgment

1. Does the experience meet the phase objective?
2. Are the assumptions and limitations communicated honestly?
3. Are any risks unacceptable for the next phase?
4. Should any feature be removed rather than repaired?
5. Is the next phase still the correct priority?

### Decision

- [ ] APPROVE
- [ ] REQUEST CHANGES
- [ ] REJECT

### Required notes

`____________________________________________________________`

### Signature and timestamp

`____________________________________________________________`

---

## 31. Final product test

The project is succeeding when this statement is true:

> A person with an idea can train an agent using simulated resources, prove a narrowly defined capability without surrendering ownership, invite independent challenge, accumulate portable reputation from useful results, and reach real-world deployment only through transparent evidence and explicit permission.

Trading should prove that the pipeline works quickly. The research protocol, graph, verifier economy, and proof system should make it valuable everywhere else.

---

## 32. External technical assumptions validated for this draft

As of 2026-06-19, this PRD assumes:

- Hyperliquid exposes public REST and WebSocket APIs for market and account information.
- Mainnet and testnet use distinct API/WebSocket endpoints.
- Official historical archives may be delayed, incomplete, and limited in dataset coverage, supporting the need for a first-party recorder.
- Hyperliquid imposes shared REST and WebSocket limits, supporting a centralized connection manager rather than one connection per agent.
- Hyperliquid API wallets, also called agent wallets, may sign for a master account or subaccount, but account queries use the actual account address.
- Builder codes may later support transparent per-fill monetization after explicit user approval.
- Testnet onboarding constraints mean the zero-friction user journey should rely on Fractal’s own simulator, not require testnet access.
- Hypothetical performance has inherent limitations and must be presented with material assumptions and prominent disclosures.

These assumptions must be revalidated against official documentation before implementation of the relevant phase.
