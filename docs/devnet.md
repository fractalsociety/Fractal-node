# Devnet & M6 operator notes

This page ties together **PRD §18 M6** pieces in-repo (anchored on **`docs/prd.md`**, not `docs/wallet.md`).

## JSON-RPC

- Default local bind: `http://127.0.0.1:8545` (`FRACTAL_RPC_ADDR`).
- **CORS** is enabled on the HTTP JSON-RPC server so browser tools (e.g. `tools/explorer`) can call `eth_*` methods cross-origin during development.

## Validator set (PRD §7 M7-b)

- **`fractal-node`** reads **`FRACTAL_VALIDATOR_SET`** when started via **`run_dev`** / **`run_follower`** (not when using `NodeInner::devnet()` in unit tests).
- Values: unset or anything other than `7` / `bft7` → Phase-1 **singleton** (`n = 1`). `7` or `bft7` → in-repo **BFT-7 fixture** (`ValidatorSet::phase2_bft7_fixture()`): seven validators with BLS keys; block header **`proposer`** rotates with the active HotStuff **view** (see `fractal_consensus::ValidatorSet::expected_proposer`).
- Producer and follower **must use the same** `FRACTAL_VALIDATOR_SET` so synced blocks pass `InvalidProposer` checks.
- **`FRACTAL_VALIDATOR_INDEX`:** which validator this process is (`0 .. validator_set_size-1`). Default **`0`**. If the value is **≥** the set size, the node logs and **clamps to `0`** so a stale env var cannot disable a singleton devnet. The producer **only builds a block when this index is the leader for the current view** (`NotMyTurn` ticks otherwise); see `fractal_node::try_produce_one_tick`.
- **`FRACTAL_VALIDATOR_SECRET_HEX`:** optional **32-byte** BLS secret ( **64** hex digits, optional `0x` prefix). If set, it must decode to a valid **blst** secret and **should** match `validators.bls_pubkey(FRACTAL_VALIDATOR_INDEX)` or peers will reject your votes (the node logs a pubkey mismatch warning). If unset or malformed, the node uses the **deterministic dev fallback** secret for `(validator_set, index)` when one exists (`run_dev` / `run_follower`).

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

## Faucet (tFRAC-style native balance)

- Binary: `cargo run -p fractal-faucet` (or Docker service in `testnets/devnet/docker-compose.yml`).
- Treasury address: **`fractal_core::DEVNET_FAUCET_TREASURY`** (prefunded in `NodeInner::devnet()`).
- Env: `FRACTAL_RPC_URL`, `FAUCET_BIND`, `FAUCET_DRIP_AMOUNT`, `FAUCET_COOLDOWN_SECS`.

## Explorer

- Static UI under `tools/explorer` — chain summary, recent blocks (click a row for tx hashes), account (balance, nonce, code) + tx lookup (`tools/explorer/README.md`).
- **Semantics:** `docs/explorer.md` (tx hash identity, leader vs follower).
- From repo root: **`./scripts/serve-explorer.sh`** (optional **`EXPLORER_PORT`**).

## RPC liveness (status stub)

- **`tools/status/`** — `./scripts/serve-status.sh` (optional **`STATUS_PORT`**, default **3355**). Polls `eth_chainId`, `eth_blockNumber`, `web3_clientVersion` from a pasted RPC URL (CORS must allow the status page origin).

## Docker devnet

- `testnets/devnet/docker-compose.yml` builds **node** + **faucet** images from this repo.
- Example multiaddrs for followers: `testnets/devnet/bootnodes.example.txt`.

Community (Discord) and a public status page are **operational** concerns outside this repository.

## M5 bridge smoke (PRD exit scale)

PRD **M5** exit line includes ≥100 receipts settled then claimed with no manual intervention (`docs/prd.md` §M5).

- **Local / CI:** from repo root, with JSON-RPC listening on **`FRACTAL_RPC_URL`** (default `http://127.0.0.1:8545`):

  ```bash
  ./scripts/run-mvp-bridge-smoke.sh
  ```

  Uses **`MVP_RECEIPT_COUNT`** (default **100**) synthetic receipts via `fractal-mvp-bridge`. The script waits for RPC with **`./scripts/wait-for-jsonrpc.sh`** (`RPC_WAIT_SECS`, default 180).

- **Docker devnet:** `docker compose -f testnets/devnet/docker-compose.yml up -d node` then the same smoke script against `http://127.0.0.1:8545`.

- **GitHub Actions:** workflow definition lives at **`docs/ci/mvp-bridge-smoke.workflow.yml`** (not under `.github/workflows/` in git) so HTTPS pushes work with default PAT scopes. **Install:** copy that file to `.github/workflows/mvp-bridge-smoke.yml`, or push with a PAT that includes the **`workflow`** scope, or create the workflow in the GitHub Actions UI. The script **`./scripts/run-mvp-bridge-smoke.sh`** is unchanged.

- **Off-chain-shaped batch:** set **`MVP_RECEIPTS_JSON`** to a file (see `crates/mvp-backend/testdata/mvp_receipts_sample.json`) instead of synthetic counts; see `crates/mvp-backend` module docs on `fractal-mvp-bridge`.
