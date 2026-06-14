# Soft Finality vs Proof Finality

FractalChain exposes two finality levels. Operators should treat them as separate states, not as two names for the same confirmation.

## Finality Levels

| Status | Meaning | Operator use |
| --- | --- | --- |
| `soft` | A committee or sequencer has accepted the block on the fast path. The block is visible to wallets, explorers, and ordinary application flows. | Good for low-value UX, local indexing, previews, and latency-sensitive agent actions. |
| `proof` | A validity proof for the block has been accepted. This is the hard settlement state used by bridge and settlement APIs. | Required for bridges, high-value settlement, dispute accounting, and external systems that rely on irreversible state. |

Blocks move in one direction: `soft` to `proof`. A block that has not yet been proven must not be presented as settlement-final.

## RPC And Explorer Surfaces

- `eth_getBlockByNumber` and `eth_getBlockByHash` include `finalityStatus: "soft" | "proof"`.
- Proof-final block responses also include proof circuit version, coverage manifest digest, and covered feature mask when available.
- The dev explorer shows finality on the chain head, recent block table, and block detail view.
- The indexer explorer API returns `finality_status` on block responses. Older indexer rows can show `unknown` until they are re-indexed from an RPC node that exposes `finalityStatus`.

## Operating Rules

- For normal wallet UX, display soft-final blocks immediately, but label them as soft-final.
- For high-value actions, warn users when the referenced block is only soft-final.
- For bridge withdrawals, settlement finality, and external accounting, require proof-final blocks.
- When `proof_required_settlement` is enabled, settlement APIs must reject soft-final blocks even if they are otherwise present in the local chain.
- Proof coverage must match the block feature set. Native-only proofs cannot finalize mixed/EVM-capable blocks or EVM-capable zones.
- Bridge and settlement errors are intentionally distinct: `soft-final` means wait for proof finality, `uncovered-circuit` means the accepted proof does not cover the requested feature set, and `unavailable-da` means data availability is missing.

## Monitoring

Track proof pipeline health separately from block production:

- `proof_final_height`: should advance behind soft head by an expected proving lag.
- Proof latency metrics: watch latest and average proof latency.
- Proof rejection reasons: repeated DA or public-input failures indicate a settlement pipeline issue, not a mempool issue.

If soft head advances while proof-final height stalls, applications can keep serving low-value reads, but bridges and high-value settlement should pause until proof finality catches up.
