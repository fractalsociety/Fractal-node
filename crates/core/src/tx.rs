use borsh::{BorshDeserialize, BorshSerialize};

use crate::state::Address;

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub enum VmKind {
    Native,
    Evm,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub enum NativeCall {
    RegisterAgent,
    SettleBatch { batch_id: u64, receipt_count: u32 },
    NoOp,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub enum TxBody {
    Transfer { to: Address, amount: u128 },
    Native(NativeCall),
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct Transaction {
    pub signer: Address,
    pub nonce: u64,
    pub vm: VmKind,
    pub body: TxBody,
}
