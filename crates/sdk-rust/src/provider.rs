//! Reference **provider** SDK (`docs/wallet.md` §7, §21.2, §25.1).
//!
//! Re-exports wallet market types and adds small helpers for off-chain services.
//! No HTTP client or chain RPC here — wire your own REST/gRPC using the types.
//!
//! **Indexer stub:** `cargo run -p fractal-indexer-stub` (`INDEXER_RPC_URL`, `INDEXER_POLL_MS`, optional `INDEXER_JSON_LOG=1`) polls `eth_blockNumber` and `eth_getBlockByNumber` for operators (`docs/wallet.md` W6-d / W6-e).

pub use fractal_wallet::{
    market::{
        provider_id_from_public_key, ChallengeError, DeliveredInfo, IntentState, MatchError,
        PostIntentError, PostReceiptError, ProviderStake, Quote, QuoteBody, ResolveError,
        SettleError, SigVerifyError, ToolIntent, ToolIntentBody, ToolMarket,
        DEFAULT_OPTIMISTIC_CHALLENGE_MS,
    },
    types::{
        Amount, IntentId, ProviderId, PublicKey, QuoteId, ReceiptId, TaskId, ToolClass,
        VerificationTier, WorkspaceId,
    },
};

/// Monotonic cursor for indexer / event-poll loops (`docs/wallet.md` §29 checklist).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct IndexerCursor {
    /// Last processed logical height or sequence (semantics up to the indexer).
    pub last_height: u64,
}

impl IndexerCursor {
    pub fn advance(&mut self, height: u64) {
        self.last_height = self.last_height.max(height);
    }
}

/// Filter for intents whose `tool_class` bit is set in `tool_class_mask`
/// (`0` = accept any class).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IntentPollFilter {
    pub tool_class_mask: u64,
}

impl IntentPollFilter {
    pub fn matches_intent(&self, intent: &ToolIntent) -> bool {
        let bit = intent.body.tool_class.bit();
        self.tool_class_mask == 0 || (self.tool_class_mask & bit) != 0
    }
}
