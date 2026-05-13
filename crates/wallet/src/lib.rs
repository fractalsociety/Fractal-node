//! FractalWork **Agent Wallet & Tool Market** primitives (spec `docs/wallet.md` v2.0).
//!
//! Implements Phase 1 checklist pieces (W1–W6): capabilities, budgets, rate limits,
//! revocation Merkle, tool market (trusted + optimistic + disputes), policy templates,
//! emergency stop, task receipt binding, and **provider id** helper for SDK re-exports.

pub mod budget;
pub mod capability;
pub mod caveat;
pub mod challenge;
pub mod emergency;
pub mod market;
pub mod merkle;
pub mod policy;
pub mod rate_limit;
pub mod revocation;
pub mod task_receipt;
pub mod types;

pub use budget::{BudgetAccount, BudgetAccountId, BudgetError};
pub use capability::{CapabilityId, CapabilityToken, CapabilityVerifyError};
pub use caveat::Caveat;
pub use challenge::{AdjudicationDecision, Challenge, ChallengeId, ChallengeKind};
pub use emergency::{EmergencyLevel, EmergencyRegistry};
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
pub use revocation::{RevocationEntry, RevocationError, RevocationSet};
pub use task_receipt::{
    build_task_receipt, summary_commitment, tool_receipt_root, verify_tool_costs, TaskReceipt,
    TaskReceiptBuildError, ToolReceiptSummary,
};
pub use types::{
    Amount, IntentId, ProviderId, PublicKey, QuoteId, ReceiptId, TaskId, TeeType, TimestampMs,
    ToolClass, VerificationTier, WorkspaceId,
};
