/**
 * Reference **provider** wire types (`docs/wallet.md` §7).
 * Amounts are decimal strings (base units, same as Rust `Amount` / u128).
 * Fixed-size fields are `0x` + hex (no 0x prefix also accepted by your parser if you choose).
 */

export type Hex = `0x${string}`;

/** §7.1 — signed tool intent body (map fields to borsh layout in `fractal_wallet::market::ToolIntentBody`). */
export interface ToolIntentBodyWire {
  intentId: Hex;
  agentSession: Hex;
  taskId: string;
  toolClass: number;
  payloadCommitment: Hex;
  maxPrice: string;
  verificationTier: number;
  deadlineMs: string;
  nonce: string;
}

export interface ToolIntentWire extends ToolIntentBodyWire {
  signature: Hex;
}

/** §7.2 — provider quote (borsh `QuoteBody` + Ed25519 sig). */
export interface QuoteBodyWire {
  quoteId: Hex;
  intentId: Hex;
  providerId: Hex;
  price: string;
  expiryMs: string;
}

export interface QuoteWire extends QuoteBodyWire {
  signature: Hex;
}

/** §29 — indexer hook: monotonic cursor for polling subgraph / native events. */
export interface IndexerCursorWire {
  lastHeight: string;
}

/** §29 — filter intents by tool-class bitmask (see `ToolClass::bit()` in Rust). */
export interface IntentPollFilterWire {
  toolClassMask: string;
}
