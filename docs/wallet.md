# FractalWork: AI-Native Agent Wallet & Tool Market
## Specification v2.0 — Hardened, Verifiable, Production-Oriented

---

## 0. What Changed From v1.0 — and Why

Before redesigning, it is worth being honest about what was wrong with the v1 spec. The diagnosis matters because the same mistakes are easy to repeat.

**Holes in v1 that this version closes:**

1. **Tool receipts were unverifiable.** v1 said the provider "signs a receipt." A signature only proves the provider asserted something — not that the work happened, not that the output is correct, not that the price was honest. v2 introduces a *verification tier* per tool class (TEE-attested, optimistic-with-challenge, or trusted-with-stake).
2. **Agent–provider collusion was not modeled.** An agent and a provider can together drain a wallet by signing fake receipts and splitting the proceeds. v2 addresses this with provider staking, output commitment requirements, random verifier sampling, and slashing.
3. **Concurrent-spend race conditions were ignored.** v1's budget checks are time-of-check / time-of-use vulnerable: two parallel tool calls can both pass the "budget available" check and double-spend. v2 uses *escrow reservations* with a strict serial budget account, not a "check then debit" pattern.
4. **No payment rail.** v1 mentions "0.02 FRAC is debited" with no statement of where it goes, when, how the provider is paid, or what happens on failure. v2 specifies a settlement state machine with conditional release and refunds.
5. **Permission objects had no replay protection or nonce scheme.** v2 capabilities are bound to a wallet nonce and chain ID.
6. **The Merkle-root and ZK references were aspirational, not connected to anything.** v2 either uses them concretely or drops them.
7. **Reputation, dispute resolution, and tool-provider identity were hand-waved.** v2 defines each as a concrete subsystem.
8. **Privacy was absent.** Agents working on private repositories or proprietary data cannot ship plaintext to "providers" — yet v1 implied exactly that. v2 specifies an envelope-encryption layer with input/output commitments.
9. **The hierarchical BIP-32 model was overengineered for the actual problem.** Key derivation does not give you permission attenuation; you still need a separate authorization layer. v2 makes *capability tokens* the primary primitive (macaroon-style, with cryptographic attenuation) and treats HD derivation as an optional key-management convenience.
10. **The FractalCore / FractalEVM split was asserted without justification.** v2 grounds it in measurable throughput requirements and explicit interfaces.
11. **Open questions were left open.** v2 answers them in §28.

**Things v1 got right and v2 keeps:** the master → workspace → project → task → session hierarchy as a *naming and budgeting* concept; the tool category taxonomy; the policy template idea; the central insight that wallets should answer "which agent, for what, for how long, at what cost" rather than only "who owns these tokens."

---

## 1. Design Goals & Non-Goals

### 1.1 Goals

- **Safe autonomy.** A compromised or misbehaving agent must have its blast radius bounded *before* it acts, not detected after.
- **Verifiable tool use.** Every paid tool call produces a receipt whose correctness can be checked or challenged by parties other than the provider.
- **Composable delegation.** An agent can spawn sub-agents (verifier, helper) with attenuated capabilities, recursively, without ever escalating privileges.
- **Sub-second authorization.** Capability checks must be fast enough that an agent calling 50 tools in a workflow is not bottlenecked on consensus.
- **Auditable.** Every action is traceable to a capability, a budget, and a receipt — for compliance, for debugging, for dispute.
- **Provider-neutral.** Anyone can offer browser, LLM, GPU, etc. services. The protocol does not pick winners.

### 1.2 Non-Goals (v2.0)

- Full post-quantum migration (capability format is PQ-ready, but secp256k1 + Ed25519 are acceptable for launch).
- Trustless LLM-output verification (this remains an open research area; v2 uses commit-and-challenge for LLM, not ZK-proof-of-inference).
- Private payments (transactions are public; only tool *payloads* are encrypted).
- Cross-chain agent wallets (single-domain at launch).

---

## 2. Threat Model

A spec without a threat model is decoration. The protocol must be safe under the following enumerated attackers.

| Attacker | Capabilities | Defense |
|---|---|---|
| **Compromised agent** | Full control of session keys; can sign any transaction the keys authorize | Capability scope, budget cap, expiry, forbidden-action set, revocation. *Damage ≤ remaining session budget.* |
| **Malicious tool provider** | Can refuse to execute, execute incorrectly, overcharge, leak inputs | Staking + slashing, output commitment with challenge window, reputation, random re-execution by verifiers |
| **Colluding agent + provider** | Sign fake receipts to drain wallets | Independent verifier sampling, provider stake exceeds maximum drain, agent-side budget caps, anomaly detection on spend patterns |
| **Malicious master-wallet operator** | Can grant capabilities; can revoke | Cannot retroactively claim work the agent did; receipts are signed by both sides; revocation does not invalidate already-settled work |
| **Front-running adversary** | Sees pending tool-purchase transactions in the mempool | Commit-reveal for sensitive intents; provider quotes signed and time-bounded |
| **Replay adversary** | Re-submits old capabilities or receipts | Nonce binding, chain-id binding, single-use receipt IDs |
| **Sybil provider** | Spins up many fake providers to win selection | Stake requirement makes Sybil expensive; reputation requires successful settled jobs |
| **Resource-exhaustion adversary** | Submits many tiny tool calls to overload the chain | Per-wallet rate limits, gas fees, anti-spam bonds |
| **Privileged insider at provider** | Reads agent input data | Envelope encryption to provider-controlled enclave key; TEE attestation for sensitive classes |
| **Malicious sub-agent** | Tries to escape its delegated scope | Capability attenuation is cryptographic and non-bypassable; sub-caveats can only narrow, never widen |

**Assumptions we make:**
- Cryptographic primitives (Ed25519, BLAKE3, BLS12-381) are secure.
- The consensus layer provides safety and liveness for transactions it accepts.
- Users can keep master-wallet keys secure (hardware wallet, MPC, etc.); the protocol does not solve master-key compromise.
- At least one honest verifier exists for any task that uses optimistic verification (a standard challenge-response assumption).

---

## 3. Architecture Overview

```
┌──────────────────────────────────────────────────────────────────┐
│                         User / DAO / Org                         │
│                       (holds master keys)                        │
└──────────────────────────────────────────────────────────────────┘
                              │
                              │ issues root capability
                              ▼
┌──────────────────────────────────────────────────────────────────┐
│                       Master Wallet Account                      │
│   • Treasury, recovery, top-level policy                         │
│   • Mints capability tokens (never used directly by agents)      │
└──────────────────────────────────────────────────────────────────┘
                              │
                              │ attenuated capabilities
                              ▼
┌──────────────────────────────────────────────────────────────────┐
│  Workspace / Project / Task scopes (naming + budget aggregation) │
└──────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌──────────────────────────────────────────────────────────────────┐
│                Agent Session — capability holder                 │
│   • Ephemeral key                                                │
│   • Bounded budget escrow                                        │
│   • Allowed-tool set, rate limits, expiry, forbidden set         │
└──────────────────────────────────────────────────────────────────┘
                              │
                              │ submits Tool Intent
                              ▼
┌──────────────────────────────────────────────────────────────────┐
│           Tool Market — intents, quotes, settlement              │
│  ┌────────────┐  ┌────────────┐  ┌────────────┐  ┌────────────┐  │
│  │  Browser   │  │    LLM     │  │    GPU     │  │   Tests    │  │
│  │ Providers  │  │ Providers  │  │ Providers  │  │ Providers  │  │
│  │  (staked)  │  │  (staked)  │  │  (staked)  │  │  (staked)  │  │
│  └────────────┘  └────────────┘  └────────────┘  └────────────┘  │
└──────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌──────────────────────────────────────────────────────────────────┐
│  Receipts → TaskReceipt → Verifier → Payout & Reputation update  │
└──────────────────────────────────────────────────────────────────┘
```

The protocol has six core modules:

1. **Account & Capability Module** — wallets, capability tokens, attenuation, revocation registry.
2. **Budget Module** — serial escrow accounts that prevent concurrent over-spend.
3. **Tool Market Module** — intent posting, quoting, matching, settlement state machine.
4. **Receipt Module** — receipts, commitments, challenge windows, dispute handling.
5. **Reputation & Staking Module** — provider stakes, scores, slashing, decay.
6. **Settlement Module** — fee market, payouts, refunds, batch finalization.

These run as native FractalCore modules. FractalEVM (§24) is a separate execution layer for general-purpose contracts that does not need to be in the hot path.

---

## 4. The Capability Token — the Central Primitive

v1 treated permissions as stored objects keyed by wallet ID. v2 makes capabilities **first-class bearer tokens** with cryptographic attenuation. This is closer to how Macaroons or Biscuit tokens work, adapted to a blockchain setting.

### 4.1 Why capability tokens, not stored ACLs

- **Offline verification.** A provider can verify an agent's capability without a state read against a possibly-stale ACL.
- **Recursive delegation.** Agents can attenuate and pass capabilities to sub-agents without protocol upgrades.
- **No central permission state.** Revocation is the only thing the chain must track centrally, and that table is small.
- **Audit by inspection.** A capability token *is* the authorization record.

### 4.2 Capability Token Format

```
CapabilityToken {
  version:            u16
  cap_id:             32 bytes              // BLAKE3(root_secret || serial)
  chain_id:           u32
  issuer:             PublicKey             // wallet that minted this
  subject:            PublicKey             // agent session key authorized to use it
  parent_cap_id:      32 bytes | null       // null if root capability
  scope:              Scope                 // see §4.3
  caveats:            Vec<Caveat>           // see §4.4
  budget_account:     BudgetAccountId       // §6
  not_before:         Timestamp
  not_after:          Timestamp
  nonce:              u64                   // monotonic per-issuer
  signature:          Signature             // Ed25519 over all fields above
}
```

The token is cryptographically chained: if `parent_cap_id` is set, the issuing wallet must hold the parent token, and the new token's caveats must be a strict subset (narrower scope, smaller budget, earlier expiry, stricter rate limits).

### 4.3 Scope

```
Scope {
  workspace_id:  WorkspaceId | ANY
  project_id:    ProjectId | ANY
  task_id:       TaskId | ANY
  tool_classes:  BitSet<ToolClass>
  providers:     Set<ProviderId> | ANY
  repositories:  Set<RepoRef>    | ANY      // for GitHub-class caps
}
```

`ANY` is permitted at the workspace level but forbidden for tool_classes when the agent is autonomous (always specify the closed set).

### 4.4 Caveats — The Restriction Vocabulary

Caveats are *only* restrictions. Adding a caveat narrows what the token allows; it can never widen. This is the core safety property and is enforced by the verifier when checking the capability chain.

```
Caveat = 
  | MaxTotalSpend(amount)
  | MaxPerCallSpend(tool_class, amount)
  | RateLimit(tool_class, count, window_seconds)
  | RequireApprovalAbove(amount)              // human-in-the-loop trigger
  | OutputCommitmentRequired(tool_class)
  | TeeAttestationRequired(tool_class, tee_type)
  | ForbiddenAction(action_id)
  | RequireVerifier(tool_class)               // independent re-execution
  | NoRecursion                               // cannot mint sub-capabilities
  | OnlyDuringHours(start, end, timezone)
  | RequireMultiSig(threshold, signers)       // for sensitive actions
  | CustomPredicate(predicate_hash)           // governance-approved only
```

The attenuation rule is mechanical: when minting a child capability, the new caveat set must imply the parent's. This is checked at verification time by re-running every caveat in the chain.

### 4.5 The Forbidden-Action Set (always-on)

These actions are forbidden on *every* non-master capability by protocol, regardless of caveats:

- `withdraw_to_external_address`
- `bridge_funds`
- `change_master_recovery`
- `mint_root_capability`
- `transfer_to_unrelated_account` (allowlist required)
- `disable_revocation_check`

A user *cannot* opt out of these on a session capability. If they want unrestricted access, they use the master wallet directly.

### 4.6 Revocation

Revocation is the one piece of state that *must* be on-chain. A small append-only structure:

```
RevocationEntry {
  cap_id:         32 bytes
  revoked_at:     Timestamp
  reason_code:    u8
  cascade:        bool                 // if true, revoke all descendants
}
```

The revocation set is stored as a sparse Merkle tree (SMT). Providers verify capabilities by checking (a) signature chain, (b) caveat implication, (c) non-revocation via SMT inclusion proof. Fresh state is needed only for (c); the proof is ~600 bytes for a tree of millions of entries.

**Cascading revocation** is critical: revoking a parent must invalidate descendants. The cascade flag is sticky — once set on an ancestor, descendants are considered revoked even if not individually listed.

### 4.7 Why not just BIP-32 HD wallets?

HD derivation gives you many keys from one seed; it does *not* give you authorization. You still need a separate layer to express "this derived key can only spend X for Y." v2 uses capability tokens as that layer.

Optionally, the issuer *may* use HD derivation for the *key material* it assigns to each session — this gives deterministic key recovery without making derivation paths part of the authorization model. The protocol does not require any specific derivation scheme.

---

## 5. Wallet Hierarchy as Naming and Budgeting

The hierarchy in v1 (master / workspace / project / task / session) is still useful — but as a *naming and budgeting* convention rather than a key-derivation requirement. Each level corresponds to a budget account and a scope filter; capabilities are minted with reference to the appropriate level.

```
MasterAccount
  ├─ owns: master keys, treasury
  ├─ can mint: any capability
  └─ contains:
     └─ WorkspaceAccount[]
        ├─ budget: workspace treasury
        ├─ default policies for the workspace
        └─ contains:
           └─ ProjectAccount[]
              ├─ budget: project allocation
              ├─ allowed tool set for the project
              └─ contains:
                 └─ TaskAccount[]
                    ├─ budget: task bounty + tool budget + verification budget
                    └─ contains:
                       └─ SessionCapability[]   // ephemeral, agent-held
```

A `SessionCapability` is the only thing an agent ever holds directly. All higher levels are administrative.

---

## 6. The Budget Module — Solving the Concurrent-Spend Bug

This is one of the biggest fixes from v1.

### 6.1 The bug v1 had

```
v1 pseudocode:
  if wallet.budget_remaining >= amount:
      execute()
      wallet.budget_remaining -= amount
```

Two parallel tool calls both read the same `budget_remaining`, both pass the check, both execute. Classic TOCTOU. With autonomous agents firing many concurrent tool calls, this is not theoretical.

### 6.2 The fix: Budget Accounts with Reservations

A budget is a separate first-class account, not a field on a wallet.

```
BudgetAccount {
  id:                  BudgetAccountId
  parent:              BudgetAccountId | null
  asset:               AssetId          // typically FRAC
  total_deposited:     Amount
  reserved:            Amount           // funds in active escrows
  spent:               Amount           // funds settled out
  available:           Amount           // = total_deposited - reserved - spent
  per_tool_caps:       Map<ToolClass, Amount>
  rate_buckets:        Map<ToolClass, TokenBucket>
  nonce:               u64
}
```

Every state change to a budget account is a serialized transaction. There is no "check then act"; reservation and debit happen atomically.

### 6.3 Reservation lifecycle

```
1. RESERVE   — funds move from available → reserved when an intent is matched.
                Fails atomically if available < amount or rate limit exhausted.
2. SETTLE    — funds move from reserved → spent on successful receipt.
3. REFUND    — funds move from reserved → available on failure/expiry/dispute win.
4. PARTIAL   — settle X, refund (reserved - X). Used for metered tools.
```

This pattern is identical to how exchanges handle order matching and the same correctness properties apply. The token-bucket rate limits are checked and decremented in the same atomic step as RESERVE.

### 6.4 Refunds

Refunds happen automatically when:
- Provider fails to deliver within the SLA window.
- Output commitment cannot be verified.
- A dispute is resolved against the provider.
- The session capability expires before settlement.

Refunds return to the *budget account*, not to the master wallet directly. The master can sweep at any time.

---

## 7. The Tool Market — Intent-Based, Not Direct-Call

v1 had agents directly calling `BuyToolAccess`. v2 uses an intent → solver → settlement flow. This is cleaner, gives the market a place to do price discovery, and matches the direction the rest of the industry is moving (CowSwap, UniswapX, ERC-7683).

### 7.1 The Tool Intent

```
ToolIntent {
  intent_id:           IntentId
  agent_session:       PublicKey            // capability subject
  capability_proof:    CapabilityChain      // see §4
  task_id:             TaskId
  tool_class:          ToolClass
  payload_commitment:  Hash                 // BLAKE3 of the (encrypted) request
  payload_pointer:     URI                  // where providers fetch the encrypted blob
  output_constraints:  OutputSpec           // required format, size, etc.
  max_price:           Amount
  preferred_providers: Set<ProviderId> | ANY
  verification_tier:   VerificationTier     // §10
  deadline:            Timestamp
  nonce:               u64
  signature:           Signature
}
```

The agent posts the intent. Providers see it, fetch the encrypted payload (only if they hold the required decryption capability per §11), and respond with quotes.

### 7.2 Quotes

```
Quote {
  quote_id:           QuoteId
  intent_id:          IntentId
  provider_id:        ProviderId
  price:              Amount
  estimated_latency:  Duration
  attestation:        Option<TeeAttestation>
  expiry:             Timestamp
  provider_stake_id:  StakeRef
  signature:          Signature
}
```

Quotes are time-bounded and signed. The agent picks one (auto-selection rules in §7.4) and submits a `MatchIntent` transaction, which atomically:

1. Reserves the budget for the quote price.
2. Locks a portion of the provider's stake equal to `slashing_multiplier × price`.
3. Transitions the intent to `MATCHED` state.

### 7.3 Settlement state machine

```
        ┌──────────┐
        │ PROPOSED │   ← intent posted
        └────┬─────┘
             │ matched quote
             ▼
        ┌──────────┐
        │ MATCHED  │   ← budget reserved, provider stake locked
        └────┬─────┘
             │ provider posts output commitment
             ▼
        ┌──────────┐
        │DELIVERED │   ← output_hash + receipt on-chain (or via DA)
        └────┬─────┘
             │ challenge window
       ┌─────┴────────────┐
       ▼                  ▼
  ┌──────────┐      ┌──────────┐
  │ SETTLED  │      │DISPUTED  │
  └──────────┘      └────┬─────┘
                         │ adjudication
                  ┌──────┴───────┐
                  ▼              ▼
            ┌──────────┐   ┌──────────┐
            │PROVIDER  │   │PROVIDER  │
            │   PAID   │   │ SLASHED  │
            └──────────┘   └──────────┘
```

States are exhaustive. Each transition is a signed transaction with explicit preconditions.

### 7.4 Auto-selection

By default the agent's wallet client picks the lowest-price quote from providers whose `(reputation × stake)` exceeds a threshold and whose `estimated_latency` is below the agent's tolerance. The agent's owner can configure preferences (cheapest, fastest, most-reputable) in the policy template.

### 7.5 Anti-front-running

Tool intents reveal the agent's payload commitment, not the payload itself. A front-runner cannot copy the actual work. For sensitive intents where even the *fact* of the request is sensitive, a two-step commit-reveal can be used: the agent first posts a hash commitment to the intent, then reveals after matching.

---

## 8. Tool Classes and Verification Tiers

Not every tool needs the same verification. A browser search returning HTML is fundamentally different from an LLM call returning an opinion. v2 classifies tools by what is verifiable about them.

### 8.1 Tool classes (v2.0)

| Class | Examples | Pricing | Verification |
|---|---|---|---|
| `BROWSER` | Web fetch, headless browser ops | Per request | Optimistic (re-fetch in challenge window) |
| `WEB_SCRAPE` | Structured extraction | Per page | Optimistic with output schema check |
| `GITHUB_READ` | Read repos, issues | Per call | TEE-attested or trusted-with-stake |
| `GITHUB_WRITE` | Commit, PR, branch ops | Per call | TEE-attested (writes are observable on GitHub) |
| `LLM_INFERENCE` | Text generation | Per token (committed) | Commit + log-prob disclosure + sample challenge |
| `EMBEDDING` | Vector embedding | Per token | Optimistic (deterministic model → reproducible) |
| `GPU_JOB` | Inference, fine-tune, batch | Per second / per job | TEE-attested or proof-of-execution (where possible) |
| `DATABASE_QUERY` | SQL execution | Per query | Provider-attested; result hash on chain |
| `EMAIL_SEND` | SMTP / API send | Per message | Provider-attested + DKIM evidence |
| `TEST_RUNNER` | CI execution | Per run | Reproducible build hash + TEE attestation preferred |
| `FILE_STORAGE` | Object storage | Per MB-day | Periodic proof-of-retrievability challenges |
| `VECTOR_SEARCH` | ANN over an index | Per query | Optimistic with re-query challenge |
| `OCR` | Image-to-text | Per page | Optimistic |
| `CODE_EXECUTION` | Sandboxed code runs | Per second | TEE-attested |

### 8.2 Verification tiers

```
VerificationTier =
  | Trusted          // accept provider signature; rely on stake + reputation
  | Optimistic       // accept by default, challengeable for N blocks
  | Attested         // requires TEE attestation alongside the receipt
  | Replicated       // executed by ≥k providers; results compared
  | Proven           // cryptographic proof of execution (e.g. for ZK-able workloads)
```

The agent's capability *can require* a higher tier than the default for a tool class. A skeptical user can set `TeeAttestationRequired(GITHUB_WRITE, IntelTDX)` as a caveat and any non-TEE provider's quote is automatically rejected.

### 8.3 LLM verification — the honest answer

Trustless LLM-output verification is unsolved. v2 does *not* pretend otherwise. For `LLM_INFERENCE` we use:

- **Token counts committed up-front.** Provider quotes "X input tokens, Y output tokens" at a known per-token price. Mismatch on input is detectable (the input is hashed and shipped). Output overcount can be challenged by demanding the full output and recounting.
- **Per-token log-probs disclosed.** The provider commits to a hash of the log-prob sequence; a sampling challenger can request a small slice and re-run on the same (declared) model to spot-check consistency. This is a probabilistic deterrent, not a proof.
- **Model identity attested via TEE** when the user requires it (`Attested` tier).

This is honest about the limits of the state of the art. v1's "metered by tokens" hand-wave glossed over this.

---

## 9. Receipts and the Audit Trail

### 9.1 ToolReceipt

```
ToolReceipt {
  receipt_id:          ReceiptId            // = BLAKE3(intent_id || provider_sig)
  intent_id:           IntentId
  task_id:             TaskId
  agent_session:       PublicKey
  provider_id:         ProviderId
  tool_class:          ToolClass
  payload_commitment:  Hash
  output_commitment:   Hash                 // BLAKE3 of the (encrypted) output
  output_pointer:      URI                  // DA layer or off-chain blob
  metering:            MeteringRecord       // tokens, seconds, bytes — class-specific
  cost:                Amount
  started_at:          Timestamp
  completed_at:        Timestamp
  attestation:         Option<TeeAttestation>
  provider_sig:        Signature
  agent_ack_sig:       Option<Signature>    // present when agent acknowledged
}
```

Receipts are posted as transactions. Their core fields go on-chain; large payloads go to a DA layer (Celestia-style, or a FractalWork-native DA module) and are referenced by hash.

### 9.2 TaskReceipt — what a finished task looks like

```
TaskReceipt {
  task_id:                  TaskId
  agent_session:            PublicKey
  artifact_commitment:      Hash             // hash of the deliverable
  artifact_pointer:         URI
  tool_receipt_root:        MerkleRoot       // commits to all ToolReceipts used
  total_tool_cost:          Amount
  total_compute_time:       Duration
  verifier_score:           Option<Score>    // see §13
  verifier_sig:             Option<Signature>
  status:                   TaskStatus       // SUBMITTED | VERIFIED | REJECTED | DISPUTED
  payout:                   PayoutRecord
}
```

The `tool_receipt_root` is the critical link: a verifier can demand inclusion proofs for any receipt and check that (a) every receipt's `task_id` matches, (b) the receipts' total cost equals `total_tool_cost`, and (c) all receipts were issued under capabilities that descend from the task's session capability. v1 had no such binding — receipts could be attached from anywhere.

### 9.3 Dispute window

Every receipt is in `DELIVERED` state for a configurable challenge window (default: 256 blocks ≈ 5 minutes; per-class overrides). Any party with a stake-backed challenger role can submit:

```
Challenge {
  receipt_id:        ReceiptId
  challenger:        PublicKey
  challenge_kind:    NotExecuted | WrongOutput | Overcharged | Unattested
  evidence:          EvidencePacket
  bond:              Amount                  // forfeited if challenge loses
}
```

Adjudication is class-specific:
- For `BROWSER` / `WEB_SCRAPE` / `EMBEDDING`: deterministic re-execution by an adjudicator quorum.
- For `GITHUB_WRITE`: check GitHub for the asserted state.
- For `TEST_RUNNER`: re-run in a reference environment; compare outputs.
- For `LLM_INFERENCE`: log-prob sample challenge; if mismatched beyond tolerance, slash.
- For `GPU_JOB` / `CODE_EXECUTION`: if attested, TEE quote must verify; otherwise replicated re-execution.

The losing side forfeits its bond; the winning side gets a portion as anti-spam compensation.

---

## 10. Provider Staking, Slashing, and Reputation

### 10.1 Why providers must stake

Without stake, a malicious provider's worst case is "lose future business." With stake, the worst case is "lose the bond." Stake creates an immediate, on-chain economic deterrent that does not depend on slow reputational signals.

### 10.2 Stake requirements

```
ProviderStake {
  provider_id:     ProviderId
  tool_classes:    Set<ToolClass>           // which classes this stake covers
  amount:          Amount
  locked:          Amount                   // currently allocated to live jobs
  available:       Amount
  withdrawal_delay: Duration                // 7 days minimum
}
```

The stake-to-job ratio is per class. For high-trust classes (`GPU_JOB`, `GITHUB_WRITE`, `EMAIL_SEND`), the locked stake must be ≥ 10× job value. For low-trust (`BROWSER`, `EMBEDDING`), ≥ 2× is acceptable. Governance sets these parameters per class.

A withdrawal delay means a provider cannot drain stake immediately before a challenge.

### 10.3 Slashing conditions

Slashing is mechanical, not discretionary. Each is triggered by a verified on-chain event:

| Condition | Slash fraction | Detection |
|---|---|---|
| Failed to deliver within SLA | 10% of locked stake | Timeout (no DELIVERED transition) |
| Output commitment mismatch | 50% of locked stake | Challenger provides counterexample |
| Attestation invalid | 100% of locked stake | Quote of attestation fails verification |
| Overcharged (metering false) | 20% + refund difference | Adjudicator re-measures |
| Equivocation (two contradictory receipts) | 100% of locked stake | Two signed receipts both surface |
| Stake withdrawal during locked dispute | 100% + ban | Detected at withdrawal time |

Slashed funds are distributed: 50% to the affected agent's budget account (compensation), 30% to the challenger (bounty), 20% burned (anti-collusion: removes the funds from the system so insiders cannot game it).

### 10.4 Reputation

Reputation is *derived* state, not separately written. It is computed from on-chain history:

```
score(provider, class) = f(
  successful_settlements,        // weighted by recency
  failed_settlements,            // exponential penalty
  slashing_events,               // very heavy penalty
  age_of_provider,               // anti-Sybil
  stake_amount,                  // skin-in-the-game weight
  diversity_of_clients           // anti-collusion
)
```

The exact function is parameterized and governance-tunable. The point is that reputation cannot be bought directly — only earned by *settled* work, which costs stake to perform fraudulently.

To bootstrap: new providers start with `score = 0` and require larger stake multipliers until they have history. Agents may filter on minimum reputation in their quote-selection policy.

### 10.5 Provider identity & registration

```
ProviderRegistration {
  provider_id:         ProviderId           // BLAKE3 of pubkey
  pubkey:              PublicKey
  endpoint:            URI                  // service URL
  tool_classes:        Set<ToolClass>
  pricing_url:         URI                  // dynamic price feed
  tee_quote:           Option<TeeAttestation>   // for TEE providers
  contact:             ContactInfo
  registration_bond:   Amount               // separate from per-class stake
  signature:           Signature
}
```

The registration bond is forfeited on de-registration during a dispute. Provider identity is just a keypair — no KYC requirement at the protocol layer (KYC, if needed, is a layer-2 concern for regulated jurisdictions).

---

## 11. Privacy — The Missing Layer

v1 had agents shipping plaintext payloads to "providers." For most real use cases (private repos, customer data, proprietary documents) this is unacceptable. v2 adds an explicit envelope-encryption layer.

### 11.1 Provider keys

Each provider publishes a long-term X25519 public encryption key as part of registration. For TEE providers, the key is bound to the enclave attestation — meaning the corresponding private key only exists inside the attested enclave and is never visible to the provider's operator.

### 11.2 Envelope flow

```
1. Agent generates a fresh symmetric key K.
2. Agent encrypts payload P with K → ciphertext C.
3. Agent encrypts K to provider's public key → wrapped_key W.
4. Agent stores (C, W) on the DA layer.
5. Agent posts intent with commitment = BLAKE3(C).
6. Provider fetches (C, W), decrypts W → K, decrypts C → P, executes.
7. Provider encrypts output O to a fresh symmetric key, wraps to the agent's pubkey.
8. Provider posts output commitment = BLAKE3(encrypted_output).
```

For replicated verification (`Replicated` tier), wrapped keys are produced for each provider in the replication set; for adjudication, a sealed reveal mechanism (threshold decryption by adjudicator quorum) allows challenge inspection without persistent disclosure.

### 11.3 What is *not* private

- The fact that a tool call happened.
- The tool class and approximate price.
- The provider and the agent.
- The size of the payload (within bucket sizes, to limit length-leak inference).

v2 does not aim to hide *that* an agent is working — only to protect payload contents. Full privacy (mixers, ZK-shielded transactions) is out of scope.

---

## 12. Sub-Agent Delegation

Real agent workflows involve sub-agents: a coding agent spawns a verifier, a research agent spawns a summarizer, etc. v2 makes this a first-class operation.

### 12.1 Delegation as capability attenuation

A session capability with no `NoRecursion` caveat may mint child capabilities, *strictly attenuated*:

- Child `not_after` ≤ parent `not_after`.
- Child `MaxTotalSpend` ≤ remaining parent budget reserved for the child.
- Child `tool_classes` ⊆ parent `tool_classes`.
- Child caveats are a superset (more restrictive) of parent's.
- Child `parent_cap_id` = parent `cap_id` (chain link).

The parent reserves a portion of its budget into a new child budget account; from the parent's perspective the reservation is already "spent" against its caveats.

### 12.2 Verifier sub-agent example

```
Coding agent (cap A: write code, max 10 FRAC, 1 hour, includes verifier requirement)
  │
  └─ mints cap B: run tests, max 2 FRAC, 30 min, no GITHUB_WRITE
       │
       └─ spawns verifier session with cap B
              │
              └─ uses TEST_RUNNER, returns signed score
```

The coding agent cannot fake a verifier score because verifier capabilities are minted from a *different* parent (a project-level "verifier" capability with `verify_own_work` forbidden). This implements separation of duties at the protocol layer.

### 12.3 Revocation cascades

Revoking a parent capability with `cascade=true` invalidates all descendants in O(1) at verification time (descendants check the parent in the revocation SMT). Without cascade, descendants survive — useful when you want to terminate the parent agent but let a verifier complete its work.

---

## 13. Verifier Roles and Independent Verification

### 13.1 Verifiers are not optional decoration

v1 had verifiers as a vague "verifier_score" field. v2 makes verification a structured role with its own capability class and economic incentives.

### 13.2 Verifier capability

Issued from the project or task account, with caveats specifically forbidding:
- Modifying the artifact being verified.
- Paying out to itself.
- Verifying work it produced (enforced via `agent_session ≠ verifier_session` check).

### 13.3 Random verifier sampling

For tasks above a configurable bounty threshold (default: 10 FRAC), a verifier is selected pseudo-randomly from a pool, weighted by reputation and stake. The randomness source is the chain's VRF beacon to prevent provider/agent collusion in verifier choice.

### 13.4 Verifier slashing

Verifiers stake too. Their slashing conditions:
- Verifying own work (caught by signature check): 100% slash.
- Conflicting verifications on the same task: 100% slash.
- Verification overturned in dispute: 50% slash.
- Late verification (missed SLA): 10% slash + verifier-pool rotation.

This prevents the "lazy verifier" attack where a verifier always rubber-stamps.

---

## 14. Transaction Types — Concrete and Complete

This replaces v1 §9 with a full, consistent set.

### 14.1 Account & capability transactions

```
MintCapability {
  parent_cap_id:    CapabilityId | null
  child_token:      CapabilityToken      // signed by parent issuer
  budget_seed:      Option<{from_budget, amount}>   // reserve funds for the child
}

RevokeCapability {
  cap_id:           CapabilityId
  reason_code:      u8
  cascade:          bool
  issuer_sig:       Signature
}

EmergencyStop {
  scope:            Scope                // workspace | project | task | all
  master_sig:       Signature
}                                         // revokes all matching capabilities in one tx
```

### 14.2 Budget transactions

```
CreateBudgetAccount { parent, asset, initial_caps }
FundBudgetAccount   { budget, amount, source_budget }
SweepBudgetAccount  { budget, dest, amount }            // master-only
CloseBudgetAccount  { budget }                          // releases remaining to parent
```

### 14.3 Tool-market transactions

```
PostIntent          { intent: ToolIntent }
PostQuote           { quote: Quote }
MatchIntent         { intent_id, quote_id }
PostReceipt         { receipt: ToolReceipt }
AckReceipt          { receipt_id, agent_sig }
ChallengeReceipt    { challenge: Challenge }
ResolveChallenge    { challenge_id, decision, evidence }
SettleReceipt       { receipt_id }                       // after challenge window
RefundReservation   { intent_id, reason_code }
```

### 14.4 Provider transactions

```
RegisterProvider    { registration: ProviderRegistration }
StakeForClass       { provider_id, tool_class, amount }
UnstakeRequest      { provider_id, tool_class, amount }    // starts withdrawal delay
UnstakeFinalize     { request_id }                          // after delay
SlashProvider       { provider_id, slash: SlashRecord }     // protocol-issued
UpdateProvider      { provider_id, fields }
DeregisterProvider  { provider_id }                         // forfeits registration bond if active disputes
```

### 14.5 Task transactions

```
PostTask            { task: TaskRecord, bounty_budget, tool_budget, verifier_budget }
CheckoutTask        { task_id, agent_session, expiry }
RenewCheckout       { task_id, evidence_of_progress, new_expiry }
SubmitTask          { task_id, artifact_pointer, tool_receipt_root }
VerifyTask          { task_id, verifier_sig, score }
DisputeTask         { task_id, challenger, evidence }
FinalizeTask        { task_id }                            // triggers payout
```

Each transaction's preconditions are explicit and atomic. There is no "check then write" pattern anywhere; all multi-field updates are single state transitions.

---

## 15. Policy Templates — Concrete, Composable, Versioned

v1 had policy templates as ad-hoc text. v2 makes them composable, versioned, and on-chain.

### 15.1 Policy template structure

```
PolicyTemplate {
  template_id:        TemplateId
  version:            SemVer
  name:               String
  description:        String
  inherits:           Option<TemplateId>      // composition
  base_caveats:       Vec<Caveat>
  required_attestations: Set<(ToolClass, TeeType)>
  default_budget:     BudgetSpec
  rate_limits:        Map<ToolClass, RateLimit>
  publisher:          PublicKey
  audit_record:       Option<URI>             // link to a published audit
}
```

Templates are registered on-chain. Wallets can apply a template by reference when minting a session capability — the protocol resolves the template and applies its caveats.

### 15.2 Built-in templates

These ship with the protocol; users may use them as-is or as a base for inheritance.

**`tpl:research-agent-v1`** — for browsing and synthesis.
```
allowed:    BROWSER, WEB_SCRAPE, EMBEDDING, LLM_INFERENCE, FILE_STORAGE (read)
forbidden:  GITHUB_WRITE, EMAIL_SEND, withdraw, bridge
budget:     3 FRAC total; BROWSER ≤ 1, LLM_INFERENCE ≤ 2
rate:       50 browser/hr, 100 LLM/hr
```

**`tpl:coding-agent-v1`** — for software work.
```
allowed:    GITHUB_READ, GITHUB_WRITE (branches only), TEST_RUNNER,
            LLM_INFERENCE, CODE_EXECUTION (sandboxed), FILE_STORAGE
forbidden:  GITHUB_WRITE on main/master, EMAIL_SEND, withdraw
budget:     10 FRAC total; TEST_RUNNER ≤ 2, LLM_INFERENCE ≤ 5
rate:       200 LLM/hr, 20 tests/hr
require:    TEE attestation for GITHUB_WRITE
```

**`tpl:email-agent-v1`** — for messaging with human-in-the-loop.
```
allowed:    LLM_INFERENCE (drafting), DATABASE_QUERY (CRM read)
            EMAIL_SEND (require human approval per send)
forbidden:  EMAIL_SEND without RequireApprovalAbove(0)
budget:     1 FRAC total
rate:       3 emails/hr
```

**`tpl:verifier-agent-v1`** — for independent verification.
```
allowed:    FILE_STORAGE (read), TEST_RUNNER, CODE_EXECUTION (sandboxed),
            LLM_INFERENCE
forbidden:  GITHUB_WRITE, EMAIL_SEND, paying_self, verifying_own_work
budget:     2 FRAC total
require:    independent of work_session
```

### 15.3 Composition

`inherits` allows extending: a `tpl:coding-agent-rust-v1` can inherit from `tpl:coding-agent-v1` and add Rust-specific tool budgets and a `RequireAttestation` for the Rust test runner. The protocol verifies inheritance forms a DAG (no cycles) and that child caveats are strictly additive.

---

## 16. Fee Market and Settlement

### 16.1 Who pays gas

By default, the agent's session capability is bound to a `gas_budget` carved out of the task-level tool budget. This means agents pay their own gas from the budget the user allocated.

Alternative: the master wallet can opt to sponsor gas for all descendants by including a `gas_sponsor` field. The protocol routes gas debits to the sponsor's account.

### 16.2 Provider settlement timing

Two settlement modes per class:

- **Instant** (challenge window 0 blocks): for high-volume, low-stakes classes where reputation + stake is sufficient. Provider receives funds immediately. Slashing still applies retroactively.
- **Windowed** (default): provider receives funds after the class's challenge window. Reserved budget is held in escrow until then.

### 16.3 Batch finalization

To keep on-chain footprint low, multiple receipt settlements for the same provider and same class can be batched into a single `BatchSettle` transaction. The batch root commits to all included receipts.

### 16.4 Pricing units

The protocol-native token is `FRAC`. Tool prices are quoted in FRAC. Providers may publish reference prices in fiat-pegged units; conversion happens off-chain at quote time. Quotes are always denominated in FRAC for on-chain matching.

---

## 17. Native Module Architecture

This pins down the FractalCore / FractalEVM split that v1 asserted without justification.

### 17.1 Why a native module, not an EVM contract

The protocol must handle bursts on the order of **10,000 capability-check operations per second** as agent populations grow. EVM execution at typical throughput (~1,000 TPS at best on optimized L2s, often much less) is insufficient. More importantly, capability checks are *read-heavy* and *latency-sensitive* — an agent calling 50 tools in a workflow does not want each call to wait for a full block confirmation.

FractalCore provides:

- **Native state for capabilities, budgets, intents, receipts.**
- **Single-block atomic operations** for reserve / settle / refund.
- **Direct-call interface** from validator-collocated agent servers (within the security boundary of the chain).
- **Light-client proofs** for off-chain providers to verify capabilities without running a full node.

### 17.2 FractalCore modules (canonical names)

```
core::account        — wallets and master keys
core::capability     — capability tokens, verification, revocation SMT
core::budget         — budget accounts, reservations, rate buckets
core::market         — intents, quotes, matching, settlement state machine
core::receipt        — receipts, DA references, challenge windows
core::stake          — provider stakes, slashing, withdrawal delays
core::reputation     — derived reputation queries
core::task           — task records, checkout, renewal, finalization
core::policy         — policy template registry
core::governance     — parameter updates, module upgrades
```

### 17.3 What FractalEVM does

Things that need general programmability but not hot-path performance:

- Governance contracts (parameter votes, treasury proposals).
- Reputation index extensions (third-party scoring algorithms operating on receipt history).
- Marketplace extensions (specialized tool categories, custom matching logic).
- DeFi integrations (LP for FRAC, borrowing for provider stakes).
- Cross-chain bridges.
- Optional dispute escalation contracts for high-value cases.

### 17.4 Bridge between layers

FractalEVM contracts can read FractalCore state via a precompile interface. FractalCore reads from EVM contracts only via explicit oracle modules (for governance results, for example). This one-way default keeps the hot path fast and predictable.

---

## 18. Throughput, Latency, and Cost Targets

A spec without numbers is wishful thinking. Targets for v2.0 launch:

| Metric | Target | Notes |
|---|---|---|
| Capability verification (off-chain, with proof) | < 5 ms | Ed25519 sig + caveat eval + SMT proof check |
| Budget reservation transaction | < 200 ms confirm | Single-shard write |
| Intent → Match latency | < 500 ms p50 | Includes provider quote round-trip |
| Receipt posting → DELIVERED | < 1 s p50 | DA layer write + on-chain commitment |
| Receipt settlement (after window) | < 100 ms tx | Cheap path, batchable |
| Sustained tool-call throughput | 10,000 / s chain-wide | Across all tool classes |
| Cost per tool call (gas) | < 0.0001 FRAC | Excluding tool price itself |
| Capability size on wire | < 2 KB typical | Chain of 3-4 caps + caveats |
| Revocation SMT proof | < 1 KB | For trees up to 10^8 entries |

These are engineering goals, not promises; the spec exists so that implementations have unambiguous targets.

---

## 19. Concrete End-to-End Examples

### 19.1 Agent buys browser access — fully specified

```
T+0    User has task FW-102 (research feature parity of three libraries).
       User mints a session capability for coding-agent:
         template: tpl:research-agent-v1
         budget: 5 FRAC into BudgetAccount#7821
         expires: T+1h
         scope: workspace=acme, project=docs, task=FW-102

T+5s   Agent constructs ToolIntent {
         class: BROWSER
         payload: "fetch https://docs.lib-a.example"  (encrypted to provider key)
         max_price: 0.05 FRAC
         deadline: T+30s
         tier: Optimistic
       }
       Agent signs with session key and posts. Cost: 0.0001 FRAC gas.

T+6s   Three providers post quotes:
         P1: 0.03 FRAC, latency 800ms, rep 0.92, stake 100 FRAC
         P2: 0.02 FRAC, latency 1.2s,  rep 0.88, stake 60 FRAC
         P3: 0.025 FRAC, latency 600ms, rep 0.95, stake 200 FRAC

T+7s   Agent's wallet client auto-selects P3 (rep × stake highest of price-acceptable).
       Submits MatchIntent → atomically:
         - Reserves 0.025 FRAC from BudgetAccount#7821
         - Locks 0.05 FRAC of P3's stake (2× job value, low-trust class)
         - Decrements BROWSER rate bucket for the session

T+7.6s P3 fetches encrypted blob, decrypts, executes browser fetch.

T+8s   P3 posts ToolReceipt:
         output_commitment: BLAKE3(encrypted_response)
         output_pointer: ipfs://...
         metering: { bytes_fetched: 12384 }
         cost: 0.025 FRAC
         provider_sig: ...
       Receipt state → DELIVERED.

T+8s..T+5m  Challenge window for BROWSER class.
              No challenge filed.

T+5m   Auto-SETTLE transaction (can be triggered by anyone, usually a relayer):
         - 0.025 FRAC moves from BudgetAccount#7821.reserved → P3's payout
         - 0.05 FRAC of P3 stake unlocks
         - Receipt finalized
         - tool_receipt_root in TaskReceipt updates
```

The whole flow, including challenge window, costs the agent 0.0251 FRAC (price + gas). All steps are atomic and replayable from chain state.

### 19.2 Sub-agent delegation — coding agent spawns verifier

```
1. Coding agent holds cap A:
     allows: GITHUB_WRITE (branch only), TEST_RUNNER, LLM_INFERENCE
     budget: 10 FRAC

2. Project policy says: tasks over 5 FRAC bounty require independent verification.

3. After writing code, coding agent does NOT mint a verifier capability itself
   (forbidden by separation-of-duties caveat on cap A).

4. Coding agent calls SubmitTask. The task is now in SUBMITTED state.

5. Project's verifier pool selects an independent verifier session via VRF.
   That verifier holds cap V minted by the project account directly:
     allows: TEST_RUNNER, CODE_EXECUTION, FILE_STORAGE (read)
     forbidden: agent_session = code agent's session (caught at use time)
     budget: 2 FRAC

6. Verifier re-runs tests, examines artifact, posts VerifyTask with score.

7. Task moves to VERIFIED. Payout disbursed.
```

Coding agent cannot fake the verifier because their capability chain does not include verifier rights.

### 19.3 Slashing — provider serves wrong output

```
1. Agent A intents an LLM_INFERENCE call. Provider P matches at 0.10 FRAC.
   P's stake locked: 1.0 FRAC (10× for LLM, Attested tier).

2. P returns output, posts receipt with TEE attestation X.

3. Challenger C inspects X — the attestation references model
   "llama-3-70b-instruct-v0" but the quote committed to "llama-3-405b".

4. C posts ChallengeReceipt with evidence: attestation quote + intent
   commitment showing mismatch. Bond: 0.5 FRAC.

5. Adjudicator quorum verifies: attestation is valid, model identity mismatches.

6. ResolveChallenge: P loses.
     - 1.0 FRAC stake slashed:
         0.5 FRAC → BudgetAccount#... (agent A compensation)
         0.3 FRAC → C (bounty)
         0.2 FRAC → burn
     - Receipt status: SLASHED, not paid
     - Reserved 0.10 FRAC returned to A's budget
     - C's bond returned
     - P's reputation drops sharply

7. P may continue operating but with much higher stake multipliers required
   for future LLM jobs.
```

---

## 20. Comparison to Existing Systems

A spec that claims to be the best AI-native blockchain should be measurable against what already exists. Here is an honest comparison.

| System | Native agent permissions? | Capability attenuation? | Tool market primitive? | Verifiable execution? | What v2 takes from it |
|---|---|---|---|---|---|
| Ethereum + ERC-4337 session keys | Smart-contract level only | Limited | No | No | Session-key UX patterns |
| Solana + Squads | Multisig with policies | Limited | No | No | Atomic multi-step transactions |
| EigenLayer AVS | Restaking; no per-call caps | No | Sort-of (services) | Yes (operator slashing) | Slashing patterns, operator stake model |
| Lit Protocol PKPs | TSS keys with conditions | Some | No | TEE-attested partial | Conditional signing patterns |
| Coinbase x402 / "agent payments" | HTTP 402 payment flow | No | Payment rail only | No | Per-call payment UX |
| Boundless / RISC Zero verifiable compute | No wallet model | No | Compute only | Yes (ZK proof) | Proof-of-execution patterns |
| Aleo / Aztec (private compute) | No agent specifics | No | No | Yes (ZK) | Privacy primitive patterns |
| FractalWork v1 (this doc's ancestor) | Wallet hierarchy only | Implicit | Yes but unverified | No | Hierarchy, tool taxonomy |
| **FractalWork v2** | **Native, first-class** | **Cryptographic, recursive** | **Intent-based, settled** | **Tiered: optimistic / TEE / replicated** | — |

The honest claim is: v2 is the *integration*. None of the individual ideas are new. The combination — capability tokens + budget escrow + intent market + verification tiers + stake-backed providers — is what produces a system that can actually run autonomous agents at scale without the operator going bankrupt overnight.

---

## 21. Migration & Compatibility

Even if v2 is a clean specification, it must coexist with existing infrastructure.

### 21.1 Wallet compatibility

- Master wallets use standard Ed25519 or secp256k1 keys; existing hardware wallets work.
- A master wallet *may* be an existing EOA (Ethereum-style address) bridged in via a deposit contract.
- Session keys are protocol-native; they do not need hardware-wallet support (they are ephemeral).

### 21.2 Tool provider compatibility

A provider running a generic HTTP service can join the network by:
1. Generating a keypair, registering, posting a stake bond.
2. Subscribing to intents matching its tool class.
3. Responding with quotes; executing on match; posting receipts.

A reference provider SDK ships with the protocol — **Rust:** `fractal_sdk::provider` (`crates/sdk-rust/src/provider.rs`; re-exports `fractal_wallet` market types + indexer helpers); **TypeScript:** `packages/fractal-provider-ts/` (wire types, `npm run check`). Existing services (OpenAI, Anthropic, GitHub) can be adapted by a thin "proxy provider" that pays them off-chain and exposes their capabilities on-chain. The proxy carries the staking risk.

### 21.3 EVM compatibility

FractalEVM is opcode-compatible with EVM Shanghai (or whatever target is current at implementation time). Standard Solidity contracts deploy unmodified. Cross-chain bridges to Ethereum, Solana, etc. live in FractalEVM, not FractalCore.

---

## 22. Governance and Parameter Tuning

Every numeric parameter in this spec (slash fractions, challenge windows, rate-limit defaults, stake multipliers) is governance-tunable, not hard-coded. Governance is a separate concern and v2 does not legislate the governance design — but it does require:

- Each tunable parameter is namespaced (`core.market.challenge_window.browser_blocks`).
- Parameter changes go through a timelock (minimum 7 days).
- Emergency parameters (kill-switch flags) have a separate shorter-timelock multisig path.
- All current parameters are queryable as on-chain state.

---

## 23. Observability and Audit

This is something v1 had no story for and that operators absolutely require.

### 23.1 The on-chain audit trail per task

```
For any task T, the chain can reconstruct:
  capability chain: master → workspace → project → task → session(s)
  budget reservations and settlements
  every tool intent posted under T's capabilities
  every receipt linked back via tool_receipt_root
  every challenge filed and resolved
  total spend vs. allocated budget
  final artifact pointer and verifier score
```

This is what "auditable" actually means: a third party can reconstruct what an agent did and what it cost, with cryptographic certainty, without trusting any single party.

### 23.2 Off-chain indexing

Reference indexers ship with the protocol, providing:
- Per-wallet activity feeds.
- Per-provider performance dashboards.
- Per-task receipt browsers.
- Anomaly alerts (spend spikes, failed-receipt clusters).

These are read-only views over chain state; the chain is the source of truth.

---

## 24. Security Considerations — Updated

### 24.1 Already-mitigated threats (see §2 threat model)
Compromised agent, malicious provider, collusion, front-running, replay, Sybil, resource exhaustion, insider read at provider, sub-agent escape — see threat model and the relevant sections.

### 24.2 Residual risks

These are honest limitations to call out:

- **Master-wallet compromise** is catastrophic. Use hardware wallets, MPC, or social recovery. The protocol does not solve this.
- **LLM output correctness** is not provable. v2 mitigates with stake, attestation, and log-prob sampling, but a sophisticated provider returning a subtly wrong answer to a non-deterministic prompt may not be caught.
- **TEE compromises** (microarchitectural attacks on Intel TDX, AMD SEV-SNP) would weaken the `Attested` tier. The protocol supports multiple TEE families to allow rapid migration.
- **DA layer availability.** If the DA layer fails, off-chain payloads become unreachable. The protocol can be configured with multiple DA endpoints.
- **Governance capture.** If governance is captured, parameters can be set adversarially. This is a general problem for all governed protocols.

### 24.3 What v2 does *not* protect against

- A user voluntarily granting unrestricted access to an agent and the agent misbehaving.
- An agent producing low-quality work that nonetheless passes verification (quality of output is the verifier's job, not the protocol's).
- Off-protocol agreements between users and providers.

---

## 25. MVP and Phasing — Realistic

### 25.1 Phase 1 — Launch (Months 0-6)

**Must ship:**
- Account & capability module with Ed25519 signatures, caveats §4.4 (the first 6 only), revocation SMT.
- Budget module with reservations, rate limits, refunds.
- Tool market with intent → quote → match → receipt → settle.
- Tool classes: `BROWSER`, `LLM_INFERENCE`, `TEST_RUNNER`, `FILE_STORAGE`.
- Provider staking and slashing (`Optimistic` and `Trusted` tiers only).
- TaskReceipt with tool_receipt_root binding.
- Policy templates: research, coding, verifier.
- Emergency stop.
- Wallet activity UI (reference web client) — static stub: `tools/wallet-web/` (`./scripts/serve-wallet-web.sh`); full in-browser verify deferred (use `fractal-wallet-cli cap show`).
- Reference provider SDK (TypeScript + Rust) — **`packages/fractal-provider-ts/`** (types + `npm run check`); Rust: **`fractal_sdk::provider`** in `crates/sdk-rust` (re-exports `fractal_wallet` market types, `IndexerCursor`, `IntentPollFilter`, `provider_id_from_public_key`).
- On-chain **TaskReceipt** anchor (W6-d): `fractal_core::NativeCall::WalletTaskReceiptAnchorV1` (`OP_WALLET_TASK_RECEIPT_ANCHOR_V1` = `0x0e`) + `State.wallet_task_receipt_anchors`; optional borsh witness verified with `cargo test -p fractal-core --features wallet`. Indexer poll stub: **`cargo run -p fractal-indexer-stub`** (`INDEXER_RPC_URL`, `INDEXER_POLL_MS`, optional `INDEXER_JSON_LOG=1`; also logs `eth_getBlockByNumber` tx counts). Sample provider HTTP: **`tools/provider-http-sample/`** (`./scripts/run-provider-http-sample.sh`; optional **`PROVIDER_ED25519_SEED_HEX`** + `pip install -r requirements-signing.txt` for Ed25519 over borsh `QuoteBody`, with `providerId = BLAKE3(providerPublicKey)`). Followers: comma-separated **`FRACTAL_BOOTSTRAP`** multiaddrs with the same `/p2p/<PeerId>` (W6-e). Borsh reference: `cargo run -p fractal-wallet --example dump_quote_body_borsh`.

### 25.2 Phase 2 — Hardening (Months 6-12)

- TEE attestation support (`Attested` tier) for Intel TDX, AMD SEV-SNP, AWS Nitro.
- `GITHUB_READ`, `GITHUB_WRITE`, `CODE_EXECUTION`, `DATABASE_QUERY` tool classes.
- Replicated verification (`Replicated` tier).
- Privacy envelope encryption with TEE-bound keys.
- Sub-agent delegation in production.
- Reputation indexer.
- Batch settlement.
- Cross-chain bridges in FractalEVM.

### 25.3 Phase 3 — Advanced (Months 12+)

- ZK proof-of-execution where applicable (`Proven` tier) for limited tool classes.
- Multi-agent shared budgets.
- Streaming payments for long-running tools.
- Marketplace specializations and custom matching contracts in EVM.
- Post-quantum capability format migration.

### 25.4 Explicitly out of scope (for v2.0)

- Private transactions (mixing, shielded transfers).
- Trustless LLM output verification.
- On-chain agent reasoning / "smart agents on chain."
- Native LLM provider operated by the protocol.

---

## 26. Open Questions From v1 — Answered

v1 left these dangling. v2 takes positions.

| v1 Question | v2 Answer |
|---|---|
| Should session wallets hold funds or only spend from parent escrow? | **Spend from parent budget account via reservations.** No standalone session funds; simpler bookkeeping, automatic refunds. |
| Should unused funds return automatically at expiry? | **Yes.** Capability expiry triggers refund of unspent reservations within the same block. |
| Should tool providers be paid instantly or batched? | **Batched after challenge window** for `Optimistic`; **instant** for `Trusted` low-stakes classes. Per-class governance setting. |
| Should tool access be priced in FRAC or native gas? | **Tool prices in FRAC.** Gas is separate and paid in FRAC (or sponsored). One asset for both keeps accounting simple. |
| Should LLM usage be paid by token estimate or final bill? | **Pre-committed cap with metered settle.** Quote commits to max input + max output tokens; final bill ≤ committed cap. Overage = provider eats it. |
| Should agents choose tool providers or should task owner? | **Agent chooses within owner-set caveats.** Owner can constrain providers via `Caveat::Providers(allowlist)` or require attestation. |
| Should wallet permissions be enforced at protocol or app layer? | **Protocol layer.** App-layer enforcement is theater — anyone can run a custom wallet client. |
| Should BIP-32 derivation be used directly? | **No.** Optional convenience for key material; not part of the authorization model. Capability tokens carry authorization. |
| Should session wallets require separate keys or be smart-account capabilities? | **Separate Ed25519 keys for sessions** (cheap, fast); the capability layer is on top. Smart-account-style features available via caveats. |
| Should agent reputation affect tool spend limits? | **Yes, optionally.** Policy templates can include `Caveat::MaxSpendForLowReputation`. Not required by protocol. |

---

## 27. The Core Primitive — Restated

v1 said the central primitive was the AgentSessionWallet. v2 corrects this: the central primitive is the **Capability Token**.

A capability token answers, in one cryptographically-verifiable object:

```
WHO       (subject: which agent session key)
WHAT      (scope: tool classes, providers, repositories)
WHERE     (chain_id, workspace, project, task)
WHEN      (not_before, not_after, time-of-day caveats)
HOW MUCH  (budget account binding, per-tool caps, per-call caps)
HOW FAST  (rate limits)
HOW STRICTLY  (verification tier, TEE requirements, approvals)
WITH WHOSE AUTHORITY  (issuer signature chain back to a master wallet)
UNTIL WHEN  (revocation status, expiry)
```

Wallets, budgets, and sessions are the *infrastructure* around this primitive. The primitive itself is bearer, attenuable, signature-chained, and protocol-verified.

The session is what *uses* a capability; the capability is what *grants* authority.

This distinction matters because:
- A single session can hold multiple capabilities (different scopes for different sub-tasks).
- A capability can outlive a session (re-bind to a new session key on rotation).
- Delegation is a capability operation, not a wallet operation.
- Audit reasons about capabilities, not wallets.

Once you put capabilities at the center, the rest of the system falls into place: budgets are what capabilities spend against; intents are what capabilities authorize; receipts are what capabilities consume; reputation is what providers earn for honoring capabilities.

---

## 28. Why This Beats Every Other Approach Tried So Far

This is the most important section. Without it, the spec is just engineering; with it, the position is clear.

**Existing chains were designed for humans approving transactions.** Every major chain assumes the signing party is a careful human who reviews each operation. Agents do not work that way. They fire hundreds of operations per minute, autonomously, often in parallel. Per-transaction human review is impossible; per-transaction protocol enforcement is mandatory.

**Smart-contract session keys are a retrofit, not a foundation.** ERC-4337 session keys, Solana Squads policies, and Lit Protocol conditions all bolt agent-style authorization onto a chain that was not designed for it. Each pays a tax: more gas, more contracts to audit, more state to read, more places for bugs. v2 makes capability checking a first-class native operation.

**Existing tool markets do not verify execution.** x402-style payment flows pay providers for HTTP responses but do not bind payment to verifiable execution. Restaking AVS slashing is for service-level guarantees, not per-call output correctness. v2 ties payment to *receipts with class-specific verification tiers*, and slashes on mismatch.

**Existing approaches conflate "wallet" with "agent."** They give the agent a wallet, then try to add restrictions. v2 inverts this: the agent has a session key with no inherent rights; rights come from a capability the user issues, which is mechanically bounded.

**Existing approaches have no story for delegation.** Real agent workflows are recursive — an agent spawns a verifier, a helper, a researcher. Without cryptographic attenuation, every delegation is a fresh authorization decision by the master. v2 makes attenuation a protocol property.

**Existing approaches handwave verification.** "The provider signs the receipt." That is not verification, that is attestation. v2 specifies *what verification means per class*: re-execute for browser, attest for TEE, replicate for GPU, sample log-probs for LLM.

The architectural claim of v2 is not that it is novel in every component. It is that each component is the right shape, that they compose, and that the composition is a system that an agent operator can actually run without going bankrupt to fraud or running out of compute on chain overhead.

---

## 29. Implementation Checklist

A minimum-viable, end-to-end implementation requires the following components, ordered by dependency:

```
□ FractalCore consensus layer (any sufficient base layer)
□ Account & key registry
□ Capability token format library (serialize, sign, verify, attenuate)
□ Revocation SMT module
□ Budget account module with atomic reserve/settle/refund
□ Token-bucket rate-limit module
□ Intent posting + quote handling
□ Settlement state machine
□ Receipt module with DA reference support
□ Provider stake module + slashing logic
□ Reputation derivation indexer
□ Challenge / adjudication module per class
□ Policy template registry
□ Task module (post, checkout, renew, submit, verify, finalize)
□ Emergency stop
□ Reference wallet client (web + CLI)
□ Reference provider SDK (Rust + TypeScript)
□ Reference DA integration (one option at launch)
□ Governance module
□ Indexer + dashboards
```

Each item has well-defined inputs, outputs, and acceptance tests. Nothing is gated on research breakthroughs. The hardest pieces are operationally — getting providers to stake, ensuring DA reliability, building good wallet UX — not cryptographically.

---

## 30. What Success Looks Like

Within twelve months of launch, the following should be true if the protocol is succeeding:

- **Tens of thousands of session capabilities active** at peak, with no observed unauthorized fund movements.
- **Dozens of independent providers per major tool class**, each with non-trivial stake.
- **At least one slashing event** has occurred and been resolved correctly (a system without slashes is a system whose slashing is untested).
- **Median agent workflow** completes in seconds, with sub-cent gas overhead per tool call.
- **At least one well-publicized adversarial test** (red-team or audit) of the capability model has been completed without breaks.
- **Reputation system has discriminated honest from dishonest providers** measurably.
- **Multiple third-party clients and provider implementations** exist (a protocol with one client is not a protocol).

Failure looks like: agents getting drained, master keys getting compromised, providers colluding undetected, the tool market consolidating to one provider per class, or the chain stalling under agent load. The spec is designed to make each of these specifically harder than in alternative designs.

---

## 31. Summary in Plain Terms

The point of this protocol, in one paragraph:

A user should be able to hand an AI agent a piece of paper that says *"you may spend up to ten dollars on web browsing and language model calls in the next hour, only for this specific task, only from approved providers, with the right to ask a verifier to double-check anything important, and at any moment I can tear this paper up and you instantly have nothing,"* and the agent can act fully autonomously within those limits, the chain enforces every word of it, every dollar spent leaves a verifiable receipt, providers who lie about their work lose more than they could ever gain, and a third party can reconstruct everything that happened with cryptographic certainty. The piece of paper is the capability token. The rest of the spec is the machinery that makes the piece of paper enforceable, attenuable, verifiable, and cheap.

---

*End of FractalWork Specification v2.0.*