# FractalChain L1 — Testnet PRD

## Product Requirements Document

**Product:** FractalChain L1 (the AI-agent-first blockchain)
**Version:** v0.2 (Target architecture + testnet track)
**Status:** Draft for Implementation
**Companion to:** FractalWork Core MVP PRD v0.2
**Primary Goal (testnet track):** Ship a fast, low-cost L1 testnet that settles FractalWork TaskReceipts on-chain, pays agents in testnet FRAC, and grows from 1 block producer to a 21-node HotStuff-2 BFT validator set without a hard fork.
**Primary Goal (v1.0+ target):** Scale to **sharded execution** with **pipelined HyperBFT** per shard, a **masterchain** for global coordination and ZK anchoring, and an **async permissionless prover pool** so consensus never waits on proofs while history stays **O(log n)** verifiable.

---

# 1. Executive Summary

FractalChain is a purpose-built Layer 1 designed around a single workload: **AI agents performing economic work, getting verified, and getting paid — at machine speed**.

Unlike general-purpose chains that bolt on AI as an afterthought, FractalChain's architecture treats the FractalWork primitives — `TaskReceipt`, `PayoutBatch`, `AgentRegistry`, `Verification` — as **first-class on-chain objects with dedicated precompiles**. General-purpose smart contracts (governance, DeFi, prediction markets on agent outcomes) run on a colocated EVM.

The chain ships in **two parallel tracks**:

**Track A — Testnet v0.1 (in repo today):** single logical chain, HotStuff-2 consensus, monolithic execution.

```text
Phase 1 (Singleton):    1 block producer, BFT-ready code path, public testnet
Phase 2 (BFT-7):        7-validator HotStuff-2, dynamic validator set
Phase 3 (BFT-21):       21-validator production target, full slashing
```

**Track B — Production target (this PRD v0.2):** **execution shards** + **masterchain** + **async ZK prover pool** (§6.2–§6.4, §7.9–§7.10). Consensus on shards stays **latency-bounded**; proofs are **retroactive** (chain live first, provable as provers catch up).

### Design Targets

| Metric | Testnet v0.1 (Track A) | Production target (Track B, per shard × N shards) |
|---|---|---|
| Block time | 500 ms | **70 ms** target (HyperBFT pipelined) |
| Finality | 1 block (HotStuff-2) | **≤ 200 ms** median, **≤ 900 ms** p99 (absolute, no reorgs) |
| Throughput | 5,000 TPS native / 1,500 TPS EVM (1 chain) | **~200K TPS** aggregate design budget (10 shards × ~20K TPS/shard — TBD per shard hardware) |
| TaskReceipt settlement | < $0.001 equivalent | Same economics at native precompile layer |
| Validator hardware | 8 vCPU / 32 GB RAM / NVMe | Shard validator: similar; provers: GPU-class optional |
| History growth | Linear block replay | **O(log n)** verifiable sync after ZK pruning (§6.3) |

### Why This Works for AI Agents

* **Sub-second finality** matches agent decision loops (LLM call → on-chain action → next LLM call).
* **Predictable gas** through native precompiles (receipt settlement is a fixed-cost opcode, not a 200-line Solidity loop).
* **Agent-native identity** via on-chain `AgentRegistry` precompile, not yet-another ERC-721.
* **Batch-friendly settlement** designed for the FractalWork rollup-style flow from day one.

---

# 2. Product Vision

The thesis is simple: **AI agents are economic actors. They need a chain that thinks of them as first-class users, not as awkward wrappers around human wallets.**

FractalChain is that chain.

* Humans use it occasionally (post jobs, govern, audit).
* Agents use it constantly (bid, submit, verify, settle, pay).
* The chain's mempool, gas market, and precompiles are tuned for the second case.

By v1.0 mainnet, FractalChain should be the default settlement layer for verifiable AI work — the way Bitcoin is the default settlement layer for cryptographic value transfer.

---

# 3. Core Thesis

**Most "AI + blockchain" projects fail because they treat AI work as arbitrary bytes.** They put a model hash on-chain, call it provenance, and ship.

FractalChain treats AI work as a **structured economic event** with native on-chain primitives:

```text
TaskReceipt    — proves work happened, who verified, what was paid
PayoutBatch    — Merkle-rooted batch of N receipts, settled in one tx
AgentRegistry  — on-chain agent identity with operator binding for Sybil resistance
Verification   — signed scoring tied to a receipt
DisputeRecord  — on-chain dispute filing and resolution
```

These aren't smart contracts. They're **precompiled opcodes** that read/write directly to dedicated state tries. This is what makes the chain fast and predictable for the agent workload.

---

# 4. Goals

## 4.1 Primary Goals

1. Ship a singleton-producer testnet (Phase 1) in ≤ 90 days.
2. Settle FractalWork Core MVP `PayoutBatch` roots on-chain.
3. Provide native FRAC token (gas + reward) with staking.
4. Provide on-chain `AgentRegistry` for worker/verifier agents.
5. Provide EVM coexistence for governance and future contracts.
6. Achieve sub-second block time and 1-block finality on the BFT path.
7. Grow validator set from 1 → 7 → 21 without hard fork.
8. Provide explorer, RPC, faucet, and developer docs.

## 4.2 Secondary Goals

1. Bridge testnet FRAC to a public EVM testnet (Sepolia) for liquidity testing.
2. Evolve toward **async ZK prover pool** with a **two-tier proof stack**: **STWO** (shard STARK / proof condensing) + **Plonky2** (masterchain recursive SNARK aggregation) — §6.2, §7.8; v0.1 ships the **STWO condenser** on the monolith first (M9).
3. Evolve toward **sharded HyperBFT** execution (§7.9, M10+) after testnet Track A stabilizes (§6.4).
4. Enable prediction markets on agent outcomes via EVM contracts.

---

# 5. Non-Goals

The testnet **will not** include:

* Mainnet token launch with real-money value
* Cross-chain bridges to mainnets
* Permissionless validator entry on day one
* On-chain LLM inference
* On-chain artifact storage (artifacts remain in S3/IPFS; only hashes on-chain)
* Full **synchronous** ZK verification on any BFT block critical path (would break sub-100 ms shard cadence). **Async** prover pool + masterchain posting (§6.2) is the only supported model for validity proofs.
* **Sharded masterchain topology** on testnet v0.1 exit (Track A remains one chain); shard + masterchain is Track B (§7.0 migration map).
* Complex on-chain governance (off-chain Snapshot-style for testnet)
* Account abstraction (EIP-4337) for v0.1
* Privacy features (mixers, stealth addresses)

---

# 6. High-Level Architecture

## 6.1 Target topology (Track B — production)

FractalChain v1.0+ is a **sharded L1**: each **shard** runs agent transactions and local state; the **masterchain** coordinates validators, anchors shard state, accepts ZK proofs, and routes cross-shard agent messages. **Provers are off-chain** and never block shard finality.

```text
┌─────────────────────────────────────────────────────────────────────────┐
│                     FractalWork Core MVP (off-chain)                     │
└───────────────────────────────┬─────────────────────────────────────────┘
                                │ PayoutBatch / agent txs
        ┌───────────────────────┼───────────────────────┐
        ▼                       ▼                       ▼
┌───────────────┐       ┌───────────────┐       ┌───────────────┐
│  Shard 0      │       │  Shard 1      │  ...  │  Shard N-1    │
│  HyperBFT     │       │  HyperBFT     │       │  HyperBFT     │
│  Native+EVM   │       │  Native+EVM   │       │  Native+EVM   │
│  state trie   │       │  state trie   │       │  state trie   │
└───────┬───────┘       └───────┬───────┘       └───────┬───────┘
        │   STARK proofs        │                       │
        └───────────────────────┼───────────────────────┘
                                ▼
                    ┌───────────────────────┐
                    │  Async ZK prover pool  │  (permissionless race)
                    │  STWO (shard) → Plonky2│
                    │  (masterchain SNARK)   │
                    └───────────┬───────────┘
                                ▼
                    ┌───────────────────────┐
                    │      Masterchain       │
                    │  validator set, shard  │
                    │  map, anchors, global  │
                    │  ZK root, cross-shard  │
                    │  agent messages        │
                    └───────────────────────┘
```

**Layer responsibilities:**

| Layer | Role | Executes agent txs? |
|---|---|---|
| **Shard** | Pipelined HyperBFT + FractalCore/EVM + per-shard MPT | **Yes** (primary workload) |
| **Async ZK pool** | **STWO** condenses each shard; **Plonky2** aggregates to `globalZkRoot` | No |
| **Masterchain** | Global validator set, shard map, anchors, validity proofs, cross-shard routing | **No** (coordination only) |

## 6.2 Async ZK prover pool

### 6.2.1 Design principle

ZK proof generation is **fully decoupled from consensus**. Provers are **off-chain**, **permissionless**, and **race** to generate proofs for shard block ranges. Proofs are posted to the **masterchain** when ready. **Consensus never waits for a proof.**

Implications:

* The chain is always **live and fast** on the shard hot path.
* It becomes **retroactively provable** as provers catch up.
* In steady state, **proof lag** is seconds to minutes depending on prover hardware.

### 6.2.2 Proof scheme (two tiers: STWO → Plonky2)

Proof generation is intentionally **split across two systems**. Each tier does what it is best at; neither runs on the consensus critical path (§6.2.1).

| Tier | System | Where it runs | Properties | Output |
|---|---|---|---|---|
| **1 — Shard condensing** | **STWO** (+ RISC-V trace) | Per-shard provers (permissionless race) | Transparent STARK, no trusted setup, quantum-resistant; larger proofs OK off-chain | `StarkProof` over block range `[N..M]` binding `state_root`, execution, `witnessCommitment` |
| **2 — Global aggregation** | **Plonky2** | Aggregator provers (permissionless race or rotating set) | Recursive **SNARK**; compact verify on masterchain and external chains | Single proof → `globalZkRoot` in masterchain block |

```text
Shard blocks N..M
    →  witness (RISC-V replay of native + EVM)
    →  STWO prove  ──►  shard STARK  (ProofSubmission.proof)
                              │
         (all shards)         ▼
    Plonky2 recursive circuit verifies STARK statements
    →  one SNARK  ──►  masterchain globalZkRoot
```

**Why two tiers (mental model):**

* **STWO** is the **proof condenser** for execution: it argues “this shard state transition is correct” over a RISC-V trace without trusted setup. This is what `fractal-proof-condenser` implements today (checkpoint-scale witnesses first; full block ranges in M10+).
* **Plonky2** is the **compressor across shards**: it takes many shard STARKs (or their verified statements) and produces **one small proof** the masterchain (and bridges) can check cheaply. STWO alone at masterchain scale would be bulky; Plonky2 is the planned **aggregation layer**.

**Relationship to testnet code (Track A):**

* **In repo today:** `fractal-proof-condenser` — async **STWO** over checkpoint jobs, `CheckpointStwoArtifactV1`, RocksDB `checkpoint_proofs`, RPC `fractal_getCheckpointProof*` (§7.8, M9). **Plonky2 is not in the repo yet.**
* **Track B:** same STWO path per shard + new **Plonky2 aggregator** crate/worker posting to masterchain (M11).

**Wire shape (unchanged intent, explicit types):**

```typescript
interface ProofSubmission {
  shardId: ShardId;
  blockRange: [u64, u64];
  proof: StarkProof;        // STWO output (tier 1)
  proverAddress: Address;
  lagSeconds: u32;
}

// Masterchain block carries tier-2 result:
// globalZkRoot = hash/bind Plonky2 SNARK over accepted shard STARKs for this height
```

### 6.2.3 Block pruning

Once a **validity proof** covers a range of shard blocks, those **raw blocks are prunable**. Verification needs only:

* The **state root** at the covered tip, and
* The **proof chain** (O(log n) proofs, not O(n) blocks).

New nodes sync by downloading:

1. Current **masterchain** state root (and shard map), and
2. **Proof chain** back to genesis or last trusted checkpoint.

Without pruning, at a design budget of **200K TPS × 10 shards**, raw block data could grow on the order of **~10 TB/day**. With ZK compression, the **provable** chain state grows **O(log n)** in proof depth, not linearly in block count.

**Pruning policy (parameters):** `PRUNE_AFTER_VALIDITY_PROOF = true` once masterchain accepts proof for `[shardId, startBlock, endBlock]`; retain witness commitments until proof finality (see `witnessCommitment` in §7.10).

### 6.2.4 Prover incentives

Provers earn a reward from the **protocol treasury** for each **accepted** proof. Reward is proportional to the **block range covered** and **inversely proportional to proof lag** (faster provers earn more). This creates a competitive prover market **without** requiring provers to be validators.

```typescript
interface ProofSubmission {
  shardId: ShardId;
  blockRange: [u64, u64];   // [startBlock, endBlock]
  proof: StarkProof;
  proverAddress: Address;
  lagSeconds: u32;          // time since last block in range was finalized
}
```

On masterchain inclusion, the protocol verifies the proof statement (shard id, range, state transition binding) and credits `proverAddress` per the incentive curve (exact formula TBD in economics spec).

## 6.3 Testnet v0.1 topology (Track A — in repo)

Track A remains a **single chain** for M1–M8. The diagram below is what ships first; §7.0 maps each box to Track B.

```text
┌──────────────────────────────────────────────────────────────────┐
│                      FractalWork Core MVP                        │
│  (off-chain backend: jobs, bids, work, verification, receipts)   │
└─────────────────────────────────┬────────────────────────────────┘
                                  │
                                  ▼ PayoutBatch (signed)
┌──────────────────────────────────────────────────────────────────┐
│                    FractalChain L1 (monolith)                    │
│  ┌────────────────────────────────────────────────────────────┐  │
│  │   Consensus: HotStuff-2 (Phase 1 = singleton, 2 = 7, 3 = 21)│  │
│  └────────────────────────────────────────────────────────────┘  │
│  ┌────────────────────────────────────────────────────────────┐  │
│  │              P2P Networking (libp2p + QUIC)                │  │
│  └────────────────────────────────────────────────────────────┘  │
│  ┌────────────────────────────────────────────────────────────┐  │
│  │                       Execution Layer                      │  │
│  │  Native VM (FractalCore) + EVM (revm) → unified state trie │  │
│  └────────────────────────────────────────────────────────────┘  │
│  ┌────────────────────────────────────────────────────────────┐  │
│  │              Storage (RocksDB column families)               │  │
│  └────────────────────────────────────────────────────────────┘  │
│  ┌────────────────────────────────────────────────────────────┐  │
│  │     RPC: JSON-RPC (EVM-compat) + fractal_* extensions       │  │
│  └────────────────────────────────────────────────────────────┘  │
│  Optional: async STWO checkpoint worker (§7.8) — not on hot path │
└──────────────────────────────────────────────────────────────────┘
```

## 6.4 How to migrate from Track A (v0.1) to Track B (v1.0+)

This table is the **implementation migration guide** for engineers updating the repo and ops after testnet stabilizes. Order is approximate; some work can proceed in parallel.

| Current (Track A / PRD v0.1) | Target (Track B) | Migration steps |
|---|---|---|
| Single `chain_id`, one `NodeInner` state | `ShardId` + per-shard state machines | Introduce `shard_id` in tx envelope; route txs by shard key (e.g. agent id hash mod N); keep monolith as **shard 0** until cutover |
| HotStuff-2 in `fractal-consensus` | HyperBFT pipelined per shard (§7.9) | New crate or `consensus::hyperbft` module; preserve HotStuff-2 for regression tests; shard validators run one instance per assigned shard |
| `Vote` / `QuorumCertificate` / `cf_consensus` | `CommitSig` pipeline + shard-local QC store | Extend RocksDB keys with `shard_id`; map existing vote pool semantics to pipelined commit stage |
| Monolithic `state_root` in block header | Per-shard `state_root` + masterchain `globalStateRoot` | Shard blocks carry local root; masterchain block merkleizes shard roots (§7.10) |
| `cf_state` full `State` snapshots | Shard MPT + pruning after proof | Keep snapshots for devnet; production nodes prune per §6.2.3; sync via proof chain |
| `fractal-proof-condenser` (STWO only) | Tier-1 shard provers + tier-2 **Plonky2** aggregator | Split crate: keep `proof-condenser` for STWO; add `proof-aggregator` (Plonky2) in M11; masterchain `SubmitValidityProof` + `globalZkRoot` |
| No cross-shard | `crossShardMessages` in masterchain block | Agent message format + routing table; agents pin to home shard, masterchain delivers at anchor cadence |
| 500 ms block time target | 70 ms shard block time | Re-tune pacemaker, networking, optimistic execution; separate milestone (M10+) |
| libp2p single mesh | Per-shard validator mesh + masterchain validators | Partition gossip topics by `shard_id`; masterchain peers subscribe to anchor/proof topics only |
| RPC single endpoint | Shard-aware RPC (`eth_chainId` encodes shard or explicit `fractal_shardId`) | Gateway routes reads/writes to correct shard backend |

**Hard fork boundary:** Track B is expected to require a **coordinated upgrade** (new genesis or state export) once shard count and masterchain genesis are fixed — not a silent swap. Testnet Track A remains valid until announced sunset.

**Do not break Track A milestones:** M1–M8 exit criteria stay on HotStuff-2 monolith. M10+ (below) scopes Track B.

## 6.5 Shared execution and storage (both tracks)

Both tracks keep: **Native VM + EVM** on a **unified per-shard state trie** (Merkle Patricia for EVM accounts; native subtries for FractalCore), **RocksDB** column families (§10.3), and **deterministic ordering within a shard** (§7.9.4). Cross-shard ordering is **not** merged at mempool level — only via masterchain anchors and explicit agent messages.

---

# 7. Consensus Layer

## 7.0 Consensus roadmap (two tracks)

| Track | Algorithm | Scope | Status |
|---|---|---|---|
| **A** | HotStuff-2 | Single chain, testnet v0.1 | **In repo** (`fractal-consensus`, `fractal-node`) — §7.1–§7.7 |
| **B** | HyperBFT-derived pipelined BFT | One instance **per shard** | **Specified here** — §7.9; implementation M10+ |
| **Coordination** | Masterchain BFT (lightweight) | Validator set, anchors, proofs | **Specified here** — §7.10; implementation M11+ |

Shard consensus (Track B) and masterchain consensus may share the same codebase with different parameters, or use a smaller validator committee on the masterchain — **TBD**. What is fixed: **shard validators execute agent work**; **masterchain validators do not**.

---

## 7.1 Algorithm Choice: HotStuff-2 (Track A — testnet v0.1)

*This section describes the consensus algorithm shipping in M1–M8. Production shards use §7.9 instead; see §6.4 for migration.*

**Why HotStuff-2 over alternatives:**

| Algorithm | Why Not |
|---|---|
| Tendermint / CometBFT | 3-round commit (propose → prevote → precommit); higher latency at 21 nodes |
| Original HotStuff | 3-chain rule; one extra round vs HotStuff-2 |
| Aptos AptosBFT (DiemBFT v4) | Strong choice; HotStuff-2 is the public successor with the same lineage |
| Narwhal/Bullshark | Excellent throughput but more complex; defer to v2 if mempool becomes bottleneck |
| Solana TowerBFT | Tied to Solana's PoH; not portable |
| Avalanche Snowman | Probabilistic finality; we want deterministic |

**HotStuff-2** gives us:

* 2-chain rule (one fewer round than HotStuff)
* Linear communication complexity per view (`O(n)`)
* Pipelined block proposal (next leader proposes while current commits)
* Deterministic finality in one block under normal operation
* Well-studied; production deployments at Aptos and others

## 7.2 Validator Set

### Phase 1 — Singleton (v0.1 testnet launch)

* 1 block producer, run by the FractalWork team
* All other nodes are full nodes (sync, serve RPC, do not produce)
* Code path is **the BFT path**, with `n=1, f=0`. No special-case "PoA mode."
* Block time: 500 ms
* This proves the entire stack end-to-end without coordination overhead.

### Phase 2 — BFT-7 (~Month 3-4)

* 7 validators (5 team-operated + 2 external partners)
* `n=7, f=2` (tolerates 2 Byzantine)
* Validator set rotates via on-chain governance call (no hard fork)
* Block time: still 500 ms target

### Phase 3 — BFT-21 (~Month 6+)

* 21 validators (production target)
* `n=21, f=6`
* Permissioned validator entry via governance vote in testnet; permissionless with stake threshold in mainnet
* Block time: 500 ms

### Why the 1 → 7 → 21 path

1. **Singleton ships fastest** and exposes integration bugs without consensus complexity.
2. **7 nodes** is the smallest non-trivial BFT set; surfaces network partition and view-change bugs.
3. **21 nodes** is the production target — large enough for credible decentralization, small enough for sub-second consensus.

## 7.3 Block Structure

```rust
struct Block {
    header: BlockHeader,
    transactions: Vec<Transaction>,
    qc: QuorumCertificate,        // certifies the *parent* block
    signature: Ed25519Signature,  // proposer's signature
}

struct BlockHeader {
    version: u16,                 // schema version
    chain_id: u64,                // 41 for testnet, TBD for mainnet
    height: u64,
    view: u64,                    // HotStuff-2 view number
    parent_hash: Hash,            // hash of parent block
    parent_qc_hash: Hash,         // hash of the QC certifying parent
    proposer: ValidatorId,
    timestamp_ms: u64,            // proposer's clock; bounded by view timeout
    state_root: Hash,             // unified state trie root (post-execution)
    tx_root: Hash,                // Merkle root of transactions
    receipt_root: Hash,           // Merkle root of execution receipts
    native_event_root: Hash,      // Merkle root of FractalCore native events
    gas_used: u64,
    gas_limit: u64,
    extra: Bytes,                 // ≤ 32 bytes, e.g., client version tag
}

struct QuorumCertificate {
    block_hash: Hash,
    view: u64,
    height: u64,
    signatures: BLSAggregateSignature,  // aggregated over 2f+1 validators
    signer_bitmap: BitVec,              // which validators signed
}
```

### Notes

* **BLS aggregation** keeps QC size constant regardless of validator count (~96 bytes for the aggregate).
* `parent_qc_hash` is the HotStuff-2 chaining mechanism: committing block B requires a QC over B's child.
* `native_event_root` is the FractalCore event Merkle root — this is what makes on-chain native ops auditable the same way the off-chain MVP is.

## 7.4 Block Production Flow

```text
View v, leader L:

1. L collects transactions from mempool (priority-ordered, see §9.3)
2. L executes them against state at parent.state_root
3. L builds Block_v with new state_root, tx_root, receipt_root, event_root
4. L broadcasts Propose(Block_v, parent_QC)
5. Each validator V:
   a. Verifies parent_QC (BLS aggregate over 2f+1)
   b. Verifies proposer is the legitimate leader for view v
   c. Re-executes transactions deterministically; compares state_root
   d. If valid, signs Vote(Block_v.hash, v) and sends to next leader L'
6. L' aggregates 2f+1 votes into QC_v
7. QC_v is included in Block_{v+1}, which COMMITS Block_v

Pipeline: while L' is proposing v+1, validators can already act on Block_v's
committed state — because HotStuff-2 commits Block_v when QC_{v+1} forms.
```

### Block Time

* 500 ms target view duration.
* If leader times out (no proposal in 500 ms), view-change triggers.
* `next_view_timeout = base * 2^consecutive_failures`, capped at 60 s.

## 7.5 Leader Rotation

* **Round-robin** in Phase 1 trivially (one validator).
* **Round-robin by stake-weighted permutation** in Phase 2-3, reshuffled each epoch.
* Epoch = 10,000 blocks (~83 minutes at 500 ms).
* Reshuffling seed = hash of last block of previous epoch.

## 7.6 Slashing (Phase 2+)

Three slashable offenses:

| Offense | Slash Amount | Detection |
|---|---|---|
| **Double-sign** (two blocks at same height/view) | 100% of stake | Any node submits both signed blocks as evidence |
| **Surround vote** (vote inconsistent with prior commits) | 50% of stake | Validator submits conflicting QC evidence |
| **Sustained downtime** (< 80% participation over an epoch) | 1% of stake + jail | Automatic on epoch close |

Evidence is submitted as a special `SlashTransaction` and processed by the native slashing precompile.

## 7.7 Liveness and Safety

* **Safety**: under HotStuff-2, no two honest validators commit conflicting blocks at the same height, provided `f < n/3`.
* **Liveness**: progress under partial synchrony; bounded message delay after GST (Global Stabilization Time).
* **View-change cost**: O(n) messages; aggregated via BLS.

## 7.8 Proof condensing: STWO (tier 1) + Plonky2 (tier 2)

**Normative async-ZK model:** §6.2.2 (STWO shard STARKs → Plonky2 masterchain SNARK). This section documents **tier 1** in the repo today and **tier 2** as the planned aggregator.

The **condenser** (STWO) sits **beside** HotStuff-2 (Track A) or **beside** HyperBFT (Track B), never inside propose–vote–commit. The **Plonky2 aggregator** sits beside the **masterchain** only — it never executes agent txs. Together they enable **pruning** (§6.2.3) without slowing shard BFT.

### Problem statement

As blocks accrue, **replay-based sync** and **deep historical audit** scale linearly with chain size. A fixed **500 ms** BFT cadence must not be held hostage by heavyweight proving or verification. The design therefore **decouples** (1) *what the chain commits to next* from (2) *how aggressively the network condenses already-committed data into STARK proofs*.

### Tier 1 — STWO proof condenser (implemented / evolving)

**STWO** (StarkWare’s STARK stack, vendored in `third_party/stwo`, used from `fractal-proof-condenser`) is the **mandated tier-1** prover. It **condenses** finalized execution into STARK proofs. Proofs attest to well-defined statements, for example:

* A transition from **state root A** to **state root B** over an ordered batch of blocks or txs, or
* Inclusion and correctness of **native + EVM** execution summarized in a **RISC-V** trace, or
* A checkpoint over **N** blocks binding headers, execution results, and consensus metadata.

Exact statement grammars, proof versions, and on-chain verifier precompiles are **TBD**; this PRD fixes the **two-tier separation** (STWO vs Plonky2), not every circuit release.

**Artifacts today:** `CheckpointStwoArtifactV1` (borsh job + STWO proof JSON), `PersistedCheckpointProofV1` in RocksDB `checkpoint_proofs`, blake3 fallback when STWO fails (`docs/devnet.md`). The tier-1 job now includes replay-trace public inputs: `riscvTraceRoot` and `riscvTraceSteps` from the deterministic `FRACRV02` block-range harness, which validates contiguous heights, parent links, and transaction roots before proving.

### Tier 2 — Plonky2 recursive SNARK (planned, M11)

**Plonky2** is the **mandated tier-2** system for masterchain aggregation (§6.2.2):

* **Input:** verified statements from shard **STWO** proofs (one or more shards per masterchain block).
* **Output:** a single recursive **SNARK** committed as `globalZkRoot`.
* **Why Plonky2:** fast recursive composition, relatively small proofs on-chain, mature open-source stack (Polygon Zero lineage); fits “many STARKs → one SNARK” better than posting raw STWO proofs to the masterchain.

**Not in repo yet.** Planned deliverables:

* `fractal-proof-aggregator` (or submodule) wrapping Plonky2 verify-of-STARK-statement circuits.
* Aggregator worker: collect accepted `ProofSubmission`s → prove → submit to masterchain.
* Masterchain light-client verifier: check Plonky2 SNARK + `globalStateRoot` (§7.10).

Alternatives (e.g. Groth16 wrapper, STWO-only recursion) remain possible for experiments but **Plonky2 is the default architecture** unless superseded by explicit governance.

### RISC-V as the provable execution ISA (feeds tier 1 only)

**RISC-V** is the preferred **ISA for the trace** that the prover argues about: tooling, emulators, and future hardware extensions align with a single, well-understood instruction set. The **L1 execution engines** (native FractalCore + revm) remain authoritative for consensus; the RISC-V layer is the **canonical replay harness** inside the proof circuit (or a wrapper that produces a fixed trace format the STWO pipeline consumes). This keeps proving **fast to iterate** and **amenable to specialization** without changing the on-chain opcode surface for ordinary txs.

### x86_64 reference port (development and CI)

The **default machine class** for building, running, and **automated testing** of the async proof condenser today is **64-bit x86 Linux** (`x86_64-unknown-linux-gnu`) and typical **developer laptops** (including Apple Silicon via the same Rust nightly pin in `rust-toolchain.toml`). This is **not** an alternate “product ISA”: consensus and execution remain as elsewhere in this PRD; x86_64 is the **reference environment** where STWO prove/verify, `tokio::task::spawn_blocking`, and the full workspace are expected to pass in CI and on engineer machines **before** optional bring-up on **riscv64** VMs or hardware.

**Smoke commands (repo root, nightly from `rust-toolchain.toml`):**

* `cargo tree -p fractal-proof-condenser -i stwo` — confirm `stwo` resolves from `third_party/stwo` (Cargo patch), not crates.io alone.
* `cargo test -p fractal-proof-condenser` — async STWO path + checkpoint witness tests.
* `cargo test --workspace` — full regression including condenser integration.

Operator and toolchain notes: **`docs/stwo-run-notes.md`** (patch, pinned nightly, optional `riscv64gc-unknown-linux-gnu` cross-check when that target is installed).

### Async “spare compute” on nodes

Validators and **full nodes** expose **flexible worker capacity**:

* A **scheduler** (per-node, plus optional gossip of job metadata) assigns proof jobs from a queue: e.g. “prove blocks \[H..H+k\]”, “compress receipts epoch E”, “roll forward last checkpoint”.
* Workers use **idle CPU / GPU / second-tier cores** so proof work **does not contend** with the latency-sensitive consensus and RPC threads.
* **Completion is non-blocking:** when a job finishes, the resulting **proof artifact + public inputs** are submitted for inclusion in a **later** block (header extension, dedicated tx type, or sidecar committed by hash in-header). If a proof is late or skipped, **safety is unchanged** — the canonical chain is still the BFT-committed block sequence; proofs only strengthen **verifiability and sync economics**.

### Interaction with HotStuff-2

* **Hot path:** leaders propose, validators vote, QCs form — **unchanged**; no step waits on STWO completion.
* **Cold path:** proof workers consume finalized blocks, build traces, generate proofs, and gossip or publish results; inclusion policy (mandatory per epoch vs. best-effort) is a **parameterizable liveness/UX tradeoff**, not a consensus safety dependency for v0.1.

### Why this keeps the chain “hot”

BFT stays **hot** (sub-second, quorum-driven) because **proof throughput is elastic**: the network can add nodes, widen proof windows, or shrink proof granularity under load. Chain **operational throughput** for user txs is not throttled by worst-case proving time; at worst, **succinct verification lags** until proofs land, while full nodes still have full state.

### Deliverables sketch (future milestone)

Wire formats for proof bundles, verifier gas/precompile budget, slashing for **invalid** proofs when proofs are mandatory, and light-client sync using the latest valid checkpoint. Tracked under **§18 M9** (STWO condenser on monolith), **M10** (shard-scale STWO), **M11** (Plonky2 + masterchain). **Tier-1 wire bundle today:** `CheckpointStwoArtifactV1` + `docs/stwo-run-notes.md`. **Invalid proof slashing today:** `InvalidProofSlashEventV1` rows burn registered prover bond once per unique evidence hash and deactivate under-bonded identities. **Tier-2 wire bundle TBD:** Plonky2 proof bytes + public inputs for `globalZkRoot`.

---

## 7.9 HyperBFT pipelined consensus (Track B — per shard)

Each **shard** runs an independent instance of a **HyperBFT-derived** BFT protocol. This replaces HotStuff-2 on the shard hot path for v1.0+ (§6.4).

### 7.9.1 Overview

Key properties:

| Property | Detail |
|---|---|
| Message complexity | **O(n)** — leader aggregates votes, no all-to-all broadcast |
| Pipelining | Block **N** commits while **N+1** votes and **N+2** is proposed — no idle rounds |
| Optimistic execution | Txs execute **speculatively** before commit; **rollback on rejection** (rare with honest 2/3+ quorum) |
| Optimistic responsiveness | Blocks produced as soon as quorum reached, not on a fixed wall-clock timer alone |
| Finality | **Absolute** — no forks, no reorgs, ever |
| Fault tolerance | Up to **1/3** Byzantine validators (by voting weight) |

### 7.9.2 Block timing (shard parameters)

| Parameter | Value |
|---|---|
| Target block time | **70 ms** |
| Finality | **≤ 200 ms** (median), **≤ 900 ms** (p99) |
| Leader rotation | Every **epoch** (configurable; default **100 blocks**) |
| Quorum threshold | **2/3 + 1** of validator **voting weight** |

Track A’s 500 ms / HotStuff-2 targets remain valid until shard cutover (§6.4).

### 7.9.3 Pipeline stages

Three stages run **concurrently** on each shard:

```text
Round T:    [Propose N+2] [Vote N+1  ] [Commit N  ]
Round T+1:  [Propose N+3] [Vote N+2  ] [Commit N+1]
Round T+2:  [Propose N+4] [Vote N+3  ] [Commit N+2]
```

A block is **committed** once it collects **2/3+ `CommitSig`** signatures (BLS-aggregated or equivalent). The leader for round **T+1** is known **deterministically** from the validator set — they begin proposing before **T** commits.

**Mapping from Track A:** HotStuff-2’s propose → vote → QC-in-next-block maps to this pipeline; implementation should reuse `Vote`/`QuorumCertificate` crypto where possible but **must not** serialize the pipeline to one block per round.

### 7.9.4 Deterministic ordering guarantee (hard requirement for AI agents)

Within a shard, transaction ordering is **fully deterministic**:

* The **leader sequences** all transactions in the proposal.
* Validators **accept or reject the entire proposal** (no partial reordering).
* There is **no mempool reordering after proposal**.

This is a **hard requirement** for AI agent state machines: two agents submitting conflicting state transitions must see the **same canonical ordering**. Cross-shard conflicts are resolved by **home shard** assignment + masterchain cross-shard message ordering (§7.10), not by competing mempool sorts.

### 7.9.5 Shard block header (sketch)

Shard blocks extend the Track A header with `shard_id` and optional `witness_commitment` for provers:

```rust
struct ShardBlockHeader {
    shard_id: u32,
    height: u64,
    round: u64,                    // pipeline round (analogous to view)
    parent_hash: Hash256,
    state_root: Hash256,           // post-execution, this shard only
    tx_root: Hash256,
    witness_commitment: Hash256,   // commitment to witness data for ZK prover (§7.10)
    // ... gas, timestamp, proposer, commit_cert_hash, etc.
}
```

---

## 7.10 Masterchain (Track B — coordination layer)

The **masterchain does not execute agent transactions**. It is **coordination and anchoring only**.

### 7.10.1 Responsibilities

| Function | Description |
|---|---|
| **Global validator set** | Canonical validator registry and stake weights |
| **Shard map** | Which validators serve which shards; shard count parameter |
| **State anchors** | Each shard’s `state_root` + height on a schedule |
| **Validity proofs** | Accept `ProofSubmission` from async prover pool (§6.2) |
| **Cross-shard routing** | `AgentMessage` delivery between shards |
| **Global roots** | `globalStateRoot` (merkle over shard roots), `globalZkRoot` (recursive SNARK) |

### 7.10.2 Anchor frequency

Shards anchor to the masterchain every **`ANCHOR_INTERVAL`** shard blocks (default: **100** shard blocks ≈ **7 s** at 70 ms/block).

* Batches cross-shard coordination so the masterchain is not per-tx bottleneck.
* Agents needing cross-shard delivery **faster than ~7 s** may request **priority anchoring** at a higher fee (parameter TBD).

### 7.10.3 Global state structure

```typescript
interface MasterchainBlock {
  height: u64;
  shardAnchors: Map<ShardId, ShardAnchor>;
  validityProofs: Map<ShardId, ProofSubmission>;
  globalStateRoot: Hash256;   // merkle root of all shard state roots
  globalZkRoot: Hash256;      // recursive SNARK covering all shard proofs
  crossShardMessages: AgentMessage[];
}

interface ShardAnchor {
  shardId: ShardId;
  blockHeight: u64;
  stateRoot: Hash256;
  witnessCommitment: Hash256; // commitment to witness data for ZK prover
}
```

**Verifier flow:** light clients trust masterchain finality → check `globalZkRoot` → optionally verify single shard proof → read shard state root at anchor.

### 7.10.4 Relationship to Track A blocks

During migration, the monolithic chain can be treated as **shard 0** with `ANCHOR_INTERVAL = ∞` (anchors implicit) until masterchain genesis launches (§6.4).

---

# 8. Networking Layer

**Track B note:** validators participate in a **per-shard** libp2p mesh (or subset mesh per assignment) **and** optionally a **masterchain** mesh for anchors/proofs. Full nodes subscribe to shard topics they serve; provers gossip proof jobs and submissions independently of shard BFT traffic.

## 8.1 Stack

```text
Transport:        QUIC (multiplexed, encrypted, 0-RTT reconnect)
Framework:        libp2p (Rust impl)
Discovery:        Kademlia DHT + hardcoded bootnodes
Encryption:       Noise XX handshake (libp2p default)
Peer Identity:    Ed25519 (same key as consensus identity for validators)
```

**Why QUIC over TCP**: head-of-line blocking is real at 500 ms blocks. QUIC's stream multiplexing lets us send block proposals and votes in parallel without one stalling the other.

## 8.2 Gossip Topology

* **Validator mesh**: all 21 validators maintain direct connections (full mesh). At n=21, this is 210 edges — trivial.
* **Full nodes**: gossipsub mesh, ~8 peers each.
* **Light clients**: pull-only via RPC; do not gossip.

## 8.3 Message Types

| Topic | Payload | Propagation |
|---|---|---|
| `proposals` | Block proposals | Validator mesh + gossipsub to full nodes |
| `votes` | HotStuff-2 votes | Direct to next leader (no gossip) |
| `qcs` | Quorum certificates | Piggybacked in next block; standalone gossip on view-change |
| `txs` | User transactions | Gossipsub, all nodes |
| `evidence` | Slashing evidence | Gossipsub, all nodes |
| `proof_artifacts` (future) | STWO checkpoint metadata / proof bundle refs (§7.8) | Optional gossipsub; inclusion via block commitment |

## 8.4 Bandwidth Budget

At 500 ms blocks, 5,000 native TPS, ~200 bytes/tx average:
* Block size: ~500 KB
* Per-validator outbound: 500 KB × 20 peers / 0.5 s = 20 MB/s = 160 Mbps
* **Validator requirement: 200 Mbps symmetric minimum, 1 Gbps recommended.**

---

# 9. Execution Layer

## 9.1 Hybrid VM Design

Two execution engines share one state:

```text
┌────────────────────────────────────────────────────────────┐
│                  Transaction Router                        │
│  (dispatches by tx.target_vm field)                        │
└──────────────┬─────────────────────────┬───────────────────┘
               │                         │
               ▼                         ▼
       ┌───────────────┐         ┌───────────────┐
       │  Native VM    │         │  EVM (revm)   │
       │  (Rust)       │         │               │
       └───────┬───────┘         └───────┬───────┘
               │                         │
               └───────────┬─────────────┘
                           ▼
              ┌─────────────────────────┐
              │   Unified State Trie    │
              │  ┌───────────────────┐  │
              │  │ accounts/         │  │  ← shared
              │  │ evm_code/         │  │  ← EVM only
              │  │ evm_storage/      │  │  ← EVM only
              │  │ native_receipts/  │  │  ← Native only
              │  │ native_batches/   │  │  ← Native only
              │  │ native_agents/    │  │  ← Native only
              │  │ native_verifs/    │  │  ← Native only
              │  │ native_disputes/  │  │  ← Native only
              │  │ stake/            │  │  ← Native only
              │  └───────────────────┘  │
              └─────────────────────────┘
```

### Account Model

Unified account model (not Bitcoin UTXO):

```rust
struct Account {
    address: Address,            // 20 bytes, same format as EVM
    nonce: u64,
    balance: U256,               // in fwei (10^-18 FRAC)
    code_hash: Option<Hash>,     // if EVM contract
    storage_root: Option<Hash>,  // if EVM contract
    agent_id: Option<AgentId>,   // if registered as agent
    account_kind: AccountKind,   // EOA | EVMContract | Agent | Validator
    schema_version: u16,
}
```

Same address space, same nonce model. An agent is an EOA that has been registered in the AgentRegistry (cheap pointer; no separate balance).

## 9.2 Native VM: FractalCore Precompiles

The native VM is a small, hand-written Rust executor for ~10 precompiled opcodes corresponding to FractalCore primitives. Each opcode:

* Has a fixed gas cost (no Turing-completeness inside the opcode)
* Reads/writes dedicated state subtries
* Emits a `NativeEvent` recorded in `native_event_root`

### Opcode Set (v0.1)

```text
0x01 REGISTER_AGENT      (operator_addr, pubkey, kind, metadata_uri)
0x02 UPDATE_AGENT        (agent_id, new_metadata_uri, new_pubkey?)
0x03 SUSPEND_AGENT       (agent_id, reason) [admin/governance only in Phase 1]
0x04 SETTLE_RECEIPT      (receipt: TaskReceipt, signatures)
0x05 SETTLE_BATCH        (batch: PayoutBatch, validator_signature)
0x06 CLAIM_PAYOUT        (batch_id, account_id, amount, merkle_proof)
0x07 FILE_DISPUTE        (receipt_id, reason_code, evidence_hash)
0x08 RESOLVE_DISPUTE     (dispute_id, resolution, payouts_diff)
0x09 STAKE               (amount)
0x0A UNSTAKE             (amount)            [with unbonding period]
0x0B SLASH               (validator_id, evidence)
0x0C DELEGATE            (validator_id, amount)
0x0D WITHDRAW_REWARDS    (validator_id)
```

### Gas Costs (Native — Phase 1)

| Opcode | Gas | Rationale |
|---|---|---|
| REGISTER_AGENT | 5,000 | One-time write to agent trie |
| SETTLE_RECEIPT | 8,000 | Fixed-cost: verify N sigs, insert into trie |
| SETTLE_BATCH (N receipts) | 15,000 + 200×N | Amortized — this is the hot path |
| CLAIM_PAYOUT | 12,000 | Merkle proof verification |
| FILE_DISPUTE | 10,000 | Includes evidence hash storage |
| STAKE/UNSTAKE | 8,000 | Standard transfer + state update |
| Per-byte calldata | 4 | Same as EVM for compatibility |

**Comparison**: a Solidity equivalent of `SETTLE_BATCH` with 100 receipts would cost ~2-5M gas. The native version costs ~35K gas — **~100× cheaper**. This is the entire reason for the hybrid VM.

### TaskReceipt Native Format

```rust
struct OnChainTaskReceipt {
    receipt_id: Hash,              // 32 bytes
    job_id: Hash,                  // off-chain reference
    requester: Address,
    worker: AgentId,               // u64 (compact)
    verifier: AgentId,             // u64
    artifact_root: Hash,
    output_hash: Hash,
    score: u8,                     // 0-100
    payout_amount: u64,            // in fwei
    verifier_fee: u64,
    protocol_fee: u64,
    final_status: ReceiptStatus,   // enum u8
    finalized_at: u64,             // chain timestamp
    signatures: SignatureBundle,   // requester + worker + verifier
    schema_version: u16,
    tool_class: u8,                // wallet ToolClass discriminant (Browser=0, …)
}
```

Compact binary encoding (SCALE or borsh). Per-receipt on-chain footprint: ~250 bytes.

### PayoutBatch Native Format

```rust
struct OnChainPayoutBatch {
    batch_id: Hash,
    operator: Address,             // who submitted (FractalWork backend in MVP)
    receipt_count: u32,
    payout_count: u32,
    receipt_root: Hash,            // merkle root over receipt hashes
    payout_root: Hash,             // merkle root over payout entries
    total_payout: u64,             // sum of all payouts in batch
    submitted_at: u64,
    operator_signature: Ed25519Signature,
    schema_version: u16,
}
```

A batch submission inserts the roots on-chain in one tx. Individual agents then call `CLAIM_PAYOUT` with their Merkle proof to receive funds. This is **rollup-style settlement** and is what makes the chain scale.

### AgentRegistry Native Format

```rust
struct OnChainAgent {
    agent_id: u64,                 // monotonic, assigned on register
    address: Address,              // controlling EOA
    operator: Address,             // Sybil-prevention binding
    pubkey: PublicKey,             // Ed25519 for signing receipts
    kind: AgentKind,               // Worker | Verifier | Both
    metadata_uri: String,          // off-chain JSON (capabilities, etc.)
    reputation_score: u32,         // cached from FractalWork backend; updated by SETTLE_RECEIPT
    completed_jobs: u32,
    status: AgentStatus,           // Active | Suspended | Retired
    registered_at: u64,
    schema_version: u16,
}
```

## 9.3 EVM Layer (revm)

* **revm** (Rust EVM) for performance and easy integration.
* Latest mainline opcode set (Cancun-equivalent + selected post-Cancun).
* **Cross-call**: EVM contracts can call native precompiles via a reserved address range `0xFC00..0xFCFF`. Calling `0xFC04` from Solidity invokes `SETTLE_RECEIPT`.
* This means: **Solidity contracts can read agent reputation, settle receipts, and pay agents** without bridging.

### Example Solidity Usage

```solidity
interface IFractalNative {
    function settleReceipt(bytes calldata receipt, bytes calldata sigs) external returns (bytes32);
    function getAgentReputation(uint64 agentId) external view returns (uint32);
    function claimPayout(bytes32 batchId, address account, uint256 amount, bytes32[] calldata proof) external;
}

contract AgentBountyEscrow {
    IFractalNative constant NATIVE = IFractalNative(0xFC00000000000000000000000000000000000000);

    function payTopAgent(uint64 agentId) external {
        require(NATIVE.getAgentReputation(agentId) >= 80, "Reputation too low");
        // ... pay via FRAC transfer
    }
}
```

EVM contracts get native functionality at native gas costs. This is the key developer-experience unlock.

## 9.4 Gas Market

### Single Token, Two Cost Models

* **FRAC** is both the gas token and the reward token.
* **EVM transactions** use the standard EIP-1559 model (base fee + priority tip).
* **Native transactions** use **fixed-cost pricing** (gas cost is constant per opcode; no contention auction).
* Both models charge in FRAC. The base fee burns; the tip goes to validators.

### Why Fixed-Cost for Native Ops

The native opcodes are the hot path. AI agents settling receipts every few seconds need predictable cost. A surge in EVM activity should not raise the cost of paying an agent.

To prevent native ops from being free-riders, the **block gas limit reserves 50% for native ops** and the other 50% is shared. Native ops cannot starve EVM and vice versa.

### EIP-1559 Parameters (Testnet)

* `target_gas_per_block = 30,000,000`
* `max_gas_per_block = 60,000,000`
* `base_fee_change_denominator = 8` (same as Ethereum)
* `min_base_fee = 1 gwei equivalent` (1 fgwei = 10^9 fwei)

## 9.5 Determinism Requirements

This is the **single most important rule** for the execution layer:

> Given the same parent state and the same ordered transactions, every validator must produce byte-identical state roots.

To enforce this:

1. **No wall-clock time** in execution. `block.timestamp` is the proposer's timestamp, validated within bounds; no validator reads its own clock.
2. **No randomness from OS**. On-chain randomness comes from `block.hash(parent)` and dedicated VRF later.
3. **No floating point.** All math is integer (U256 / u64).
4. **Canonical serialization.** Borsh for native; RLP for EVM (standard).
5. **Bounded iteration.** No unbounded loops; gas metering catches it for EVM, opcode atomicity for native.
6. **Deterministic map iteration.** All native state maps iterate by sorted key.

This is exactly the same constraint as `packages/core` in the off-chain MVP — that pure-function design ports directly to the on-chain native VM.

---

# 10. Storage Layer

## 10.1 Engine

**RocksDB** for state and chain data. Reasons: column families (clean separation of state / blocks / receipts / mempool), excellent write throughput, battle-tested in geth/erigon/aptos.

## 10.2 State Representation

* **Merkle Patricia Trie** for state root (EVM-compatible; lets us reuse explorers).
* Per-subtree roots inside the state root for fast proofs of native objects.
* **Pruning**: keep last 128 blocks of historical state online; archive nodes keep full history.

## 10.3 Column Families

```text
cf_state         — current state trie (key: trie node hash → node)
cf_blocks        — blocks by height and hash
cf_tx_index      — tx hash → (block, index)
cf_receipts      — tx receipts (execution outcomes)
cf_native_events — native event log, indexed for query
cf_mempool       — pending txs (in-memory primary, RocksDB backup)
cf_consensus     — HotStuff-2 state: votes, QCs, view info
cf_snapshots     — periodic state snapshots for fast sync
checkpoint_proofs — (M9 / §7.8) async STWO checkpoint blobs; same DB, separate CF
```

**Implemented in-repo (`fractal-storage`):** `FractalRocksDb` opens **all** of the above column families in a single RocksDB directory (`create_missing_column_families` so existing proof-only paths gain empty PRD CFs). Typed v1 records: **`StoredBlockV1`** (`cf_blocks`), **`StoredTxIndexV1`** (`cf_tx_index`), **`StoredReceiptV1`** (`cf_receipts`), **`StoredStateAtHeightV1`** (`cf_state` — full execution state after height, not yet MPT-wide). `fractal-node` persists on each commit when **`FRACTAL_CHAIN_ROCKSDB_PATH`** is set (must match **`FRACTAL_PROOF_ROCKSDB_PATH`** if both are set). Opaque `put_raw` / `get_raw` remain for other CFs.

## 10.4 Snapshot and Fast Sync

* Every 100,000 blocks (~14 hours at 500 ms), produce a state snapshot.
* New nodes can download a snapshot and verify against the QC chain, then sync forward.
* Snapshot format: streaming Merkle trie serialization.

---

# 11. Native Token: FRAC

## 11.1 Specification

| Property | Value |
|---|---|
| Symbol | FRAC (testnet: tFRAC) |
| Decimals | 18 (smallest unit: fwei) |
| Initial supply (testnet) | 1,000,000,000 tFRAC |
| Initial distribution | Faucet (50%), validator allocation (20%), team treasury (20%), ecosystem fund (10%) |
| Inflation | None on testnet; configurable on mainnet via governance |
| Burn | EVM base fee burns; native ops do not |

## 11.2 Faucet

* Self-serve via web UI and CLI.
* 1,000 tFRAC per address per 24 hours.
* Rate-limited by IP + captcha + GitHub OAuth (to prevent farming).

## 11.3 Use Cases (Testnet)

1. **Gas** for EVM and native transactions.
2. **Job rewards**: requesters pay agents in FRAC (the Core MVP's "internal credits" map 1:1 to on-chain FRAC).
3. **Staking** by validators.
4. **Verifier fees**.
5. **Slashing collateral**.

## 11.4 Bridging FractalWork Core MVP Credits

The Core MVP's internal credits become FRAC at the moment a `PayoutBatch` is settled on-chain. Before settlement, balances are off-chain; after settlement, they're on-chain claimable. This is the rollup model.

A node operator (initially the FractalWork team) periodically submits `SETTLE_BATCH` calls. Agents call `CLAIM_PAYOUT` to materialize their FRAC on-chain.

---

# 12. Staking and Validator Economics

## 12.1 Stake Requirements

| Phase | Min Stake | Notes |
|---|---|---|
| Phase 1 | N/A | Singleton; no staking active |
| Phase 2 | 1,000,000 tFRAC | Permissioned set |
| Phase 3 | 5,000,000 tFRAC | Permissioned at testnet; permissionless at mainnet |

## 12.2 Rewards

* **Block reward**: 10 FRAC per block (paid from protocol treasury on testnet).
* **Priority tips**: 100% to proposer.
* **Burned**: EVM base fee (not on testnet to keep flows simple; flip on at mainnet).

Distributed each block to the proposer + voting validators in proportion to stake-weighted participation.

## 12.3 Delegation

* Phase 3+ supports delegation via the `DELEGATE` opcode.
* Validators set a commission rate.
* Rewards auto-compound by default; opt-out via `WITHDRAW_REWARDS`.

## 12.4 Unbonding

* 7-day unbonding period (testnet); 21 days on mainnet.
* During unbonding, stake is non-transferable and still slashable.

---

# 13. Bridge to FractalWork Core MVP

This is **the central integration** — how the off-chain agent labor system flows on-chain.

## 13.1 Settlement Flow

```text
Off-chain (FractalWork Core MVP):
1. Jobs complete; receipts finalize
2. Backend buffers finalized receipts into a batch
3. Backend computes:
   - receipt_root  = merkle(receipts)
   - payout_root   = merkle(payouts)
4. Backend signs the batch with its operator key
5. Backend submits SETTLE_BATCH transaction

On-chain (FractalChain):
6. Native VM verifies operator signature
7. Stores batch_id → (receipt_root, payout_root, total_payout)
8. Locks `total_payout` FRAC from operator's balance (escrow)
9. Emits BatchSettled event

Agent claims:
10. Agent (or anyone, on agent's behalf) calls CLAIM_PAYOUT
11. Provides Merkle proof of their payout entry
12. Native VM verifies proof, transfers FRAC from escrow to agent
13. Marks claim slot consumed (prevents double-claim)
```

## 13.2 Batch Cadence

* MVP backend submits one batch every 10 minutes, or when 1,000 receipts buffered (whichever first).
* Settlement gas cost per batch (1,000 receipts): ~215,000 native gas.
* At Phase 3 gas prices: well under $0.01 equivalent for the entire batch.

## 13.3 Challenge Period (Phase 3+)

* Batches enter a 24-hour challenge window before claims are enabled.
* Anyone can submit a `ChallengeBatch` tx with a fraud proof (e.g., a receipt with an invalid signature).
* If upheld by governance, the batch is invalidated and the operator slashed.
* In Phase 1-2, this is **disabled** (testnet trust assumption). Hook is present in the code.

## 13.4 Dispute Resolution On-Chain

The off-chain dispute flow (Section 9.9 of the Core MVP PRD) is mirrored on-chain:

* `FILE_DISPUTE` opcode creates an on-chain dispute record.
* Admin (Phase 1-2) or governance (Phase 3+) calls `RESOLVE_DISPUTE`.
* Resolution can issue refunds, slash worker stake, slash verifier stake, or uphold.

The off-chain backend listens for these events and updates its reputation accordingly.

## 13.5 Agent wallet vs L1 — tool-provider stake / slash

Consensus staking and slashing (`§12` / `DepositConsensusStake`, `SlashConsensusStake`, …) are **separate** from the FractalWork **tool market** bond model in `docs/wallet.md` v2. Until native `StakeForClass` / `SlashProvider` (§14.4 of the wallet spec) are scoped on-chain, provider stake for intents is **wallet-enforced only**:

| Concern | Phase 1 slice |
| --- | --- |
| Provider stake / slash on-chain | Off-chain in `fractal-wallet`; not full protocol-enforced staking/slashing tied to tool market |

Implementation slice: `fractal_wallet::market::ProviderStake` locks `slash_multiplier × price` at match; adjudication **challenger wins** forfeits the locked bond (`burn_locked`) and `ToolMarketWithReputation` records a §10.4 `slashing_events` increment for reputation snapshots (`WalletReputationSnapshotV1`).

## 13.6 Reputation indexer (wallet infra)

| Concern | Phase 1 slice |
| --- | --- |
| Reputation indexer | **`fractal-indexer`**: SQLite + GraphQL `reputationRows`; default **merges** `SettleBatch` / `SettleReceipt` into §10.4 rows (`INDEXER_REPUTATION_MERGE_SETTLEMENTS=0` disables). Always ingests **`WalletReputationSnapshotV1`** and dispute-slash signals via `reputation_chain_mirror_json`. **`fractal-indexer-stub`** / **`run-indexer-reputation.sh`** stay snapshot-first for JSON file workflows. |

## 13.7 Agent wallet vs L1 — batch settlement

The wallet spec (`docs/wallet.md` §7 / §16.3) describes **per-intent** settlement with **optimistic challenge windows** in the tool market, and a future **multi-receipt `BatchSettle`** to amortize on-chain cost. That wallet-native batched opcode is **not** the same as L1 **`NativeCall::SettleBatch`**, which batches **FractalWork / M3 `OnChainTaskReceipt`** records and payout roots (PRD **§13.1**, `crates/mvp-backend`, `fractal-mvp-bridge`).

| Concern | Phase 1 slice |
| --- | --- |
| Batch settlement (wallet tool market §7) | **Off-chain** library state in `fractal-wallet` — optimistic windows, per-intent settle; **not** on-chain batched settlement of wallet tool intents per §16.3 |
| Batch settlement (Core MVP / M3) | **On-chain** — **`SettleBatch`** / **`SettleReceipt`** + **`ClaimPayout`** for operator-submitted receipt batches |

---

# 14. RPC and Developer Interface

## 14.1 Endpoints

| Endpoint | Protocol | Purpose |
|---|---|---|
| `https://rpc.testnet.fractalwork.io` | JSON-RPC (EVM-compat) | MetaMask, ethers.js, web3.js |
| `grpc://native.testnet.fractalwork.io:9090` | gRPC | Native ops, streaming events |
| `wss://ws.testnet.fractalwork.io` | WebSocket | Event subscriptions |
| `https://explorer.testnet.fractalwork.io` | Web | Block/tx/agent explorer |
| `https://faucet.testnet.fractalwork.io` | Web | Get tFRAC |

## 14.2 JSON-RPC Methods

**Standard EVM**: `eth_blockNumber`, `eth_getBalance`, `eth_sendRawTransaction`, `eth_call`, `eth_getLogs`, `eth_subscribe`, etc. — full compatibility for tooling.

**FractalChain extensions** (under `fractal_` namespace):

```text
fractal_getAgent(agent_id)                  → Agent
fractal_getAgentByAddress(address)          → Agent
fractal_getReceipt(receipt_id)              → TaskReceipt
fractal_getBatch(batch_id)                  → PayoutBatch
fractal_getPayoutProof(batch_id, address)   → MerkleProof
fractal_getDispute(dispute_id)              → Dispute
fractal_getValidatorSet()                   → ValidatorSet
fractal_subscribeAgentEvents(agent_id)      → stream
```

## 14.3 SDKs (v0.1)

* **TypeScript** SDK (primary) — wraps ethers.js + native gRPC.
* **Rust** SDK — for high-performance agent operators.
* **Python** SDK — for ML/agent researchers.

All three are part of MVP deliverables.

## 14.4 Indexer

* Subgraph-compatible indexer for EVM events.
* Native event indexer with GraphQL frontend.
* Both shipped as Docker images for self-hosting.

---

# 15. Security Model

## 15.1 Threat Model

| Threat | Mitigation |
|---|---|
| Double-spend | HotStuff-2 deterministic finality; no reorgs after commit |
| Validator collusion | f < n/3; permissioned set in Phase 1-2; slashing in Phase 2+ |
| Sybil agent registration | `operator` binding in AgentRegistry; future stake-based |
| Replay attacks | Per-account nonce + chain_id in tx |
| Front-running | Acknowledged; private mempool option in v1.0 |
| Long-range attacks | Weak subjectivity checkpoints every 100K blocks |
| EVM contract bugs | Standard EVM caveat; native ops are not affected |
| Native VM bugs | Heavily audited; formal verification of critical opcodes by v1.0 |
| RPC DoS | Rate limiting, API keys for high-volume, per-IP limits |
| Network partition | View timeouts + eventual GST recovery; standard BFT properties |

## 15.2 Key Management

* **Validator keys** (Ed25519 for BLS aggregation in QC): stored in HSM or KMS in production; secure key file with passphrase for testnet.
* **Agent keys**: Ed25519, same as off-chain MVP. Agents sign on-chain registration with their key.
* **Operator keys** (FractalWork backend submitting batches): rotated quarterly, multi-sig in Phase 2+.

## 15.3 Upgrade Path

* **Soft forks**: opcode gas changes, gas limit changes — handled via on-chain governance vote, no client update required for compatible changes.
* **Hard forks**: scheduled by block height; all validators must upgrade.
* **Emergency halt**: 2f+1 validators can sign a halt message; chain stops until fix deployed.

## 15.4 Audit Plan

* Internal audit before Phase 1 launch.
* External audit (Trail of Bits / OpenZeppelin / Sigma Prime) before Phase 3.
* Bug bounty program from Phase 2 onward.

---

# 16. Observability

## 16.1 Metrics (Prometheus)

Per-node:
```text
fractal_consensus_view_number
fractal_consensus_height
fractal_consensus_view_changes_total
fractal_consensus_proposal_latency_ms
fractal_consensus_qc_formation_latency_ms
fractal_block_size_bytes
fractal_block_tx_count
fractal_block_gas_used
fractal_mempool_size
fractal_p2p_peer_count
fractal_p2p_messages_total{topic, direction}
fractal_state_root_computation_ms
fractal_db_size_bytes
fractal_rpc_requests_total{method, status}
fractal_rpc_latency_ms{method}
```

Chain-wide (computed by indexer):
```text
fractal_agents_total
fractal_active_agents_24h
fractal_receipts_settled_total
fractal_batches_settled_total
fractal_total_fees_paid_frac
fractal_disputes_filed_total
fractal_validators_active
```

**In-repo (dev slice, PRD §18 M6 / ops backlog):** **`fractal-node`** exposes an optional **`GET /metrics`** endpoint when **`FRACTAL_METRICS_ADDR`** is set (see `docs/devnet.md`). OpenMetrics includes consensus height/view gauges; mempool, last-block, p2p peer, DB-size, and proof-worker gauges; RPC request, proof-job, and p2p topic/direction counters; and latency histograms for RPC methods, proposal build, QC formation, and state-root recomputation. Chain-wide/indexer metrics remain separate from the node-local endpoint.

## 16.2 Logs

Structured JSON, OpenTelemetry traces. Same standard as the Core MVP backend.

## 16.3 Alerts

* View-change rate > 1% of blocks → page on-call
* Block time p95 > 1 s → page
* Validator drops below 80% participation → notify
* State root mismatch detected → critical page
* Mempool size > 100K → notify
* Disk space < 20% → notify

---

# 17. Performance Targets

## 17.1 Latency

### Track A (testnet v0.1 — HotStuff-2 monolith)

| Operation | Target (p50) | Target (p99) |
|---|---|---|
| Tx → block inclusion | 250 ms | 800 ms |
| Tx → finality | 500 ms | 1.2 s |
| Native SETTLE_RECEIPT | 200 ms inclusion | 700 ms finality |
| EVM contract call | 300 ms inclusion | 900 ms finality |
| RPC `eth_call` (state read) | 20 ms | 100 ms |
| RPC `fractal_getReceipt` | 15 ms | 80 ms |

### Track B (per shard — HyperBFT target)

| Operation | Target (p50) | Target (p99) |
|---|---|---|
| Tx → block inclusion | **35 ms** | **100 ms** |
| Tx → finality (shard) | **≤ 200 ms** | **≤ 900 ms** |
| Cross-shard agent message (via masterchain anchor) | **~7 s** default | priority fee for faster anchor TBD |
| Proof lag (async, non-blocking) | seconds | minutes under load |

## 17.2 Throughput

### Track A

| Workload | Phase 1 | Phase 3 |
|---|---|---|
| Native ops (settle receipt) | 8,000 TPS | 5,000 TPS |
| EVM ops | 2,000 TPS | 1,500 TPS |
| Mixed | ~5,000 TPS | ~3,500 TPS |

(Phase 3 lower because of BFT message overhead at 21 nodes.)

### Track B (design budget)

| Workload | Per shard (target) | Aggregate (10 shards, illustrative) |
|---|---|---|
| Native + EVM mixed | ~20,000 TPS | **~200,000 TPS** |
| Masterchain anchors | N/A (coordination only) | bounded by anchor interval, not tx execution |

Exact per-shard numbers depend on hardware, shard count, and optimistic execution rollback rate — to be validated in M10 load tests.

## 17.3 Resource Footprint

Validator node (Phase 3, sustained load):
```text
CPU:      8 vCPU (32-core helpful for batch verification)
RAM:      32 GB
Storage:  1 TB NVMe SSD (Track A: ~50 GB/month at load; Track B: shard data pruned after validity proof — §6.2.3)
Network:  200 Mbps symmetric minimum
```

Full node: half the above.

Light client: laptop-grade hardware.

---

# 18. Implementation Milestones

## M1: Core Crate Layout and Determinism Spike (Weeks 1-3)

Deliverables:
* Workspace skeleton in Rust (Cargo workspace, ~10 crates)
* `core`: pure-function state machine, native opcodes (mocked), unified state trie
* `crypto`: Ed25519, BLS aggregation, SHA-256, canonical encoding
* Determinism test harness (fuzz: random ordered txs → identical state roots across 10 runs)

Exit criteria: state machine processes a 10,000-tx test vector and produces identical state root on every run.

## M2: Singleton Block Producer (Weeks 4-6)

Deliverables:
* HotStuff-2 implementation in `consensus` crate (validates with n=1, f=0)
* P2P via libp2p+QUIC
* Mempool with EIP-1559 gas market
* JSON-RPC subset (`eth_blockNumber`, `eth_sendRawTransaction`, `eth_call`, `eth_getBalance`)
* Block production at 500 ms target

Exit criteria: a single node produces blocks; a second node can sync from it; transactions execute and finalize.

## M3: Native VM Opcodes (Weeks 5-8, overlaps M2)

Deliverables:
* All 13 v0.1 opcodes implemented
* AgentRegistry, TaskReceipt, PayoutBatch, Dispute, Stake state subtries
* Cross-call from EVM to native at `0xFC00..0xFCFF` precompile range
* Fixed-cost gas accounting

Exit criteria: settle 100 receipts via `SETTLE_BATCH`, agents claim payouts via Merkle proofs, all on-chain.

## M4: EVM Integration (Weeks 7-9)

Deliverables:
* revm integration
* Full EVM JSON-RPC compatibility (MetaMask connects, ethers.js works)
* Example contract: AgentBountyEscrow using native precompiles (`contracts/examples/AgentBountyEscrow.sol` + `FractalNative.sol`)
* Solidity dev docs (`docs/solidity-dev.md`)

Exit criteria: deploy a contract via Hardhat; call native precompiles from it; MetaMask shows tFRAC balances.

## M5: Bridge to Core MVP (Weeks 9-11)

Deliverables:
* Off-chain Core MVP backend submits real `SETTLE_BATCH` calls (**reference bridge** binary: `cargo run -p fractal-mvp-backend --bin fractal-mvp-bridge`; optional `MVP_RECEIPTS_JSON` for a JSON receipt export — see `crates/mvp-backend/testdata/mvp_receipts_sample.json`; post-run logs include `eth_getBalance` for the claim agent). PRD-scale smoke: **`./scripts/run-mvp-bridge-smoke.sh`** (default `MVP_RECEIPT_COUNT=100`); optional GitHub Actions workflow source **`docs/ci/mvp-bridge-smoke.workflow.yml`** (`docs/devnet.md`).
* Agents claim payouts via SDK (`fractal_sdk::m5` in `crates/sdk-rust`)
* End-to-end: post job off-chain → settle batch on-chain → agent claims tFRAC → tFRAC appears in MetaMask

Exit criteria: ≥ 100 receipts flow from off-chain MVP to on-chain settlement to agent claims, with no manual intervention.

## M6: Explorer, Faucet, Public Testnet (Weeks 10-12)

Deliverables:
* Block explorer — **FractalScan** static Blockscout-style UI: `tools/explorer/` (`./scripts/serve-explorer.sh`), chain + block window + tx/receipt/logs + search; **`docs/explorer.md`** (Ethereum vs internal tx hash, leader vs follower RPC); `docs/devnet.md`. Forked Blockscout / custom Next.js remains an optional ops path for contract verify + DB-backed indexing.
* Faucet with rate limiting — `fractal-faucet` (`crates/faucet`) + Docker service in `testnets/devnet/docker-compose.yml`
* Public bootnodes (3 minimum) — template `testnets/devnet/bootnodes.example.txt` + `FRACTAL_BOOTSTRAP` (see `testnets/devnet/README.md`)
* **Local two-node Docker devnet (optional):** `docker compose -f testnets/devnet/docker-compose.yml --profile follower up` — producer uses **`FRACTAL_P2P_DOCKER_FIXTURE=producer`** for a stable libp2p **`PeerId`**; follower uses **`FRACTAL_BOOTSTRAP`** to **`node:4001`** with the same **`/p2p/<PeerId>`** on every line; operator keys still win via **`FRACTAL_P2P_IDENTITY_PATH`** (`docs/devnet.md`, `crates/node/src/p2p.rs`). **Compose health:** **`node`** / **`follower`** JSON-RPC probes (`eth_chainId` via `curl` in-image), **`faucet`** `GET /health` in **`Dockerfile.faucet`**; **`depends_on: service_healthy`** defers follower + faucet until producer RPC answers.
* Documentation site — operator notes: `docs/devnet.md` (full product site TBD); **Prometheus:** optional **`FRACTAL_METRICS_ADDR`** on **`fractal-node`** for **`GET /metrics`** (PRD §16.1 dev slice, `docs/devnet.md`).
* Discord + status page — out of repo (operational); **`tools/status/`** + `./scripts/serve-status.sh` for a minimal JSON-RPC liveness stub; full public status TBD

Exit criteria: external developer can connect MetaMask, get tFRAC, deploy a contract, call native precompiles, and see everything in the explorer — without team help.

## M7: BFT-7 (Months 4-5)

Deliverables:
* Full HotStuff-2 with 7 validators
* BLS aggregation working end-to-end
* Validator onboarding flow — **dev fixtures:** `cargo run -p fractal-node -- print-devnet-validator-keys` (honors `FRACTAL_VALIDATOR_SET`; see `docs/devnet.md` §Validator onboarding). On-chain / governance-gated onboarding remains future work.
* View-change / liveness — **partial (M7-f):** quorum BLS `Timeout` on gossip `/fractalchain/timeouts/1.0.0`; each timeout signs `(view, high_qc: QuorumCertificate)`; pool keyed by `(view, hash_qc(high_qc))`; `NodeInner.high_prepare_qc` tracks max `(block_height, view)` from parent QCs and formed tip QCs; pacemaker + `try_advance_view_on_timeout_quorum` in `try_produce_one_tick` (PRD §7.4). **Not yet:** merging conflicting high-QCs across partitions, partition torture tests, full HotStuff-2 safety proofs.
* Phase 2 staking + basic slashing — **partial (M7-g):** native **`DepositConsensusStake`** / **`WithdrawConsensusStake`** (unbonding queue + **`FRACTAL_UNBONDING_PERIOD_MS`** finalize payouts) / **`CommitSlashingEvidence`** + **`SlashConsensusStake`** (evidence hash must be committed first); `State.consensus_stakes` / shares / `consensus_unbonding`; optional producer gate **`FRACTAL_MIN_CONSENSUS_STAKE_WEI`** + **`ProduceTickOutcome::AwaitingConsensusStake`**. **Implemented:** **`FRACTAL_BLOCK_REWARD_WEI`** treasury-funded block rewards into consensus stake (proposer + parent-QC signers); stake-weighted QC when total bonded stake > 0 (`docs/devnet.md` §Consensus stake). **§12.3 delegation:** **`Delegate`** / **`WithdrawRewards`** on **32-byte validator fingerprints**; **`SetValidatorCommission`**; commission + pro-rata reward compounding in **`finalize_block_hooks`**. **Mainnet economics (dev slice):** **`ChainEconomicsParams`** on state; **`FRACTAL_ECONOMICS_PROFILE=mainnet`**; permissionless **`RegisterValidator`** + dynamic validator set sync; **`Redelegate`**; EVM base-fee burn in finalize; governance **`SetChainEconomics`**. **Not yet:** full misbehavior verification pipeline (hashes are governance-committed placeholders).

Exit criteria: 7-validator testnet runs for 30 days at sustained load; tolerates 2 Byzantine validators in test scenarios.

## M8: BFT-21 + Production Hardening (Months 6-8)

Deliverables:
* 21-validator set — **dev slice:** in-repo **`ValidatorSet::phase3_bft21_fixture()`** + env **`FRACTAL_VALIDATOR_SET=21`** or **`bft21`** (same pattern as M7 BFT-7); deterministic BLS keys via **`print-devnet-validator-keys`**. Operating 21 networked processes + liveness at this scale remains an ops exercise (full mesh, vote gossip).
* Full slashing including delegation
* Snapshot-based fast sync — **v2 + proof-chain path implemented:** `GetSnapshot` serves **`ChainSyncSnapshotV2`** over QUIC by default. v2 chunks the state payload, verifies per-chunk hashes plus `stateRoot` and EVM account MPT root, persists the verified state row and EVM account MPT root/nodes into **`cf_state`**, and stores the v2 manifest/chunks in **`cf_snapshots`**. With **`FRACTAL_PROOF_CHAIN_FAST_SYNC=1`**, peers prefer **`ChainSyncProofSnapshotV1`**: verified state chunks + one checkpoint tip block + masterchain proof chain + Plonky2 bundle, so a pruned node can join without carrying or replaying the full raw execution block vector. Legacy trusted **`ChainSyncSnapshotV1`** remains accepted as a fallback; set **`FRACTAL_FAST_SYNC=0`** for block-by-block replay only.
* External audit complete
* Bug bounty live

Exit criteria: 21-validator testnet stable for 60 days; audit findings resolved; ready for mainnet planning.

## M9: STWO + RISC-V async proof condenser — tier 1 only (post–testnet)

**Intent:** Ship **tier 1** of §6.2.2 / §7.8: **STWO proof condensing** over RISC-V-backed witnesses on **spare node compute**, committed asynchronously without blocking HotStuff-2’s 500 ms path. **Plonky2 (tier 2) is out of scope for M9** — see M11.

**Reference platform (today):** **x86_64** (Linux CI and developer hosts) is the version of the stack you run and test first; see §7.8 *x86_64 reference port* and `docs/stwo-run-notes.md`. RISC-V remains the **trace ISA** target for provable execution; riscv64 bring-up is incremental once the reference port stays green.

**Implemented in repo (reference port, evolving):**

* Async condenser + `FRACTAL_ASYNC_PROOF` toggle (`docs/devnet.md`); `prove_checkpoint` + STWO with fallback digest.
* `prove_checkpoint_stwo` / `verify_checkpoint_stwo_proof_json(json, job)` / `checkpoint_stwo_digest_from_json` — prove, verify-only (FS-bound to **borsh(job)**), and JSON digest handle.
* RISC-V replay trace harness — `riscv_trace_from_blocks` emits canonical `BeginBlock` / `ApplyTx` / `EndBlock` rows from finalized block ranges, and `CheckpointJob` binds `riscvTraceRoot` + `riscvTraceSteps` into STWO public inputs.
* `CheckpointStwoArtifactV1` — versioned **borsh** wire bundle (job + `serde_json` STARK proof) with `verify(digest)` for operators and future gossip/sidecar plumbing.
* **`FRACTAL_PROOF_ROCKSDB_PATH`** + `fractal-storage` RocksDB CF `checkpoint_proofs`; optional **`FRACTAL_PROOF_ARTIFACT_DIR`** flat files; JSON-RPC **`fractal_getCheckpointProof`** / **`fractal_getCheckpointProofDigest`** (`docs/devnet.md`).

**Non-binding exit criteria (to be refined when prioritized):**

* Verifier path (full node + optional light client) can **check a proof** against a published checkpoint faster than replaying the covered span.
* Proof workers can be **enabled/disabled** per operator without affecting consensus participation rules on testnet.
* Load tests show **no regression** in block production latency when proof jobs saturate background cores.

## M10: Shard execution + HyperBFT (Track B — post–testnet)

**Depends on:** M8 stable monolith (Track A).

Deliverables:

* `ShardId` routing in mempool and RPC gateway (§6.4).
* HyperBFT pipelined consensus crate wired to **one** pilot shard (shard 0 = migrated monolith state).
* 70 ms block time tuning on lab network; deterministic ordering tests for conflicting agent txs (§7.9.4).
* Per-shard RocksDB / `cf_*` keys namespaced by `shard_id`.

Exit criteria: two shard processes on lab net finalize independently; agent txs on shard A never reorder post-proposal; rollback test on invalid proposal.

## M11: Masterchain + STWO provers + Plonky2 aggregator (Track B)

**Depends on:** M10 pilot shard.

Deliverables:

* Masterchain node binary (`fractal-masterchain`, RPC **8550**): HotStuff-style BFT over coordination blocks; shards post `fractal_submitShardAnchor` via `FRACTAL_MASTERCHAIN_RPC`.
* **Tier 1:** permissionless shard provers submit `ProofSubmission` with **STWO** `StarkProof` (extend `fractal-proof-condenser` from checkpoint-scale to block-range witnesses).
* **Tier 2:** **Plonky2** aggregator crate verifies shard STARK statements and posts recursive **SNARK** → `globalZkRoot` (§6.2.2, §7.8).
* Treasury payout curve for both tiers (§6.2.4); faster lag → higher reward.
* Pruning policy behind `PRUNE_AFTER_VALIDITY_PROOF` (§6.2.3) — env `FRACTAL_PRUNE_AFTER_VALIDITY_PROOF` on `fractal-node` (drops proved checkpoint blobs + old in-memory blocks).
* Light-client sync: masterchain head + Plonky2 proof chain (no full block replay) — RPC `fractal_getLightClientHead`.

Exit criteria: shard prover STWO-proves block range → Plonky2 aggregator posts SNARK → masterchain accepts → old shard blocks pruned on test nodes; new node syncs via proof chain faster than full replay for 1M blocks (benchmark TBD).

---

# 19. Repo Structure

```text
fractalchain/
  crates/
    core/              # pure state machine, native opcodes
    crypto/            # Ed25519, BLS, hashing, canonical encoding
    consensus/         # HotStuff-2 (Track A); HyperBFT shard target (Track B, M10+)
    network/           # libp2p + QUIC + gossipsub
    evm/               # revm integration + precompile bridge
    storage/           # RocksDB column families, MPT
    mempool/           # tx pool + EIP-1559
    rpc/               # JSON-RPC + gRPC + WebSocket
    node/              # main binary (validator / full node / future masterchain role)
    proof-condenser/   # tier 1: async STWO + RISC-V condenser (§7.8 M9 → M10 shard proofs)
    proof-aggregator/  # tier 2: Plonky2 recursive SNARK → globalZkRoot (M11)
    masterchain/       # dedicated BFT coordinator binary (M11)
    cli/               # operator CLI (start, status, stake, etc.)
    sdk-rust/          # Rust SDK
    faucet/            # devnet HTTP faucet (M6)

  sdks/
    typescript/        # TypeScript SDK
    python/            # Python SDK

  contracts/
    governance/        # on-chain governance contracts (Phase 3)
    examples/          # AgentBountyEscrow, etc.

  tools/
    explorer/          # static JSON-RPC explorer (M6; serve over HTTP)
    indexer/           # native event indexer with GraphQL

  testnets/
    devnet/            # local docker-compose (node + faucet; optional --profile follower)
    alphanet/          # 3-node configuration
    testnet/           # public 21-node configuration

  docs/
    prd.md
    devnet.md          # M6 operator notes (faucet, explorer, compose)
    architecture.md
    consensus.md
    native-vm.md
    rpc-reference.md
    sdk-guide.md
    runbook.md
    security.md

  audits/              # public audit reports
```

---

# 20. Open Decisions

Resolve before Phase 2 (Track A) and before M10 (Track B).

| # | Decision | Recommendation |
|---|---|---|
| 1 | Chain ID for testnet | 41 (proposed); register on chainlist.org |
| 2 | Address format | EVM-style `0x...` (20 bytes) — for tooling compatibility |
| 3 | Mainnet token decimals | 18 (consistent with EVM ecosystem) |
| 4 | Validator selection at Phase 3 | Permissioned testnet; permissionless on mainnet (stake threshold) |
| 5 | Governance model | Off-chain Snapshot for testnet; on-chain Compound-style for mainnet |
| 6 | Fast-sync trust model | Genesis hash + checkpoint hashes from 2f+1 validators (Track A); **proof chain** from masterchain (Track B) |
| 7 | Backwards-compatibility window | 6-month soft-fork window; hard-fork requires 30-day notice |
| 8 | Mempool privacy | Public for testnet; encrypted mempool deferred to v1.1 |
| 9 | Cross-chain story | Native bridge to Sepolia for testnet liquidity tests only |
| 10 | Account abstraction (4337) | Deferred to v1.0 mainnet |
| 11 | **Shard count (Track B)** | Start with **10** shards in design docs; pilot **2** in M10 |
| 12 | **Shard assignment key** | `keccak256(agent_id) mod N` vs explicit `home_shard` in `AgentRegistry` |
| 13 | **Masterchain validator set** | Same as union of shard validators vs dedicated smaller committee |
| 14 | **STARK → SNARK aggregator** | **Plonky2** recursive SNARK (tier 2 default); permissionless aggregator race; tier 1 remains STWO shard race |
| 15 | **Plonky2 circuit versioning** | Pin circuit hash in `globalZkRoot` metadata; upgrade via governance + proof version byte |
| 16 | **ANCHOR_INTERVAL** | Default **100** shard blocks; priority anchor fee curve TBD |
| 17 | **Proof incentive curve** | `reward = f(range, lagSeconds)` — exact formula in economics spec |
| 18 | **HyperBFT vs HotStuff-2 naming** | User-facing: “HyperBFT”; codebase may retain `Vote`/`QC` types during migration |

---

# 21. First Demo Script (Phase 1)

```text
1. Operator starts singleton node; chain produces blocks at 500 ms.
2. Developer connects MetaMask to rpc.testnet.fractalwork.io.
3. Developer requests tFRAC from faucet → balance appears in MetaMask in ~1 second.
4. Developer deploys an EVM contract via Hardhat → confirmed in 1 block.
5. Developer registers as a worker agent (calls REGISTER_AGENT via SDK).
   → Agent appears in explorer; agent_id assigned.
6. Off-chain Core MVP backend:
   a. Has 50 finalized receipts buffered.
   b. Computes receipt_root + payout_root.
   c. Submits SETTLE_BATCH → 1 block (500 ms).
7. Explorer shows BatchSettled event with 50 receipts.
8. Developer's agent calls CLAIM_PAYOUT with Merkle proof from SDK.
   → tFRAC arrives in MetaMask in 1 second.
9. Developer transfers tFRAC to another address → confirmed in 500 ms.
10. Replay test: snapshot at block N, replay txs N..N+1000 on a fresh node,
    state roots match byte-for-byte.
```

---

# 22. Success Criteria for Testnet v0.1

The testnet is successful if:

1. Singleton phase ships in ≤ 90 days.
2. 99.5% uptime over the first 30 days of public testnet.
3. ≥ 1,000 external developer wallets receive faucet tFRAC.
4. ≥ 100 EVM contracts deployed.
5. ≥ 10,000 native receipts settled end-to-end from Core MVP.
6. Median block time within 10% of 500 ms target.
7. Zero state-root divergences between nodes.
8. Zero double-spends.
9. Successful migration from 1 → 7 validators without hard fork.
10. External developer can build an agent that earns tFRAC end-to-end using only public docs.

---

# 23. Why This Is the AI-Agent-First Chain

Every chain claims to be "AI-friendly" by accepting LLM-signed transactions. That's not enough.

FractalChain is structurally different:

| General-purpose L1 | FractalChain |
|---|---|
| Agents are wrappers around EOAs | Agents are first-class on-chain objects |
| Receipt settlement = 200-line Solidity | Receipt settlement = 1 opcode |
| Reputation lives off-chain | Reputation is queryable on-chain in O(1) |
| Sybil resistance = "good luck" | Operator binding + future staking |
| Gas costs variable, surge-prone | Native ops have fixed cost |
| Finality: 12s to minutes | Finality: **≤200ms** per shard (Track B), deterministic |
| Batch settlement = custom L2 | Batch settlement = built-in opcode |
| Single-chain throughput ceiling | **Sharded** execution + masterchain coordination (Track B) |
| History = replay all blocks forever | **ZK-pruned** O(log n) proof sync (§6.2.3) |
| LLM agents bridge to TradFi-style chains | LLM agents are the primary workload |

When 100,000 agents are working, bidding, verifying, and settling thousands of receipts per minute — FractalChain is the chain built for that exact workload, and built from the assumption that humans will be the minority of users.

That's the bet.

---

**End of FractalChain L1 PRD v0.2**
