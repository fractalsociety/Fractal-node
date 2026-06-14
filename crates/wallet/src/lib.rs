//! FractalWork **Agent Wallet & Tool Market** primitives (spec `docs/wallet.md` v2.0).
//!
//! Implements Phase 1 checklist pieces (W1–W6): capabilities, budgets, rate limits,
//! revocation Merkle, tool market (trusted + optimistic + disputes), policy templates,
//! emergency stop, **§9.1 `ToolReceipt`** + task receipt binding (§9.2), and **provider id** helper for SDK re-exports.

pub mod budget;
pub mod capability;
pub mod caveat;
pub mod challenge;
pub mod emergency;
pub mod finality_warning;
pub mod market;
pub mod merkle;
pub mod policy;
pub mod rate_limit;
pub mod reputation;
pub mod revocation;
pub mod task_receipt;
pub mod types;

pub use budget::{BudgetAccount, BudgetAccountId, BudgetError};
pub use capability::{CapabilityId, CapabilityToken, CapabilityVerifyError};
pub use caveat::Caveat;
pub use challenge::{AdjudicationDecision, Challenge, ChallengeId, ChallengeKind};
pub use emergency::{EmergencyLevel, EmergencyRegistry};
pub use finality_warning::{
    warn_if_high_value_soft_final, HighValueFinalityPolicy, WalletFinalityStatus,
    WalletFinalityWarning,
};
pub use market::{
    provider_id_from_public_key, ChallengeError, DeliveredInfo, IntentState, MatchError,
    PostIntentError, PostReceiptError, ProviderStake, Quote, QuoteBody, ResolveError, SettleError,
    SigVerifyError, ToolIntent, ToolIntentBody, ToolMarket, DEFAULT_OPTIMISTIC_CHALLENGE_MS,
};
pub use policy::{
    builtins as policy_builtins, BudgetSpec, PolicyError, PolicyRegistry, PolicyTemplate,
    RateLimitSpec, ResolvedPolicy, SemVer, TemplateId,
};
pub use rate_limit::{RateLimitError, TokenBucket};
pub use reputation::{
    bootstrap_stake_multiplier_bps, compute_reputation_score_milli, reputation_stake_product,
    select_quote, BootstrapStakeParams, QuoteCandidate, QuoteSelectionError, QuoteSelectionGates,
    QuoteSelectionPreference, ReputationLedgerSummary, ReputationMilli, ReputationParams,
    SettlementEvent,
};
pub use revocation::{RevocationEntry, RevocationError, RevocationSet};
pub use task_receipt::{
    build_task_receipt, derive_tool_receipt_id, tool_receipt_leaf_commitment, tool_receipt_root,
    verify_tool_receipt_costs, MeteringRecord, TaskReceipt, TaskReceiptBuildError, TeeAttestation,
    ToolReceipt, ToolReceiptAgentAckBody, ToolReceiptBody, ToolReceiptVerifyError,
};
pub use types::{
    Amount, IntentId, ProviderId, PublicKey, QuoteId, ReceiptId, TaskId, TeeType, TimestampMs,
    ToolClass, VerificationTier, WorkspaceId,
};

pub fn verify_capability_with_revocation(
    token: &CapabilityToken,
    now_ms: TimestampMs,
    _revocation_root: &[u8; 32],
    _ancestor_chain: &[CapabilityId],
    _proof: &[[u8; 32]],
) -> Result<(), capability::CapabilityVerifyError> {
    token.verify_time(now_ms)?;
    token.verify_autonomous_tool_mask()
}
