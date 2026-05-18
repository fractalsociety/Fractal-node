# FractalChain dev explorer — RPC semantics

This document supports **PRD §18 M6** (`docs/prd.md`) and the static UI in **`tools/explorer/`**. It explains how **transaction hashes** in JSON-RPC relate to FractalChain’s dual encoding paths (Ethereum-style raw txs vs native `Transaction`).

## Two ways a transaction can be identified on-chain

1. **Ethereum-style hash (MetaMask / Hardhat / `eth_sendRawTransaction`)**  
   `H = keccak256(raw_eip1559_bytes)` — this is what wallets and explorers usually show for EVM-submitted txs.

2. **Internal Fractal hash (borsh `Transaction`)**  
   `H' = keccak256(borsh_encode(Transaction))` — used for native-only submissions and for some internal maps before an EIP-1559 raw payload is stored.

The **block** object returned by `eth_getBlockByNumber(..., false)` lists **hashes** in its `transactions` array. For blocks produced by the current `fractal-node` producer:

- If the tx was submitted as **EIP-1559 raw bytes**, the block lists **`keccak256(raw)`** (the Ethereum-style hash) when that path is active (see `NodeInner` mined-tx / RPC hash mapping in `crates/node`).
- If the tx is **native borsh only**, the block lists **`keccak256(borsh(tx))`**.

So the explorer’s “transaction hash” column is always “whatever the RPC returned for that block” — **do not assume** it is always equal to `eth_getTransactionByHash` for a hash you copied from another node unless both nodes agree on the same mapping.

## Leader vs follower

- **Producer (leader)** holds `eth_signed_raw` and can return full EIP-1559 JSON from `eth_getTransactionByHash` for those txs.
- **Follower** nodes replay **consensus blocks** and, as of 2026-05, rebuild **`mined_txs`**, **`eth_signed_raw`**, and RPC↔internal hash maps from each block’s `eth_signed_raw` sidecar so `eth_getTransactionByHash` / log `transactionHash` can match the producer for the same logical transaction (still verify in your deployment).

**Operational rule:** for explorer + wallet debugging, point the UI at the **same JSON-RPC instance** you used to submit the transaction (usually the producer), unless you have verified follower RPC parity.

## What the static explorer does (`tools/explorer/` — **FractalScan**)

- Reads **only** JSON-RPC (`eth_*`, `web3_*`, `net_*`, optional `fractal_*` checkpoint methods).
- Does **not** index the chain locally; it is a thin client (no Blockscout Postgres stack).
- **FractalScan (dev)** adds a Blockscout-**style** surface: universal search, per-block tx table with receipt status/gas, tx summary + **event logs** table, configurable block window, `eth_syncing`, and head **checkpoint digest** when the node exposes it.

For **contract verification**, **internal traces**, and **GraphQL** the way hosted Blockscout does, you still need a full Blockscout (or similar) deployment or future indexer work — see PRD M6 vs M8.

For deep block analytics (internal traces, contract labels) beyond what JSON-RPC returns today, a **hosted Blockscout** deployment remains an optional ops milestone; this repo ships **FractalScan** as the **static** operator/developer explorer plus the semantics above.

## Related paths

- Run: `./scripts/serve-explorer.sh` — see `tools/explorer/README.md`.
- Devnet compose + faucet: `docs/devnet.md`, `testnets/devnet/README.md`.
- Liveness checks for operators: `tools/status/` + `./scripts/serve-status.sh`.
