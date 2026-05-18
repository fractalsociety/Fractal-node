//! On-chain native shapes for PRD §9.2 / §18 M3 (compact borsh).

use borsh::{BorshDeserialize, BorshSerialize};

use crate::address::Address;
use fractal_crypto::Hash256;

/// Native opcode bytes (PRD §9.2 table).
pub const OP_REGISTER_AGENT: u8 = 0x01;
pub const OP_UPDATE_AGENT: u8 = 0x02;
pub const OP_SUSPEND_AGENT: u8 = 0x03;
pub const OP_SETTLE_RECEIPT: u8 = 0x04;
pub const OP_SETTLE_BATCH: u8 = 0x05;
pub const OP_CLAIM_PAYOUT: u8 = 0x06;
pub const OP_FILE_DISPUTE: u8 = 0x07;
pub const OP_RESOLVE_DISPUTE: u8 = 0x08;
pub const OP_STAKE: u8 = 0x09;
pub const OP_UNSTAKE: u8 = 0x0a;
pub const OP_SLASH: u8 = 0x0b;
pub const OP_DELEGATE: u8 = 0x0c;
pub const OP_WITHDRAW_REWARDS: u8 = 0x0d;
/// W6-d: anchor `task_receipt_commitment` on-chain (`docs/wallet.md` §9.2, `wallet_anchor`).
pub const OP_WALLET_TASK_RECEIPT_ANCHOR_V1: u8 = 0x0e;
/// §14.1 `MintCapability` (`WalletMintCapabilityV1`).
pub const OP_WALLET_MINT_CAPABILITY_V1: u8 = 0x0f;
/// §14.2 `CreateBudgetAccount` (`WalletCreateBudgetAccountV1`).
pub const OP_WALLET_CREATE_BUDGET_V1: u8 = 0x10;
/// §14.2 `FundBudgetAccount` (`WalletFundBudgetAccountV1`).
pub const OP_WALLET_FUND_BUDGET_V1: u8 = 0x11;
/// §14.2 `CloseBudgetAccount` (`WalletCloseBudgetAccountV1`).
pub const OP_WALLET_CLOSE_BUDGET_V1: u8 = 0x12;
/// §14.1 `RevokeCapability` (`WalletRevokeCapabilityV1`).
pub const OP_WALLET_REVOKE_CAPABILITY_V1: u8 = 0x13;
/// §14.5 `PostTask` — unified task product surface (`docs/wallet.md` §29).
pub const OP_WALLET_POST_TASK_V1: u8 = 0x14;
pub const OP_WALLET_CHECKOUT_TASK_V1: u8 = 0x15;
pub const OP_WALLET_RENEW_CHECKOUT_V1: u8 = 0x16;
pub const OP_WALLET_SUBMIT_TASK_V1: u8 = 0x17;
pub const OP_WALLET_VERIFY_TASK_V1: u8 = 0x18;
pub const OP_WALLET_FINALIZE_TASK_V1: u8 = 0x19;
/// §29 governance global emergency stop (`WalletEmergencyStopV1`); wire is always borsh `NativeCall`.
pub const OP_WALLET_EMERGENCY_STOP_V1: u8 = 0x1a;
/// §16.3 wallet-native multi–tool-receipt batch settle (`WalletBatchSettleV1`; not M3 `SettleBatch`).
pub const OP_WALLET_BATCH_SETTLE_V1: u8 = 0x1b;
/// §14.4 `RegisterProvider`.
pub const OP_WALLET_REGISTER_PROVIDER_V1: u8 = 0x1c;
/// §14.4 `StakeForClass`.
pub const OP_WALLET_STAKE_PROVIDER_CLASS_V1: u8 = 0x1d;
/// §14.4 `UnstakeRequest`.
pub const OP_WALLET_PROVIDER_UNSTAKE_REQUEST_V1: u8 = 0x1e;
/// §14.4 `UnstakeFinalize`.
pub const OP_WALLET_PROVIDER_UNSTAKE_FINALIZE_V1: u8 = 0x1f;
/// §14.4 `SlashProvider`.
pub const OP_WALLET_SLASH_PROVIDER_V1: u8 = 0x20;
/// §14.4 `UpdateProvider`.
pub const OP_WALLET_UPDATE_PROVIDER_V1: u8 = 0x21;
/// §14.4 `DeregisterProvider`.
pub const OP_WALLET_DEREGISTER_PROVIDER_V1: u8 = 0x22;
/// §14.1 scoped master-wallet emergency stop (`WalletScopedEmergencyStopV1`).
pub const OP_WALLET_SCOPED_EMERGENCY_STOP_V1: u8 = 0x23;

/// Default §10.2 withdrawal delay for provider stake.
pub const WALLET_PROVIDER_UNSTAKE_DELAY_MS: u64 = 7 * 24 * 60 * 60 * 1000;

/// `docs/wallet.md` §14.5 task row status (on-chain state machine).
pub const WALLET_TASK_POSTED: u8 = 0;
pub const WALLET_TASK_CHECKED_OUT: u8 = 1;
pub const WALLET_TASK_SUBMITTED: u8 = 2;
pub const WALLET_TASK_VERIFIED: u8 = 3;
pub const WALLET_TASK_FINALIZED: u8 = 4;

/// On-chain task row for §14.5 lifecycle (`PostTask` → … → `FinalizeTask`).
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct OnChainTaskRow {
    pub owner: Address,
    pub metadata_uri: String,
    /// `bounty_budget + tool_budget + verifier_budget` locked at post until finalize.
    pub escrow_wei: u128,
    pub status: u8,
    pub posted_at_ms: u64,
    pub agent_session: Option<[u8; 32]>,
    pub checkout_expiry_ms: u64,
    pub checkout_signer: Option<Address>,
    pub artifact_pointer: String,
    pub tool_receipt_root: Hash256,
    pub verifier_sig: Option<[u8; 64]>,
    pub verifier_score: u8,
    pub renew_evidence_uri: String,
}

impl Default for OnChainTaskRow {
    fn default() -> Self {
        Self {
            owner: [0u8; 20],
            metadata_uri: String::new(),
            escrow_wei: 0,
            status: WALLET_TASK_POSTED,
            posted_at_ms: 0,
            agent_session: None,
            checkout_expiry_ms: 0,
            checkout_signer: None,
            artifact_pointer: String::new(),
            tool_receipt_root: [0u8; 32],
            verifier_sig: None,
            verifier_score: 0,
            renew_evidence_uri: String::new(),
        }
    }
}

/// `MintCapability.budget_seed` wire shape (`docs/wallet.md` §14.1).
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct WalletBudgetSeed {
    pub from_budget: u64,
    pub amount: u128,
}

/// On-chain budget row (`docs/wallet.md` §14.2).
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct OnChainBudgetAccount {
    pub id: u64,
    pub parent: Option<u64>,
    pub owner: Address,
    pub total_deposited: u128,
    pub reserved: u128,
    pub spent: u128,
    pub nonce: u64,
}

impl OnChainBudgetAccount {
    #[must_use]
    pub fn available(&self) -> u128 {
        self.total_deposited
            .saturating_sub(self.reserved)
            .saturating_sub(self.spent)
    }
}

/// On-chain revocation row (`docs/wallet.md` §14.1 / §4.6).
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct OnChainRevocationEntry {
    pub revoked_at_ms: u64,
    pub reason_code: u8,
    pub cascade: bool,
}

/// Ed25519-signed payload for [`crate::tx::NativeCall::WalletRevokeCapabilityV1`].
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct WalletRevokeCapabilitySignBody {
    pub cap_id: [u8; 32],
    pub reason_code: u8,
    pub cascade: bool,
    pub chain_id: u32,
}

/// Optional hardening for §14.1 `EmergencyStop { scope, master_sig }`.
///
/// `None` workspace/project/task means any. `tool_class_mask == 0` means any tool class.
/// `provider_id == None` means any provider. A stop covers a capability when all populated
/// fields match and the tool/provider selectors overlap the capability scope.
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct WalletEmergencyScopeV1 {
    pub workspace_id: Option<u64>,
    pub project_id: Option<u64>,
    pub task_id: Option<u64>,
    pub tool_class_mask: u64,
    pub provider_id: Option<Hash256>,
}

impl WalletEmergencyScopeV1 {
    #[must_use]
    pub fn global() -> Self {
        Self {
            workspace_id: None,
            project_id: None,
            task_id: None,
            tool_class_mask: 0,
            provider_id: None,
        }
    }
}

/// Ed25519-signed payload for [`crate::tx::NativeCall::WalletScopedEmergencyStopV1`].
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct WalletScopedEmergencyStopSignBodyV1 {
    pub chain_id: u32,
    pub engage: bool,
    pub scope: WalletEmergencyScopeV1,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct WalletScopedEmergencyStopRecordV1 {
    pub master_public_key: [u8; 32],
    pub scope: WalletEmergencyScopeV1,
    pub engaged_at_ms: u64,
}

/// [`ResolveDispute`] `resolution` value: worker / provider is at fault (indexer + wallet §10.4 slash signal).
pub const DISPUTE_RESOLUTION_PROVIDER_FAULT: u8 = 2;

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct OnChainTaskReceipt {
    pub receipt_id: Hash256,
    pub job_id: Hash256,
    pub requester: Address,
    pub worker: u64,
    pub verifier: u64,
    pub artifact_root: Hash256,
    pub output_hash: Hash256,
    pub score: u8,
    pub payout_amount: u128,
    pub verifier_fee: u128,
    pub protocol_fee: u128,
    pub final_status: u8,
    pub finalized_at: u64,
    pub schema_version: u16,
    /// Wallet tool-class discriminant (`0` = Browser, …). Wire schema v2.
    pub tool_class: u8,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct PayoutEntry {
    pub index: u32,
    pub account: Address,
    pub amount: u128,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct SettleBatchPayload {
    pub batch_id: Hash256,
    pub operator: Address,
    pub receipts: Vec<OnChainTaskReceipt>,
    pub payout_entries: Vec<PayoutEntry>,
    pub submitted_at: u64,
    pub operator_sig: [u8; 64],
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct AgentRecord {
    pub agent_id: u64,
    pub address: Address,
    pub operator: Address,
    pub pubkey: [u8; 32],
    pub kind: u8,
    pub metadata_uri: String,
    pub reputation_score: u32,
    pub completed_jobs: u32,
    pub status: u8,
    pub registered_at: u64,
    pub schema_version: u16,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct StoredBatch {
    pub operator: Address,
    pub receipt_root: Hash256,
    pub payout_root: Hash256,
    pub receipt_count: u32,
    pub payout_count: u32,
    pub total_payout: u128,
    pub submitted_at: u64,
}

/// Wallet §16.3 batched tool-market receipts (distinct from M3 [`SettleBatchPayload`]).
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct WalletToolBatchSettlePayload {
    pub batch_id: Hash256,
    pub provider_id: [u8; 32],
    /// Ed25519 public key; must satisfy `BLAKE3(provider_public_key) == provider_id`.
    pub provider_public_key: [u8; 32],
    pub tool_class: u8,
    pub receipt_root: Hash256,
    pub total_cost: u128,
    /// On-chain payout recipient for `total_cost` (escrow relayer debits, provider receives).
    pub payout_to: Address,
    /// `borsh(fractal_wallet::ToolReceipt)` rows committed under `receipt_root`.
    pub receipts_borsh: Vec<Vec<u8>>,
    pub submitted_at: u64,
    pub provider_batch_sig: [u8; 64],
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct StoredWalletToolBatch {
    pub relayer: Address,
    pub provider_id: [u8; 32],
    pub tool_class: u8,
    pub receipt_root: Hash256,
    pub receipt_count: u32,
    pub total_cost: u128,
    pub payout_to: Address,
    pub submitted_at: u64,
}

/// Provider registration row (`docs/wallet.md` §10.5 / §14.4).
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct ProviderRegistration {
    pub provider_id: Hash256,
    pub owner: Address,
    pub public_key: [u8; 32],
    pub encryption_pubkey: [u8; 32],
    pub metadata_uri: String,
    pub endpoint_uri: String,
    pub tool_classes: Vec<u8>,
    pub tee_attestation_hash: Option<Hash256>,
    pub registration_bond: u128,
}

/// On-chain provider identity row.
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct OnChainProviderRow {
    pub registration: ProviderRegistration,
    pub registered_at_ms: u64,
    pub updated_at_ms: u64,
    pub active: bool,
}

/// Per `(provider, tool_class)` bonded stake row.
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct OnChainProviderStakeRow {
    pub total: u128,
    pub available: u128,
    pub pending_unstake: u128,
    pub slashed_total: u128,
}

/// Pending provider unstake request; stake remains slashable until finalized.
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct OnChainProviderUnstakeRequest {
    pub request_id: u64,
    pub provider_id: Hash256,
    pub tool_class: u8,
    pub owner: Address,
    pub amount: u128,
    pub requested_at_ms: u64,
    pub release_ms: u64,
}

/// Governance-issued provider slash record (`docs/wallet.md` §10.3 / §14.4).
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct ProviderSlashRecord {
    pub tool_class: u8,
    pub amount: u128,
    pub reason_code: u8,
    pub evidence_hash: Hash256,
    pub challenger: Address,
}

/// Stored slash history row.
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct OnChainProviderSlashRecord {
    pub provider_id: Hash256,
    pub tool_class: u8,
    pub requested_amount: u128,
    pub burned_amount: u128,
    pub reason_code: u8,
    pub evidence_hash: Hash256,
    pub challenger: Address,
    pub slashed_at_ms: u64,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct DisputeRecord {
    pub receipt_id: Hash256,
    pub filer: Address,
    pub reason_code: u32,
    pub evidence_hash: Hash256,
    pub status: u8,
}
