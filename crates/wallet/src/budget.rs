//! Budget accounts with RESERVE / SETTLE / REFUND / PARTIAL (`docs/wallet.md` §6).

use std::collections::BTreeMap;

use thiserror::Error;

use crate::types::{Amount, ToolClass};

pub type BudgetAccountId = u64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BudgetAccount {
    pub id: BudgetAccountId,
    pub parent: Option<BudgetAccountId>,
    pub total_deposited: Amount,
    pub reserved: Amount,
    pub spent: Amount,
    pub per_tool_caps: BTreeMap<ToolClass, Amount>,
    pub nonce: u64,
}

impl BudgetAccount {
    pub fn new(id: BudgetAccountId, parent: Option<BudgetAccountId>, total_deposited: Amount) -> Self {
        Self {
            id,
            parent,
            total_deposited,
            reserved: 0,
            spent: 0,
            per_tool_caps: BTreeMap::new(),
            nonce: 0,
        }
    }

    pub fn available(&self) -> Amount {
        self.total_deposited.saturating_sub(self.reserved).saturating_sub(self.spent)
    }

    pub fn deposit(&mut self, amount: Amount) {
        self.total_deposited = self.total_deposited.saturating_add(amount);
    }

    /// §6.3 — atomic reserve for matched intent.
    pub fn reserve(&mut self, class: ToolClass, amount: Amount) -> Result<(), BudgetError> {
        if let Some(cap) = self.per_tool_caps.get(&class) {
            if amount > *cap {
                return Err(BudgetError::PerToolCapExceeded);
            }
        }
        if amount > self.available() {
            return Err(BudgetError::InsufficientAvailable);
        }
        self.reserved = self.reserved.saturating_add(amount);
        self.nonce = self.nonce.saturating_add(1);
        Ok(())
    }

    pub fn settle(&mut self, amount: Amount) -> Result<(), BudgetError> {
        if amount > self.reserved {
            return Err(BudgetError::InsufficientReserved);
        }
        self.reserved -= amount;
        self.spent = self.spent.saturating_add(amount);
        self.nonce = self.nonce.saturating_add(1);
        Ok(())
    }

    pub fn refund(&mut self, amount: Amount) -> Result<(), BudgetError> {
        if amount > self.reserved {
            return Err(BudgetError::InsufficientReserved);
        }
        self.reserved -= amount;
        self.nonce = self.nonce.saturating_add(1);
        Ok(())
    }

    /// Partial settle: move `settle` from reserved → spent, remainder reserved → available.
    pub fn partial_settle(&mut self, settle: Amount, reserved_total: Amount) -> Result<(), BudgetError> {
        if reserved_total > self.reserved {
            return Err(BudgetError::InsufficientReserved);
        }
        if settle > reserved_total {
            return Err(BudgetError::InsufficientReserved);
        }
        self.reserved -= reserved_total;
        self.spent = self.spent.saturating_add(settle);
        // remainder (reserved_total - settle) already returned to available because we dropped full reserved_total from self.reserved and only added settle to spent
        let _refund = reserved_total - settle;
        self.nonce = self.nonce.saturating_add(1);
        Ok(())
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum BudgetError {
    #[error("insufficient available balance for reservation")]
    InsufficientAvailable,
    #[error("insufficient reserved balance")]
    InsufficientReserved,
    #[error("per-tool cap exceeded")]
    PerToolCapExceeded,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserve_settle_refund_serial() {
        let mut b = BudgetAccount::new(1, None, 1000);
        b.per_tool_caps.insert(ToolClass::Browser, 500);
        b.reserve(ToolClass::Browser, 200).unwrap();
        assert_eq!(b.available(), 800);
        b.settle(200).unwrap();
        assert_eq!(b.reserved, 0);
        assert_eq!(b.spent, 200);
        b.reserve(ToolClass::Browser, 100).unwrap();
        b.refund(100).unwrap();
        assert_eq!(b.available(), 800);
    }

    #[test]
    fn partial_settle() {
        let mut b = BudgetAccount::new(1, None, 1000);
        b.reserve(ToolClass::LlmInference, 300).unwrap();
        b.partial_settle(100, 300).unwrap();
        assert_eq!(b.reserved, 0);
        assert_eq!(b.spent, 100);
        assert_eq!(b.available(), 900);
    }
}
