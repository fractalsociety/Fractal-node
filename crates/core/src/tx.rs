use borsh::{BorshDeserialize, BorshSerialize};

use crate::address::Address;
use crate::native_types::{
    OnChainTaskReceipt, ProviderRegistration, ProviderSlashRecord, SettleBatchPayload,
    WalletToolBatchSettlePayload,
};
use crate::native_types::{WalletBudgetSeed, WalletEmergencyScopeV1};
use fractal_crypto::Hash256;

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub enum VmKind {
    Native,
    Evm,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub enum NativeCall {
    RegisterAgent {
        operator: Address,
        pubkey: [u8; 32],
        kind: u8,
        metadata_uri: String,
    },
    UpdateAgent {
        agent_id: u64,
        new_metadata_uri: String,
        new_pubkey: Option<[u8; 32]>,
    },
    SuspendAgent {
        agent_id: u64,
        reason: String,
    },
    SettleReceipt(OnChainTaskReceipt),
    SettleBatch(SettleBatchPayload),
    ClaimPayout {
        batch_id: fractal_crypto::Hash256,
        account: Address,
        amount: u128,
        leaf_index: u32,
        proof: Vec<fractal_crypto::Hash256>,
    },
    FileDispute {
        receipt_id: fractal_crypto::Hash256,
        reason_code: u32,
        evidence_hash: fractal_crypto::Hash256,
    },
    ResolveDispute {
        dispute_id: u64,
        resolution: u8,
        payouts_diff: i128,
    },
    Stake {
        amount: u128,
    },
    Unstake {
        amount: u128,
    },
    Slash {
        validator_id: Address,
        evidence_hash: fractal_crypto::Hash256,
    },
    /// PRD §12.3 `DELEGATE`: bond liquid FRAC to a validator fingerprint (same accounting as
    /// [`DepositConsensusStake`] but the canonical delegation opcode).
    Delegate {
        validator_fingerprint: [u8; 32],
        amount: u128,
    },
    /// PRD §12.3 `WITHDRAW_REWARDS`: liquidate accrued commission / rewards for this delegation.
    WithdrawRewards {
        validator_fingerprint: [u8; 32],
    },
    NoOp,
    /// Anchor `keccak256(borsh(TaskReceipt))` (see `wallet_anchor::task_receipt_commitment`).
    /// Empty `receipt_witness`: stores commitment under signer (dev trust). Non-empty witness:
    /// requires `fractal-core` `--features wallet` and must deserialize to a matching `TaskReceipt`.
    WalletTaskReceiptAnchorV1 {
        commitment: Hash256,
        receipt_witness: Vec<u8>,
    },
    /// PRD §12 / M7: bond FRAC to a validator identity (`BlockHeader.proposer` fingerprint).
    /// Any funded account may deposit; tracked per `(signer, fingerprint)` for withdrawal.
    DepositConsensusStake {
        validator_fingerprint: [u8; 32],
        amount: u128,
    },
    /// Withdraw the caller's own [`DepositConsensusStake`] for this fingerprint.
    WithdrawConsensusStake {
        validator_fingerprint: [u8; 32],
        amount: u128,
    },
    /// Governance-only: register `evidence_hash` before [`NativeCall::SlashConsensusStake`].
    CommitSlashingEvidence {
        evidence_hash: fractal_crypto::Hash256,
    },
    /// Governance-only: burn consensus stake for `validator_fingerprint` after [`CommitSlashingEvidence`].
    SlashConsensusStake {
        validator_fingerprint: [u8; 32],
        evidence_hash: fractal_crypto::Hash256,
    },
    /// Permissionless: slash after native verification of `evidence_borsh`
    /// ([`fractal_bft_wire::ConsensusMisbehaviorEvidenceV1`]). Does not require governance
    /// [`CommitSlashingEvidence`]; evidence hash is derived and stored for replay protection.
    SlashConsensusStakeVerified {
        validator_fingerprint: [u8; 32],
        evidence_borsh: Vec<u8>,
    },
    /// §17 `core::reputation`: store indexer-derived [`fractal_wallet::ReputationLedgerSummary`] (borsh).
    /// Requires `fractal-core` **`--features wallet`**. Governance-only; recomputes score with default params.
    WalletReputationSnapshotV1 {
        provider_id: [u8; 32],
        tool_class: u8,
        summary_borsh: Vec<u8>,
    },
    /// PRD §12.3: validator operator sets commission on rewards (basis points, max 10_000).
    SetValidatorCommission {
        validator_fingerprint: [u8; 32],
        commission_bps: u16,
    },
    /// Mainnet permissionless enrollment after bonded stake ≥ `State.chain_economics.min_validator_stake_wei`.
    RegisterValidator {
        validator_fingerprint: [u8; 32],
        bls_pubkey: [u8; 48],
    },
    /// Move the caller's bonded stake from one validator fingerprint to another (unbonding not required).
    Redelegate {
        from_validator_fingerprint: [u8; 32],
        to_validator_fingerprint: [u8; 32],
        amount: u128,
    },
    /// Governance-only: update on-chain economics profile fields.
    SetChainEconomics {
        min_validator_stake_wei: u128,
        unbonding_period_ms: u64,
        permissionless_validator_entry: bool,
        evm_base_fee_burn: bool,
    },
    /// §14.1 `MintCapability` — register `child_token` and optional `budget_seed` split.
    /// Requires `fractal-core` **`--features wallet`** and `revocation_proof_borsh`
    /// (`fractal_wallet::RevocationVerifyProof` against `wallet_revocation_merkle_root`).
    WalletMintCapabilityV1 {
        parent_cap_id: Option<[u8; 32]>,
        child_token_borsh: Vec<u8>,
        budget_seed: Option<WalletBudgetSeed>,
        revocation_proof_borsh: Vec<u8>,
    },
    /// §14.2 `CreateBudgetAccount` — returns new id via state (`next_wallet_budget_id` before bump).
    WalletCreateBudgetAccountV1 {
        parent: Option<u64>,
        initial_deposit: u128,
    },
    /// §14.2 `FundBudgetAccount`.
    WalletFundBudgetAccountV1 {
        budget: u64,
        amount: u128,
        source_budget: Option<u64>,
    },
    /// §14.2 `CloseBudgetAccount`.
    WalletCloseBudgetAccountV1 {
        budget: u64,
    },
    /// §14.1 `RevokeCapability` — requires `fractal-core` **`--features wallet`**.
    WalletRevokeCapabilityV1 {
        cap_id: [u8; 32],
        reason_code: u8,
        cascade: bool,
        issuer_sig: [u8; 64],
    },
    /// §14.5 `PostTask` — creates a task row and escrows budgets from the poster.
    WalletPostTaskV1 {
        metadata_uri: String,
        bounty_budget: u128,
        tool_budget: u128,
        verifier_budget: u128,
    },
    /// §14.5 `CheckoutTask`.
    WalletCheckoutTaskV1 {
        task_id: u64,
        agent_session: [u8; 32],
        expiry_ms: u64,
    },
    /// §14.5 `RenewCheckout`.
    WalletRenewCheckoutV1 {
        task_id: u64,
        evidence_uri: String,
        new_expiry_ms: u64,
    },
    /// §14.5 `SubmitTask`.
    WalletSubmitTaskV1 {
        task_id: u64,
        artifact_pointer: String,
        tool_receipt_root: Hash256,
    },
    /// §14.5 `VerifyTask` (verifier must differ from checkout signer).
    WalletVerifyTaskV1 {
        task_id: u64,
        verifier_sig: [u8; 64],
        score: u8,
    },
    /// §14.5 `FinalizeTask` — pays escrow to checkout signer; permissionless relayer OK.
    WalletFinalizeTaskV1 {
        task_id: u64,
    },
    /// §14.1 / §29: governance kill-switch (global v1). When engaged, blocks new wallet mints,
    /// budget creates/funds, task progress ops, and receipt anchors; revokes, budget close, and
    /// task finalize remain permitted.
    WalletEmergencyStopV1 {
        engage: bool,
    },
    /// §16.3 wallet-native multi–tool-receipt batch (`docs/wallet.md`; not M3 `SettleBatch`).
    WalletBatchSettleV1(WalletToolBatchSettlePayload),
    /// §14.4 `RegisterProvider`.
    WalletRegisterProviderV1 {
        registration: ProviderRegistration,
    },
    /// §14.4 `StakeForClass`.
    WalletStakeForClassV1 {
        provider_id: [u8; 32],
        tool_class: u8,
        amount: u128,
    },
    /// §14.4 `UnstakeRequest`.
    WalletProviderUnstakeRequestV1 {
        provider_id: [u8; 32],
        tool_class: u8,
        amount: u128,
    },
    /// §14.4 `UnstakeFinalize`.
    WalletProviderUnstakeFinalizeV1 {
        request_id: u64,
    },
    /// §14.4 `SlashProvider`.
    WalletSlashProviderV1 {
        provider_id: [u8; 32],
        slash: ProviderSlashRecord,
    },
    /// §14.4 `UpdateProvider`.
    WalletUpdateProviderV1 {
        provider_id: [u8; 32],
        metadata_uri: String,
        endpoint_uri: String,
        active: bool,
    },
    /// §14.4 `DeregisterProvider`.
    WalletDeregisterProviderV1 {
        provider_id: [u8; 32],
    },
    /// §14.1 scoped master-wallet stop. `master_sig` signs
    /// `WalletScopedEmergencyStopSignBodyV1 { chain_id, engage, scope }` with `master_public_key`.
    WalletScopedEmergencyStopV1 {
        engage: bool,
        scope: WalletEmergencyScopeV1,
        master_public_key: [u8; 32],
        master_sig: [u8; 64],
    },
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub enum TxBody {
    Transfer {
        to: Address,
        amount: u128,
    },
    Native(NativeCall),
    /// Minimal EVM call (M4): execute EVM bytecode and/or precompiles.
    /// `gas_limit` is an execution cap; actual gas accounting is handled separately.
    EvmCall {
        to: Address,
        value: u128,
        calldata: Vec<u8>,
        gas_limit: u64,
    },
    /// Minimal EVM CREATE (M4): store deployed code deterministically.
    /// `init_code` is treated as "runtime code" for devnet until full EVM init execution lands.
    EvmCreate {
        value: u128,
        init_code: Vec<u8>,
        gas_limit: u64,
    },
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct Transaction {
    pub signer: Address,
    pub nonce: u64,
    pub vm: VmKind,
    pub body: TxBody,
}
