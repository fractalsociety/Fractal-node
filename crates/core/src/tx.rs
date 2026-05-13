use borsh::{BorshDeserialize, BorshSerialize};

use crate::address::Address;
use crate::native_types::{OnChainTaskReceipt, SettleBatchPayload};
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
    Delegate {
        validator: Address,
        amount: u128,
    },
    WithdrawRewards {
        validator: Address,
    },
    NoOp,
    /// Anchor `keccak256(borsh(TaskReceipt))` (see `wallet_anchor::task_receipt_commitment`).
    /// Empty `receipt_witness`: stores commitment under signer (dev trust). Non-empty witness:
    /// requires `fractal-core` `--features wallet` and must deserialize to a matching `TaskReceipt`.
    WalletTaskReceiptAnchorV1 {
        commitment: Hash256,
        receipt_witness: Vec<u8>,
    },
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub enum TxBody {
    Transfer { to: Address, amount: u128 },
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
