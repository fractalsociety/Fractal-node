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
- **Follower** nodes replay **consensus blocks**; if they do not replicate the leader’s `eth_signed_raw` / RPC-hash maps, `eth_getTransactionByHash` may disagree with the leader for the same logical transaction.

**Operational rule:** for explorer + wallet debugging, point the UI at the **same JSON-RPC instance** you used to submit the transaction (usually the producer), unless you have verified follower RPC parity.

## What the static explorer does

- Reads **only** JSON-RPC (`eth_*`, `web3_*`, `net_*`).
- Does not index the chain locally; it is a thin client.

For deep block analytics (internal traces, contract labels), a **Blockscout-class** deployment remains a separate milestone; this repo ships the **dev explorer** plus the semantics above.

## Related paths

- Run: `./scripts/serve-explorer.sh` — see `tools/explorer/README.md`.
- Devnet compose + faucet: `docs/devnet.md`, `testnets/devnet/README.md`.
- Liveness checks for operators: `tools/status/` + `./scripts/serve-status.sh`.
