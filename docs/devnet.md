# Devnet & M6 operator notes

This page ties together **PRD §18 M6** pieces in-repo (anchored on **`docs/prd.md`**, not `docs/wallet.md`).

## JSON-RPC

- Default local bind: `http://127.0.0.1:8545` (`FRACTAL_RPC_ADDR`).
- **CORS** is enabled on the HTTP JSON-RPC server so browser tools (e.g. `tools/explorer`) can call `eth_*` methods cross-origin during development.
- **MetaMask:** add a custom network named `FractalChain Devnet` with RPC URL `http://127.0.0.1:8545`, chain id `41`, and currency symbol `tFRAC`. MetaMask-created accounts are standard secp256k1 addresses; fund one through `fractal-faucet` or by importing the prefunded Hardhat dev account. See `docs/solidity-dev.md`.

## Prometheus metrics (`FRACTAL_METRICS_ADDR`, PRD §16.1)

- Optional second HTTP listener: set **`FRACTAL_METRICS_ADDR`** to a socket (e.g. **`127.0.0.1:9090`**) on **`fractal-node`** (`run_dev` / `run_follower`). Scrapers use **`GET /metrics`** (OpenMetrics text).
- Exported gauges: **`fractal_consensus_height`**, **`fractal_consensus_view_number`**, **`fractal_mempool_size`** (plus legacy **`fractal_mempool_transactions`**), **`fractal_last_block_gas_used`**, **`fractal_last_block_tx_count`**, **`fractal_p2p_peer_count`** (established QUIC connections, best-effort), **`fractal_db_size_bytes`**, **`fractal_proof_worker_enabled`**, and **`fractal_proof_artifacts_cached`**. **Counters:** **`fractal_rpc_requests_total{method,status}`** (`status` = `ok` or `err`), **`fractal_p2p_messages_total{topic,direction}`**, **`fractal_proof_jobs_enqueued_total`**, and **`fractal_proof_jobs_dropped_total`** since process start. **Histograms:** **`fractal_rpc_latency_ms{method}`**, **`fractal_consensus_proposal_latency_ms`**, **`fractal_consensus_qc_formation_latency_ms`**, and **`fractal_state_root_computation_ms`**.

## Validator set (PRD §7 M7-b)

- **`fractal-node`** reads **`FRACTAL_VALIDATOR_SET`** when started via **`run_dev`** / **`run_follower`** (not when using `NodeInner::devnet()` in unit tests).
- Values: unset or anything other than `7` / `bft7` / `21` / `bft21` → Phase-1 **singleton** (`n = 1`). `7` or `bft7` → in-repo **BFT-7 fixture** (`ValidatorSet::phase2_bft7_fixture()`). `21` or `bft21` → **BFT-21 fixture** (`ValidatorSet::phase3_bft21_fixture()`, PRD **M8**): `n = 21`, PBFT quorum **13** votes. Block header **`proposer`** rotates with the HotStuff **view** (`fractal_consensus::ValidatorSet::expected_proposer`).
- Producer and follower **must use the same** `FRACTAL_VALIDATOR_SET` so synced blocks pass `InvalidProposer` checks.
- **`FRACTAL_VALIDATOR_INDEX`:** which validator this process is (`0 .. validator_set_size-1`). Default **`0`**. If the value is **≥** the set size, the node logs and **clamps to `0`** so a stale env var cannot disable a singleton devnet. The producer **only builds a block when this index is the leader for the current view** (`NotMyTurn` ticks otherwise); see `fractal_node::try_produce_one_tick`.
- **`FRACTAL_VALIDATOR_SECRET_HEX`:** optional **32-byte** BLS secret ( **64** hex digits, optional `0x` prefix). If set, it must decode to a valid **blst** secret and **should** match `validators.bls_pubkey(FRACTAL_VALIDATOR_INDEX)` or peers will reject your votes (the node logs a pubkey mismatch warning). If unset or malformed, the node uses the **deterministic dev fallback** secret for `(validator_set, index)` when one exists (`run_dev` / `run_follower`).

## Consensus stake (PRD §12 / M7-g)

- **Fingerprints:** bonded stake is keyed by the same **32-byte** value as `BlockHeader.proposer` / `ValidatorSet::entry(i).fingerprint` (see **`print-devnet-validator-keys`**).
- **Opcodes:** `fractal_core::NativeCall::{DepositConsensusStake, WithdrawConsensusStake, CommitSlashingEvidence, SlashConsensusStake, SlashConsensusStakeVerified}` — deposit moves FRAC from the signer into `State.consensus_stakes` (+ per-signer shares). **`WithdrawConsensusStake`** queues **`consensus_unbonding`** (no immediate liquid payout); **`finalize_block_hooks`** (run inside **`execute_and_build_block`** on the producer) anchors **`release_ms`** then pays matured entries. **`CommitSlashingEvidence`** (governance) registers a hash; **`SlashConsensusStake`** consumes that hash and burns stake. **`SlashConsensusStakeVerified`** is **permissionless**: `evidence_borsh` must deserialize to **`fractal_bft_wire::ConsensusMisbehaviorEvidenceV1`** (double-vote, conflicting-QC, or timeout equivocation), verify against the active permissionless **`validator_registry`** set and stake weights, then record `keccak256(borsh(evidence))` in **`applied_misbehavior_evidence_hashes`** (replay protection) before burning stake — no governance pre-commit.
- **`FRACTAL_MIN_CONSENSUS_STAKE_WEI`:** optional decimal **`u128`** read once at process start. When **> 0**, the producer returns **`AwaitingConsensusStake`** until **`State::consensus_stake_total_for_fingerprint`** for **this node's** validator fingerprint is **≥** the minimum. Default **`0`**.
- **`FRACTAL_UNBONDING_PERIOD_MS`:** milliseconds added at finalize to new unbond entries (default **604800000** = 7 days).
- **`FRACTAL_BLOCK_REWARD_WEI`:** per-block subsidy debited from **`DEVNET_FAUCET_TREASURY`** and credited into **`consensus_stakes`** for the proposer + parent-QC signers (effective weight `max(1, bonded)`). Default **`0`** (no subsidy).
- **Stake-weighted QC:** `NodeInner::record_vote` / **`try_form_qc`** / **`apply_synced_block`** parent-QC verify use **`validator_stake_weights`**; when **total bonded stake is zero**, quorum falls back to the legacy **vote count** rule (`2f+1`).
- **Delegation (PRD §12.3):** **`NativeCall::Delegate { validator_fingerprint, amount }`** bonds liquid FRAC into the same **`consensus_stakes` / `consensus_stake_shares`** maps as **`DepositConsensusStake`**. The validator operator (largest shareholder for that fingerprint) sets **`SetValidatorCommission { validator_fingerprint, commission_bps }`** (basis points, max **10_000**). Block rewards from **`FRACTAL_BLOCK_REWARD_WEI`** split per fingerprint: commission → **`consensus_reward_credits`** for the operator; the remainder **auto-compounds** pro-rata into shareholder stakes. **`WithdrawRewards { validator_fingerprint }`** pays accrued credits to liquid balance. **`WithdrawConsensusStake`** still unbonds principal only.
- **Mainnet economics profile:** set **`FRACTAL_ECONOMICS_PROFILE=mainnet`** on **`fractal-node`** to seed **`State.chain_economics`** (5M FRAC min validator bond, 21-day unbonding, permissionless **`RegisterValidator`**, EVM base-fee burn). Default / `testnet` keeps the Phase-2 testnet profile (1M FRAC min when governance enables permissionless, 7-day unbonding, no burn). Opcodes: **`RegisterValidator`**, **`Redelegate`**, governance **`SetChainEconomics`**. After each committed block the node calls **`sync_permissionless_validators()`** to rebuild the in-memory **`ValidatorSet`** from **`validator_registry`** rows that still meet the bonded minimum.

## Vote gossip (libp2p GossipSub, PRD §18 M7-d-5)

Votes use the **same QUIC/libp2p swarm** as block sync (request-response on `/fractalchain/sync/1.0.0`); gossip is **not** a second TCP stack.

- **Topic string:** **`/fractalchain/votes/1.0.0`** — canonical constant **`fractal_network::VOTES_TOPIC_STR`** (`crates/network/src/lib.rs`). Subscribers and publishers must use this exact string (GossipSub `IdentTopic`).
- **Payload:** **borsh** encoding of **`fractal_consensus::Vote`** (BLS signature over a narrow sign-body: view, height, header hash). Inbound messages are verified and applied to the local **`VotePool`**; see `crates/consensus/src/vote.rs` and `NodeInner::record_vote`.
- **`run_dev` / `run_follower`:** the binary **automatically** connects the execution node to gossip: it creates an internal channel, calls **`NodeInner::set_vote_sink`**, and passes the receiver into **`producer_network_task`** / **`follower_network_task`** so each committed block can **`forward_vote_after_commit`**. Operators do **not** configure `vote_sink` by hand for these entrypoints.
- **Embeds / tests:** use **`NodeInner::set_vote_sink(Some(tx))`** plus the same **`vote_rx`** passed into `fractal_node::p2p::producer_network_task(..., Some(vote_rx))` (see **`crates/node/tests/m7_d5_gossipsub_votes.rs`**).
- **Publish retries:** `gossipsub::publish` can return **`NoPeersSubscribedToTopic`** until the mesh has grafted a peer; the P2P task **queues** the last payload and retries after swarm activity (`crates/node/src/p2p.rs`).
- **Followers:** set **`FRACTAL_BOOTSTRAP`** to the producer’s printed QUIC multiaddr (with `/p2p/<PeerId>`). Use the **same** `FRACTAL_VALIDATOR_SET` as the producer; each process should use its own **`FRACTAL_VALIDATOR_INDEX`** on multi-validator devnets.
- **Implementation map:** `crates/node/src/p2p.rs` (GossipSub config, subscribe, publish, inbound decode), `crates/node/src/lib.rs` (`run_dev`, `run_follower`, `forward_vote_after_commit`), **`crates/node/tests/m7_d5_gossipsub_votes.rs`** (two-node vote exchange smoke).

## P2P host identity (PRD §8.1)

- **`FRACTAL_P2P_IDENTITY_PATH`:** optional path to a file storing the libp2p **protobuf-encoded Ed25519 private key** (same format as [`Keypair::to_protobuf_encoding`](https://docs.rs/libp2p-identity/latest/libp2p_identity/struct.Keypair.html#method.to_protobuf_encoding) in `libp2p-identity`). If the file is missing, the node creates it (and parent directories) on first start. **Stable `PeerId`** across restarts lets **`FRACTAL_BOOTSTRAP`** `/p2p/...` strings stay valid for operators. If unset, each run uses a **fresh** Ed25519 key (unchanged default for tests and one-shot local runs).
- **Precedence:** when **`FRACTAL_P2P_IDENTITY_PATH`** is set to a non-empty path, that file wins over any other host-key source (including the docker dev fixture below).
- **`FRACTAL_P2P_DOCKER_FIXTURE`:** optional dev-only shortcut for **`testnets/devnet/docker-compose.yml`**: set to **`producer`** or **`follower`** (aliases **`1`** / **`2`**) to use built-in deterministic Ed25519 seeds so **`FRACTAL_BOOTSTRAP`** can hard-code **`/p2p/<PeerId>`** without mounting keys. The canonical producer **`PeerId`** is asserted in **`crates/node/src/p2p.rs`** (`docker_fixture_producer_peer_id_matches_devnet_compose`). If both **`FRACTAL_P2P_IDENTITY_PATH`** and **`FRACTAL_P2P_DOCKER_FIXTURE`** are set, **identity path is used** (operator-supplied key).

### Validator onboarding (PRD §18 M7)

Print the fixture table (fingerprint, BLS pubkey, optional dev **`FRACTAL_VALIDATOR_SECRET_HEX`**) for the current `FRACTAL_VALIDATOR_SET`:

```bash
cargo run -p fractal-node -- print-devnet-validator-keys
```

Use `FRACTAL_VALIDATOR_SET=7` (or `bft7`) in the same shell to print all seven rows. Use **`21`** or **`bft21`** for the twenty-one-row M8 table (quorum 13). **Dev fixture secrets are not for mainnet** — production onboarding will use HSM-held keys and governance-approved validator sets once that path exists in the binary.

### Gossip topics (M7)

- **`/fractalchain/votes/1.0.0`** — borsh [`Vote`] payloads (`fractal_network::VOTES_TOPIC_STR`).
- **`/fractalchain/timeouts/1.0.0`** — borsh [`Timeout`] (`view`, embedded **`high_qc`**, BLS sig) for view pacemaker / quorum timeout certificates (`fractal_network::TIMEOUTS_TOPIC_STR`, PRD §7.4 M7-f). Multi-validator devnets subscribe on producer and follower swarms.

## M8 slice — BFT-21 fixture + sync (`docs/prd.md` §M8)

- **Validator set:** `FRACTAL_VALIDATOR_SET=21` or `bft21` loads **`ValidatorSet::phase3_bft21_fixture()`** (`n = 21`, quorum **13**). Use **`cargo run -p fractal-node -- print-devnet-validator-keys`** with that env to dump all rows + dev **`FRACTAL_VALIDATOR_SECRET_HEX`** hints. Same caveats as BFT-7: one binary with **`FRACTAL_VALIDATOR_INDEX=0`** only proposes when `view % 21 == 0` unless other validators gossip votes / sync blocks.
- **Snapshot fast sync:** When a joining node has **local height 0** and the peer’s tip height is **&gt; 0**, the follower requests **`SyncRequest::GetSnapshot`** by default. Normal peers serve **`ChainSyncSnapshotV2`** (`crates/node/src/chain_snapshot.rs`) — consensus tip fields, validator entries, fee/mempool metadata, full block vector for the current dev chain model, and a chunked state payload bound by per-chunk hashes, `stateRoot`, and the EVM account MPT root. Set **`FRACTAL_PROOF_CHAIN_FAST_SYNC=1`** on the serving peer to prefer **`ChainSyncProofSnapshotV1`**: verified chunked state plus the checkpoint tip block, masterchain proof chain, and Plonky2 bundle, without carrying full raw execution history. Import verifies state/MPT roots, masterchain `globalStateRoot`, proof ranges, and Plonky2 binding, then writes `cf_state`, `cf_snapshots`, shard anchors, and masterchain blocks before syncing forward. Legacy **`ChainSyncSnapshotV1`** remains accepted as a fallback. Wire: **`fractal_network::SyncRequest::GetSnapshot`** / **`SyncResponse::Snapshot(Vec<u8>)`** (`crates/node/src/p2p.rs`). **`FRACTAL_FAST_SYNC`:** unset → **on**; set to **`0`**, **`false`**, **`off`**, or **`no`** to force legacy **`GetBlocks`** replay only. **`FRACTAL_VALIDATOR_SECRET_HEX`** is refreshed from env rules after applying a snapshot.
- **Smoke:** `./scripts/try-m8-bft21-smoke.sh` runs consensus tests + prints the onboarding table head.

## Async proof condenser (PRD §7.8 M9)

- When **`fractal-node`** starts via **`run_dev`** or **`run_follower`**, it spawns a background worker (`crates/proof-condenser`) by default: each **finalized** block (local produce or `apply_synced_block`) enqueues a cheap **checkpoint job**; heavy work runs in **`tokio::task::spawn_blocking`** and does **not** delay consensus.
- **`FRACTAL_ASYNC_PROOF`:** unset or any value other than `0` / `false` / `off` / `no` → worker **on** (default **`1`**). Set to **`0`** to disable (e.g. minimal profiling runs).
- **`FRACTAL_CHAIN_ROCKSDB_PATH`:** optional directory; when set (or when **`FRACTAL_PROOF_ROCKSDB_PATH`** alone is set — same unified open), the node writes **PRD §10.3** chain data after each committed block: **`cf_blocks`** (`StoredBlockV1`), **`cf_tx_index`**, **`cf_receipts`**, **`cf_state`** (`StoredStateAtHeightV1` — full `State` after the block, dev slice before MPT). If **both** `FRACTAL_CHAIN_ROCKSDB_PATH` and **`FRACTAL_PROOF_ROCKSDB_PATH`** are set, they **must be identical** (one RocksDB lock per process).
- **`FRACTAL_PROOF_ROCKSDB_PATH`:** optional directory. When set, `fractal-node` opens a **RocksDB** there via **`fractal_storage::FractalRocksDb`**: all **PRD §10.3** column families (`cf_state`, `cf_blocks`, `cf_tx_index`, `cf_receipts`, `cf_native_events`, `cf_mempool`, `cf_consensus`, `cf_snapshots`) plus **`checkpoint_proofs`** (M9). Checkpoint records are stored as **`borsh(PersistedCheckpointProofV1)`** under **`checkpoint_proofs`**, keyed by block height (big-endian `u8[8]`). Older directories that only had `checkpoint_proofs` automatically receive the additional empty column families on next open. Recommended for production-style persistence.
- **`FRACTAL_PROOF_ARTIFACT_DIR`:** optional legacy **flat-file** directory; writes **`{height:016}.proof.borsh`** per height. You may set **both** env vars; records are written to **each** backend that is configured. **`get`** tries memory, then RocksDB, then filesystem.
- **JSON-RPC:** **`fractal_getCheckpointProof`** and **`fractal_getCheckpointProofDigest`** take **`[heightHex]`** (e.g. `["0x1"]`) and return a JSON object / digest string, or **`null`** if async proof is disabled or no record exists yet for that height.
- **Wallet revocation:** **`fractal_getWalletRevocationMerkleRoot`** (`[]`) → head root (`0x` + 32 bytes). **`fractal_getWalletRevocationEntries`** (`[]`) → `{ revocationRoot, entries: [{ capId, revokedAtMs, reasonCode, cascade }], count }` for proof construction. CLI: **`cap build-revocation-proof --from-rpc`** with **`FRACTAL_RPC_URL`**. Providers call **`provider_verify_intent_capability`** before match (`docs/wallet.md` §4.6 (c)).
- **Wallet reputation (governance snapshots + indexer):** **`fractal_getWalletReputation`** (`[]`) → `{ scores: [{ providerId, toolClass, scoreMilli, ledgerCommitment }], count }`. Submit: **`cargo run -p fractal-cli -- chain submit-reputation-snapshot --provider-id <hex32> --tool-class <u8> --summary-json tools/reputation-example-summary.json --apply-local`** (dev) or **`--rpc-url`**. **`fractal-indexer`** (see `./scripts/serve-indexer.sh`, `tools/indexer/README.md`) persists **`reputation_rows` in SQLite** and serves GraphQL; **`SettleBatch` / `SettleReceipt` → §10.4 merge is on by default** (`INDEXER_REPUTATION_MERGE_SETTLEMENTS=0` to disable). Stub JSON workflow: **`./scripts/run-indexer-reputation.sh`** (`fractal-indexer-stub`) keeps **`INDEXER_REPUTATION_MERGE_SETTLEMENTS=0`** for snapshot-first `INDEXER_REPUTATION_STORE_PATH` mirroring.
- **Wallet emergency stop:** **`fractal_getWalletEmergencyStop`** (`[]`) → boolean. Governance toggles the global switch via **`NativeCall::WalletEmergencyStopV1`**; CLI: **`cargo run -p fractal-cli -- chain emergency-stop --engage`** / **`--disengage`** (`--apply-local` or `--rpc-url`). Scoped hardening is also on-chain: **`WalletScopedEmergencyStopV1 { engage, scope, master_public_key, master_sig }`** verifies the master Ed25519 signature over `WalletScopedEmergencyStopSignBodyV1 { chain_id, engage, scope }` and blocks future capability mints under that master when workspace/project/task/tool-class/provider scope selectors match. See `docs/wallet.md` §25.1 / §29.
- The worker runs **real STWO** prove+verify on a tiny checkpoint/range-bound witness (with `blake3` fallback if proving fails). `checkpoint_job_from_block_range` first builds a deterministic RISC-V replay trace over finalized blocks, validating contiguous heights, parent links, and transaction roots, then binds `riscvTraceRoot` / `riscvTraceSteps` into the STWO public inputs (`docs/stwo-run-notes.md`). Heavy work stays in **`spawn_blocking`**. Logs: `fractal-proof-condenser: async checkpoint height=…`.

## Track B — shards + masterchain ZK (M10 / M11)

- **Pilot shards (two processes):** `./scripts/run-pilot-shards.sh` — `FRACTAL_CONSENSUS_MODE=hyperbft`, `FRACTAL_SHARD_COUNT=2`, RPC **8545** / **8547**, 70 ms blocks, anchors every **100** blocks by default. Automated smoke: `./scripts/run-pilot-shards.sh smoke-start` (anchor every **4** blocks + STWO→Plonky2 on each shard) or `./scripts/run-pilot-shards.sh smoke` if already running.
- **HyperBFT BFT-7 validator shard:** `./scripts/run-hyperbft-bft7-shard.sh smoke-start` — seven `fractal-node` processes on one shard (`FRACTAL_VALIDATOR_SET=7`, RPC **8650**–**8656**, QUIC **9200**–**9206**). Each validator runs HyperBFT ticks and shard-scoped vote gossip; block sync pulls from any connected peer ahead of local tip. Lab default `FRACTAL_DEV_INJECT_QUORUM=1` synthesizes quorum votes until proposal gossip ships. Smoke checks all seven processes are up and **cluster max** `eth_blockNumber` ≥ 2 (`HYPERBFT_SMOKE_MIN_HEIGHT` to override). `./scripts/run-hyperbft-bft7-shard.sh smoke` when already running.
- **HyperBFT deterministic torture slice:** `cargo test -p fractal-node --test hyperbft_bft7_torture` simulates a BFT-7 shard with validators **5** and **6** partitioned out, quorum timeout view-skips over the offline leaders, native `NoOp` load on live leaders, and synthetic p99 finality checked against the **900 ms** M10 budget. This is a regression guard, not the lab-hardware soak/sign-off.
- **Shard 0 monolith migration:** Track A `ChainSyncSnapshotV1` can be imported into an empty Track B shard-0 node only. Set **`FRACTAL_SHARD_COUNT>1`**, **`FRACTAL_SHARD_ID=0`**, and either **`FRACTAL_MIGRATE_MONOLITH_SNAPSHOT_PATH=/path/to/snapshot.borsh`** at startup or **`FRACTAL_MIGRATE_MONOLITH_TO_SHARD0=1`** on a follower receiving a monolith fast-sync snapshot. Non-zero target shards and non-monolith source snapshots are rejected.
- **Cross-shard messages:** `MasterchainLedger::submit_cross_shard_message` queues `CrossShardMessageV1`; the next masterchain block carries a deterministic, deduplicated `crossShardMessages` list sorted by `(from_shard, to_shard, payload_hash)`. Destination shards validate `keccak256(payload) == payloadHash`, decode payloads as `borsh(NativeCall)`, execute them through the native syscall path, and idempotently journal `(masterchain_height, message_index)`.
- **RPC gateway (M10):** `./scripts/run-rpc-gateway.sh` listens on **8549** by default and fronts the pilot shards. It routes `eth_sendRawTransaction` by recovered signer home shard, address reads (`eth_getBalance`, nonce, code, storage) by address home shard, `eth_call` by `to` address, `eth_estimateGas` by `from`, transaction/hash lookups across shards, and `eth_getLogs` either to the addressed shard or merged across shards. Configure with `FRACTAL_GATEWAY_ADDR`, `FRACTAL_GATEWAY_SHARDS` (`0=http://127.0.0.1:8545,1=http://127.0.0.1:8547`), or `FRACTAL_SHARD_RPC_URLS`.
- **Dedicated masterchain BFT:** `./scripts/run-masterchain-bft.sh` — coordination-only process on RPC **8550** (`fractal-masterchain` binary). Shards set `FRACTAL_MASTERCHAIN_RPC=http://127.0.0.1:8550` to post `fractal_submitShardAnchor` instead of sealing masterchain blocks locally. The binary honors `FRACTAL_VALIDATOR_SET=7|bft7|21|bft21` and `FRACTAL_VALIDATOR_INDEX`; it also starts masterchain gossipsub on **`FRACTAL_MASTERCHAIN_P2P_LISTEN`** (default `/ip4/0.0.0.0/udp/0/quic-v1`) and dials comma-separated **`FRACTAL_MASTERCHAIN_BOOTSTRAP`** entries. Vote payloads are `borsh(MasterchainVoteGossipV1)` on `/fractalchain/masterchain/votes/1.0.0`; timeout payloads are `borsh(MasterchainTimeoutGossipV1)` on `/fractalchain/masterchain/timeouts/1.0.0`. BFT-7 vote/timeouts form 5-of-7 QCs in `fractal-masterchain` tests, and `./scripts/run-masterchain-bft7-smoke.sh` runs a seven-process localhost QC smoke without direct vote injection. Pilot + masterchain: `./scripts/run-pilot-shards.sh start-with-masterchain`.
- **Permissionless prover market:** `fractal_submitValidityProof` is open to non-validator provers. Enable bond-gated admission with **`FRACTAL_PROVER_MARKET_MIN_BOND_WEI`**; then provers must register via **`fractal_registerProver`** with `{ prover, bondWei }` before submissions are accepted. Query with **`fractal_getProverIdentity`**. Anti-spam knobs: **`FRACTAL_PROVER_MARKET_MAX_PENDING`** (default **8**) and **`FRACTAL_PROVER_MARKET_MAX_RANGE_BLOCKS`** (default **10000**). Optional startup registration for the local auto-submitter: **`FRACTAL_PROVER_ADDRESS`** + **`FRACTAL_PROVER_BOND_WEI`**.
- **Prover economics:** dedicated masterchain rewards can be enabled with **`FRACTAL_PROVER_REWARD_PER_BLOCK_WEI`**. Accepted proofs credit `prover` from the in-memory treasury by `reward = basePerBlock * coveredBlocks * lagHalfLife / (lagSeconds + lagHalfLife)`, capped by remaining treasury. Optional knobs: **`FRACTAL_PROVER_REWARD_LAG_HALF_LIFE_SECONDS`** (default **60**), **`FRACTAL_PROVER_REWARD_TREASURY_WEI`** (initial treasury), and **`FRACTAL_PROVER_TREASURY_ADDRESS`** (`0x` + 20 bytes, event metadata).
- **Invalid proof slashing (§7.8):** enable with **`FRACTAL_PROOF_SLASHING_ENABLED=1`**. Optional **`FRACTAL_PROOF_REQUIRE_VERIFIED_STWO=1`** (mandatory verified STWO on seal) and **`FRACTAL_PROOF_SLASH_AMOUNT_WEI`**. Each unique `InvalidProofSlashEventV1` now attempts one burn against the registered prover identity bond, records `burnedBondWei` / before-after bond fields in **`fractal_getInvalidProofSlashEvents`**, and deactivates the prover if the remaining bond falls below **`FRACTAL_PROVER_MARKET_MIN_BOND_WEI`**.
- **Masterchain follower sync:** shard followers now pull `MasterchainBlockV1` records over the same QUIC/libp2p request-response channel as execution blocks (`GetMasterchainTip` / `GetMasterchainBlocks`), so local `fractal_getMasterchainHead` can catch up without RPC-only masterchain reads.
- **Single-node lab (STWO → tier1 → Plonky2):** `./scripts/run-track-b-lab.sh` then `./scripts/smoke-track-b-e2e.sh` — anchor every **4** blocks, auto tier-1 submit, verified STWO public statements are bound into the Plonky2 circuit when artifacts are available, then Plonky2 seals on anchor.
- **Env:** `FRACTAL_SHARD_COUNT`, `FRACTAL_SHARD_ID`, `FRACTAL_ANCHOR_INTERVAL` (`0` = off on monolith), `FRACTAL_AUTO_VALIDITY_PROOF` (default **on** when `FRACTAL_ANCHOR_INTERVAL > 0`), `FRACTAL_PROVER_ADDRESS` (optional `0x` + 20 bytes).
- **JSON-RPC:** `fractal_getShardId`, `fractal_getMasterchainHead`, `fractal_getDeliveredCrossShardMessages`, `fractal_submitValidityProof`, `fractal_registerProver`, `fractal_getProverIdentity`, `fractal_getGlobalZkRoot`, `fractal_getGlobalZkProof`, `fractal_getLightClientHead` (masterchain + Plonky2 + execution tip for light sync), `fractal_getCheckpointProof` / `Digest`.
- **Light-client verifier (no full node):** `fractal-light-client` crate — `fetch_and_verify_light_client_head(rpc_url)` or offline `verify_light_client_head` / `verify_masterchain_block` after parsing `fractal_getLightClientHead` JSON. Recomputes `globalStateRoot` from shard anchors, validates tier-1 proof ranges, verifies Plonky2 SNARK when `plonky2` is present. Tests: `cargo test -p fractal-light-client`.
- **Pruning (M11):** `FRACTAL_PRUNE_AFTER_VALIDITY_PROOF=1` (default) drops in-memory execution blocks, RocksDB `checkpoint_proofs`, and height-scoped execution rows (`cf_blocks`, block-hash index, `cf_tx_index`, `cf_receipts`, native events, height-keyed `cf_state`) at or below the proved `endBlock` after each seal with a non-zero `globalZkRoot` (keeps a 32-block tip window; content-addressed MPT nodes are retained).
- **Tests:** `cargo test -p fractal-node --test masterchain_zk --test stwo_plonky2_pipeline --test shard_anchor --test quic_sync`, `cargo test -p fractal-node --test hyperbft_bft7_torture`, `cargo test -p fractal-node chain_snapshot_v1`, `cargo test -p fractal-masterchain`, and `./scripts/run-masterchain-bft7-smoke.sh`.

## Faucet (tFRAC-style native balance)

- Binary: `cargo run -p fractal-faucet` (or Docker service in `testnets/devnet/docker-compose.yml`).
- Treasury address: **`fractal_core::DEVNET_FAUCET_TREASURY`** (prefunded in `NodeInner::devnet()`).
- Env: `FRACTAL_RPC_URL`, `FAUCET_BIND`, `FAUCET_DRIP_AMOUNT`, `FAUCET_COOLDOWN_SECS`.

## Explorer

- **FractalScan** static UI under `tools/explorer` — Blockscout-style dev explorer: search, block window, tx/receipt/logs, optional checkpoint digest (`tools/explorer/README.md`).
- **Deep index (optional):** run **`./scripts/serve-indexer.sh`**, then open FractalScan with **`?indexer=http://127.0.0.1:8088`** (same host as GraphQL). Indexed blocks/txs/search come from **`GET /api/v1/explorer/*`**; live balance, code, and receipt logs still use JSON-RPC.
- **Semantics:** `docs/explorer.md` (tx hash identity, leader vs follower).
- From repo root: **`./scripts/serve-explorer.sh`** (optional **`EXPLORER_PORT`**).

## RPC liveness (status stub)

- **`tools/status/`** — `./scripts/serve-status.sh` (optional **`STATUS_PORT`**, default **3355**). Polls `eth_chainId`, `eth_blockNumber`, `web3_clientVersion` from a pasted RPC URL (CORS must allow the status page origin).

## Docker devnet

- `testnets/devnet/docker-compose.yml` builds **node** + **faucet** images from this repo. **`node`** sets **`FRACTAL_P2P_DOCKER_FIXTURE=producer`** for a stable QUIC **`PeerId`**.
- **Health checks:** **`node`** and **`follower`** use Compose `healthcheck` + `curl` + `eth_chainId`; **`faucet`** image uses `GET /health` in **`Dockerfile.faucet`**. **`follower`** and **`faucet`** use **`depends_on: service_healthy`** on **`node`** so they start after JSON-RPC responds.
- Optional **`follower`** service (Compose profile **`follower`**): **`FRACTAL_BOOTSTRAP=/ip4/node/udp/4001/quic-v1/p2p/<producer>`**, **`FRACTAL_P2P_DOCKER_FIXTURE=follower`**, RPC on **8546**. From repo root: `docker compose -f testnets/devnet/docker-compose.yml --profile follower up --build`.
- Example multiaddrs for followers: `testnets/devnet/bootnodes.example.txt`.

Community (Discord) and a public status page are **operational** concerns outside this repository.

## M5 bridge smoke (PRD exit scale)

PRD **M5** exit line includes ≥100 receipts settled then claimed with no manual intervention (`docs/prd.md` §M5).

- **Local / CI:** from repo root, with JSON-RPC listening on **`FRACTAL_RPC_URL`** (default `http://127.0.0.1:8545`):

  ```bash
  ./scripts/run-mvp-bridge-smoke.sh
  ```

  Uses **`MVP_RECEIPT_COUNT`** (default **100**) synthetic receipts via **`fractal-mvp-bridge`** (PRD M5 reference bridge in `crates/mvp-backend`). The script waits for RPC with **`./scripts/wait-for-jsonrpc.sh`** (`RPC_WAIT_SECS`, default 180).

  **`cargo run -p fractal-mvp-backend --bin fractal-mvp-bridge -- --help`** prints environment variables and examples.

- **Docker devnet:** `docker compose -f testnets/devnet/docker-compose.yml up -d node` then the same smoke script against `http://127.0.0.1:8545`.

- **GitHub Actions:** workflow definition lives at **`docs/ci/mvp-bridge-smoke.workflow.yml`** (not under `.github/workflows/` in git) so HTTPS pushes work with default PAT scopes. **Install:** copy that file to `.github/workflows/mvp-bridge-smoke.yml`, or push with a PAT that includes the **`workflow`** scope, or create the workflow in the GitHub Actions UI. The script **`./scripts/run-mvp-bridge-smoke.sh`** is unchanged.

- **Off-chain-shaped batch:** set **`MVP_RECEIPTS_JSON`** to a file (see `crates/mvp-backend/testdata/mvp_receipts_sample.json`) instead of synthetic counts; see `crates/mvp-backend` module docs on `fractal-mvp-bridge`.
