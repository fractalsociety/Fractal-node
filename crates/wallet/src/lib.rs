//! FractalWork **Agent Wallet & Tool Market** primitives (spec `docs/wallet.md` v2.0).
//!
//! Implements Phase 1 checklist pieces (W1–W6): capabilities, budgets, rate limits,
//! revocation Merkle, tool market (trusted + optimistic + disputes), policy templates,
//! emergency stop, **§9.1 `ToolReceipt`** + task receipt binding (§9.2), **provider id** helper,
//! **tool-provider stake** (`ProviderStake`: lock at match, `burn_locked` on adjudication loss),
//! **§10.4 derived reputation** + §7.4 quote selection (`reputation`), and **§10.4 ledger sync**
//! from [`reputation_market::ToolMarketWithReputation`] over [`market::ToolMarket`].

pub mod batch_settle;
pub mod budget;
pub mod capability;
pub mod caveat;
pub mod challenge;
pub mod delegation;
pub mod emergency;
pub mod market;
pub mod merkle;
pub mod policy;
pub mod production;
pub mod rate_limit;
pub mod reputation;
pub mod reputation_market;
pub mod revocation;
pub mod smt;
pub mod task_receipt;
pub mod types;

pub use batch_settle::{
    prepare_wallet_batch_receipts, sign_wallet_tool_batch, verify_wallet_batch_receipts,
    verify_wallet_tool_batch_sig, wallet_tool_batch_sign_message, WalletBatchSettleBuildError,
    WalletBatchSettleSigError, WalletBatchSettleVerifyError,
};
pub use budget::{BudgetAccount, BudgetAccountId, BudgetError};
pub use capability::{CapabilityId, CapabilityToken, CapabilityVerifyError};
pub use caveat::Caveat;
pub use challenge::{AdjudicationDecision, Challenge, ChallengeId, ChallengeKind};
pub use delegation::{
    allocate_budget_and_build_delegated_child_body, build_delegated_child_body,
    parent_allows_sub_agent_delegation, tightest_max_total_spend, DelegatedChildParams,
    DelegationError,
};
pub use delegation::session::{
    capability_allows_tool, child_params_for_role, delegate_sub_agent_production,
    run_verifier_tool_session_e2e, sessions_are_distinct, verify_delegation_pair,
    DelegationVerificationReport, ProductionDelegationBundle, SessionE2eError, SubAgentRole,
    VerifierSessionOutcome,
};
pub use emergency::{EmergencyLevel, EmergencyRegistry};
pub use market::{
    provider_id_from_onchain_worker_agent, provider_id_from_public_key,
    provider_verify_intent_capability, AgentCapabilityPresentation, ChallengeError, DeliveredInfo,
    IntentState, MatchError, PostIntentError, PostReceiptError, ProviderCapabilityVerifyError,
    ProviderStake, Quote, QuoteBody, ResolveError, SettleError, SigVerifyError, ToolIntent,
    ToolIntentBody, ToolMarket, DEFAULT_OPTIMISTIC_CHALLENGE_MS,
};
pub use policy::{
    builtins as policy_builtins, BudgetSpec, PolicyError, PolicyRegistry, PolicyTemplate,
    RateLimitSpec, ResolvedPolicy, SemVer, TemplateId,
};
pub use production::{
    challenge_kind_for_production_failure, decode_tee_quote_v1, encode_tee_quote_v1,
    select_verifier_weighted, should_sample_verifier, verify_production_tool_receipt,
    verify_tee_attestation, ClassVerificationMethod, MeteringRequirements, ProductionVerifyContext,
    ProductionVerifyError, ProductionVerifyReport, TeeAttestationError, TeeQuoteV1,
    VerifierCandidate, VerifierSamplingConfig, TEE_QUOTE_MAGIC,
};
pub use rate_limit::{RateLimitError, TokenBucket};
pub use reputation::{
    bootstrap_stake_multiplier_bps, compute_reputation_score_milli, recency_factor_micro,
    reputation_stake_product, select_quote, BootstrapStakeParams, QuoteCandidate,
    QuoteSelectionError, QuoteSelectionGates, QuoteSelectionPreference, ReputationLedgerSummary,
    ReputationMilli, ReputationParams, SettlementEvent,
};
pub use reputation_market::ToolMarketWithReputation;
pub use merkle::{
    verify_non_membership_commitments_compact, NeighborWitness, SortedNonMembershipProof,
};
pub use smt::{
    empty_tree_root, verify_membership as verify_smt_membership,
    verify_non_membership as verify_smt_non_membership, RevocationSparseTrie, SmtMembershipProof,
    SmtNonMembershipProof, SMT_KEY_BITS,
};
pub use revocation::{
    revocation_leaf_commitment, verify_capability_with_revocation,
    verify_proof_bundle, CapabilityRevocationVerifyError, RevocationAncestorWitness,
    RevocationEntry, RevocationError, RevocationProofError, RevocationSet,
    RevocationVerifyProof,
};
pub use task_receipt::{
    build_task_receipt, derive_tool_receipt_id, tool_receipt_leaf_commitment, tool_receipt_root,
    verify_tool_receipt_costs, MeteringRecord, TaskReceipt, TaskReceiptBuildError, TeeAttestation,
    ToolReceipt, ToolReceiptAgentAckBody, ToolReceiptBody, ToolReceiptVerifyError,
};
pub use types::{
    Amount, IntentId, ProviderId, PublicKey, QuoteId, ReceiptId, TaskId, TeeType, TimestampMs,
    ToolClass, VerificationTier, WorkspaceId,
};
