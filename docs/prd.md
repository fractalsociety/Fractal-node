# FractalChain L1 — Testnet PRD

## Product Requirements Document

**Product:** FractalChain L1 (the AI-agent-first blockchain)
**Version:** v0.1 (Testnet)
**Status:** Draft for Implementation
**Companion to:** FractalWork Core MVP PRD v0.2
**Primary Goal:** Ship a fast, low-cost L1 testnet that settles FractalWork TaskReceipts on-chain, pays agents in testnet FRAC, and grows from 1 block producer to a 21-node HotStuff-2 BFT validator set without a hard fork.

---

# 1. Executive Summary

FractalChain is a purpose-built Layer 1 designed around a single workload: **AI agents performing economic work, getting verified, and getting paid — at machine speed**.

Unlike general-purpose chains that bolt on AI as an afterthought, FractalChain's architecture treats the FractalWork primitives — `TaskReceipt`, `PayoutBatch`, `AgentRegistry`, `Verification` — as **first-class on-chain objects with dedicated precompiles**. General-purpose smart contracts (governance, DeFi, prediction markets on agent outcomes) run on a colocated EVM.

The chain ships in three phases without breaking changes:

```text
Phase 1 (Singleton):    1 block producer, BFT-ready code path, public testnet
Phase 2 (BFT-7):        7-validator HotStuff-2, dynamic validator set
Phase 3 (BFT-21):       21-validator production target, full slashing
```

### Design Targets (Testnet)

| Metric | Target |
|---|---|
| Block time | 500 ms (sub-second) |
| Finality | 1 block (HotStuff-2 deterministic) |
| Throughput | 5,000 TPS native ops / 1,500 TPS EVM at testnet |
| TaskReceipt settlement cost | < $0.001 equivalent |
| Native FractalCore op cost | ~10× cheaper than EVM equivalent |
| Validator hardware | 8 vCPU / 32 GB RAM / NVMe SSD |

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
2. Provide a STARK/proof hook for future state validity proofs.
3. Enable prediction markets on agent outcomes via EVM contracts.

---

# 5. Non-Goals

The testnet **will not** include:

* Mainnet token launch with real-money value
* Cross-chain bridges to mainnets
* Permissionless validator entry on day one
* On-chain LLM inference
* On-chain artifact storage (artifacts remain in S3/IPFS; only hashes on-chain)
* Full ZK proof system (proof hook only)
* Complex on-chain governance (off-chain Snapshot-style for testnet)
* Account abstraction (EIP-4337) for v0.1
* Privacy features (mixers, stealth addresses)

---

# 6. High-Level Architecture

```text
┌──────────────────────────────────────────────────────────────────┐
│                      FractalWork Core MVP                        │
│  (off-chain backend: jobs, bids, work, verification, receipts)   │
└─────────────────────────────────┬────────────────────────────────┘
                                  │
                                  ▼ PayoutBatch (signed)
┌──────────────────────────────────────────────────────────────────┐
│                          FractalChain L1                         │
│                                                                  │
│  ┌────────────────────────────────────────────────────────────┐  │
│  │   Consensus: HotStuff-2 (Phase 1 = singleton, 2 = 7, 3 = 21)│  │
│  └────────────────────────────────────────────────────────────┘  │
│  ┌────────────────────────────────────────────────────────────┐  │
│  │              P2P Networking (libp2p + QUIC)                │  │
│  └────────────────────────────────────────────────────────────┘  │
│  ┌────────────────────────────────────────────────────────────┐  │
│  │                       Execution Layer                      │  │
│  │  ┌──────────────────────────┐  ┌─────────────────────────┐ │  │
│  │  │  Native VM (Rust)        │  │  EVM (revm)             │ │  │
│  │  │  • TaskReceipt           │  │  • Solidity contracts   │ │  │
│  │  │  • PayoutBatch           │  │  • Governance           │ │  │
│  │  │  • AgentRegistry         │  │  • Prediction markets   │ │  │
│  │  │  • Verification          │  │  • DeFi (later)         │ │  │
│  │  │  • Dispute               │  │                         │ │  │
│  │  └──────────┬───────────────┘  └─────────┬───────────────┘ │  │
│  │             │                            │                 │  │
│  │             ▼                            ▼                 │  │
│  │       ┌─────────────────────────────────────────┐          │  │
│  │       │   Unified State Trie (Merkle Patricia)  │          │  │
│  │       │   accounts | code | storage | native    │          │  │
│  │       └─────────────────────────────────────────┘          │  │
│  └────────────────────────────────────────────────────────────┘  │
│  ┌────────────────────────────────────────────────────────────┐  │
│  │              Storage Layer (RocksDB / Sled)                │  │
│  └────────────────────────────────────────────────────────────┘  │
│  ┌────────────────────────────────────────────────────────────┐  │
│  │     RPC: JSON-RPC (EVM-compat) + Native gRPC + WebSocket   │  │
│  └────────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────────┘
```

Five layers, top to bottom: consensus, networking, execution (Native VM + EVM sharing one state trie), storage, RPC. The split between native and EVM is purely about gas economics and developer ergonomics — both execution engines write to the **same state trie**, so cross-calls are cheap.

---

# 7. Consensus Layer

## 7.1 Algorithm Choice: HotStuff-2

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

---

# 8. Networking Layer

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
```

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

| Operation | Target (p50) | Target (p99) |
|---|---|---|
| Tx → block inclusion | 250 ms | 800 ms |
| Tx → finality | 500 ms | 1.2 s |
| Native SETTLE_RECEIPT | 200 ms inclusion | 700 ms finality |
| EVM contract call | 300 ms inclusion | 900 ms finality |
| RPC `eth_call` (state read) | 20 ms | 100 ms |
| RPC `fractal_getReceipt` | 15 ms | 80 ms |

## 17.2 Throughput

| Workload | Phase 1 | Phase 3 |
|---|---|---|
| Native ops (settle receipt) | 8,000 TPS | 5,000 TPS |
| EVM ops | 2,000 TPS | 1,500 TPS |
| Mixed | ~5,000 TPS | ~3,500 TPS |

(Phase 3 lower because of BFT message overhead at 21 nodes.)

## 17.3 Resource Footprint

Validator node (Phase 3, sustained load):
```text
CPU:      8 vCPU (32-core helpful for batch verification)
RAM:      32 GB
Storage:  1 TB NVMe SSD (chain data grows ~50 GB/month at load)
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
* Off-chain Core MVP backend submits real `SETTLE_BATCH` calls
* Agents claim payouts via SDK
* End-to-end: post job off-chain → settle batch on-chain → agent claims tFRAC → tFRAC appears in MetaMask

Exit criteria: ≥ 100 receipts flow from off-chain MVP to on-chain settlement to agent claims, with no manual intervention.

## M6: Explorer, Faucet, Public Testnet (Weeks 10-12)

Deliverables:
* Block explorer (forked Blockscout or custom Next.js)
* Faucet with rate limiting
* Public bootnodes (3 minimum)
* Documentation site
* Discord + status page

Exit criteria: external developer can connect MetaMask, get tFRAC, deploy a contract, call native precompiles, and see everything in the explorer — without team help.

## M7: BFT-7 (Months 4-5)

Deliverables:
* Full HotStuff-2 with 7 validators
* BLS aggregation working end-to-end
* Validator onboarding flow
* View-change tested under partition and Byzantine validators
* Phase 2 staking + basic slashing

Exit criteria: 7-validator testnet runs for 30 days at sustained load; tolerates 2 Byzantine validators in test scenarios.

## M8: BFT-21 + Production Hardening (Months 6-8)

Deliverables:
* 21-validator set
* Full slashing including delegation
* Snapshot-based fast sync
* External audit complete
* Bug bounty live

Exit criteria: 21-validator testnet stable for 60 days; audit findings resolved; ready for mainnet planning.

---

# 19. Repo Structure

```text
fractalchain/
  crates/
    core/              # pure state machine, native opcodes
    crypto/            # Ed25519, BLS, hashing, canonical encoding
    consensus/         # HotStuff-2 implementation
    network/           # libp2p + QUIC + gossipsub
    evm/               # revm integration + precompile bridge
    storage/           # RocksDB column families, MPT
    mempool/           # tx pool + EIP-1559
    rpc/               # JSON-RPC + gRPC + WebSocket
    node/              # main binary, assembles all crates
    cli/               # operator CLI (start, status, stake, etc.)
    sdk-rust/          # Rust SDK

  sdks/
    typescript/        # TypeScript SDK
    python/            # Python SDK

  contracts/
    governance/        # on-chain governance contracts (Phase 3)
    examples/          # AgentBountyEscrow, etc.

  tools/
    explorer/          # Next.js block explorer
    faucet/            # faucet web app
    indexer/           # native event indexer with GraphQL

  testnets/
    devnet/            # local docker-compose, 1 node
    alphanet/          # 3-node configuration
    testnet/           # public 21-node configuration

  docs/
    prd.md
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

Resolve before Phase 2.

| # | Decision | Recommendation |
|---|---|---|
| 1 | Chain ID for testnet | 41 (proposed); register on chainlist.org |
| 2 | Address format | EVM-style `0x...` (20 bytes) — for tooling compatibility |
| 3 | Mainnet token decimals | 18 (consistent with EVM ecosystem) |
| 4 | Validator selection at Phase 3 | Permissioned testnet; permissionless on mainnet (stake threshold) |
| 5 | Governance model | Off-chain Snapshot for testnet; on-chain Compound-style for mainnet |
| 6 | Fast-sync trust model | Genesis hash + checkpoint hashes from 2f+1 validators |
| 7 | Backwards-compatibility window | 6-month soft-fork window; hard-fork requires 30-day notice |
| 8 | Mempool privacy | Public for testnet; encrypted mempool deferred to v1.1 |
| 9 | Cross-chain story | Native bridge to Sepolia for testnet liquidity tests only |
| 10 | Account abstraction (4337) | Deferred to v1.0 mainnet |

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
| Finality: 12s to minutes | Finality: 500ms, deterministic |
| Batch settlement = custom L2 | Batch settlement = built-in opcode |
| LLM agents bridge to TradFi-style chains | LLM agents are the primary workload |

When 100,000 agents are working, bidding, verifying, and settling thousands of receipts per minute — FractalChain is the chain built for that exact workload, and built from the assumption that humans will be the minority of users.

That's the bet.

---

**End of FractalChain L1 PRD v0.1**
