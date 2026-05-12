use borsh::{BorshDeserialize, BorshSerialize};
use std::collections::BTreeMap;

use crate::error::ExecError;
use crate::tx::{NativeCall, Transaction, TxBody, VmKind};

pub type Address = [u8; 20];

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct Account {
    pub nonce: u64,
    pub balance: u128,
}

/// Unified execution state (accounts + mocked native subtrees).
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct State {
    pub accounts: BTreeMap<Address, Account>,
    /// Mock `SETTLE_BATCH`: `batch_id` → number of receipts recorded.
    pub settled_batches: BTreeMap<u64, u32>,
    /// Mock `REGISTER_AGENT`: monotonic id assignment.
    pub next_agent_id: u64,
    pub agents_by_id: BTreeMap<u64, Address>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            accounts: BTreeMap::new(),
            settled_batches: BTreeMap::new(),
            next_agent_id: 1,
            agents_by_id: BTreeMap::new(),
        }
    }
}

impl State {
    pub fn apply_transaction(&mut self, tx: &Transaction) -> Result<(), ExecError> {
        let signer = tx.signer;
        let account = self.accounts.get(&signer).ok_or(ExecError::UnknownSigner)?;
        if account.nonce != tx.nonce {
            return Err(ExecError::BadNonce {
                expected: account.nonce,
                actual: tx.nonce,
            });
        }

        match (&tx.vm, &tx.body) {
            (VmKind::Native, TxBody::Native(call)) => self.apply_native(signer, call),
            (VmKind::Evm, TxBody::Transfer { to, amount }) => self.apply_transfer(signer, *to, *amount),
            _ => Err(ExecError::InvalidShape),
        }
    }

    fn bump_nonce(&mut self, signer: Address) {
        let a = self.accounts.get_mut(&signer).expect("signer exists");
        a.nonce = a.nonce.saturating_add(1);
    }

    fn apply_transfer(&mut self, from: Address, to: Address, amount: u128) -> Result<(), ExecError> {
        {
            let from_acc = self.accounts.get(&from).ok_or(ExecError::UnknownSigner)?;
            if from_acc.balance < amount {
                return Err(ExecError::InsufficientBalance);
            }
        }
        {
            let from_acc = self.accounts.get_mut(&from).expect("from exists");
            from_acc.balance -= amount;
        }
        self.accounts.entry(to).or_insert(Account { nonce: 0, balance: 0 }).balance += amount;
        self.bump_nonce(from);
        Ok(())
    }

    fn apply_native(&mut self, signer: Address, call: &NativeCall) -> Result<(), ExecError> {
        match call {
            NativeCall::RegisterAgent => {
                let id = self.next_agent_id;
                self.next_agent_id = self.next_agent_id.saturating_add(1);
                self.agents_by_id.insert(id, signer);
                self.bump_nonce(signer);
                Ok(())
            }
            NativeCall::SettleBatch { batch_id, receipt_count } => {
                self.settled_batches.insert(*batch_id, *receipt_count);
                self.bump_nonce(signer);
                Ok(())
            }
            NativeCall::NoOp => {
                self.bump_nonce(signer);
                Ok(())
            }
        }
    }
}
