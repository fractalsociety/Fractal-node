//! Execution-budget package.
//!
//! Track a deterministic execution resource budget (steps/calls) with
//! consume/allow semantics for sandbox-style limits.

/// Deterministic resource budget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionBudget {
    /// Maximum units that may be consumed.
    pub limit: u64,
    /// Units already consumed.
    pub used: u64,
}

impl ExecutionBudget {
    /// Create a new budget with zero usage.
    pub fn new(limit: u64) -> Self {
        Self { limit, used: 0 }
    }

    /// Consume `n` units, returning `false` without mutating if it would exceed
    /// the limit.
    pub fn consume(&mut self, n: u64) -> bool {
        let Some(next_used) = self.used.checked_add(n) else {
            return false;
        };
        if next_used > self.limit {
            return false;
        }
        self.used = next_used;
        true
    }

    /// Units remaining before the budget is exhausted.
    pub fn remaining(&self) -> u64 {
        self.limit.saturating_sub(self.used)
    }

    /// Return true when no more units remain.
    pub fn exhausted(&self) -> bool {
        self.used >= self.limit
    }
}
