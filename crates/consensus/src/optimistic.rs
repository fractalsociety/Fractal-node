//! Optimistic execution buffer with rollback (`docs/prd.md` §7.9.1).

use fractal_core::State;

/// Speculative state for HyperBFT propose while commit lags (§7.9.3).
#[derive(Debug, Clone)]
pub struct OptimisticExecution {
    /// Last committed execution state (rollback target).
    pub checkpoint: State,
    /// Scratch used for speculative block building.
    pub scratch: State,
}

impl Default for OptimisticExecution {
    fn default() -> Self {
        let s = State::default();
        Self {
            checkpoint: s.clone(),
            scratch: s,
        }
    }
}

impl OptimisticExecution {
    #[must_use]
    pub fn new(committed: &State) -> Self {
        Self {
            checkpoint: committed.clone(),
            scratch: committed.clone(),
        }
    }

    /// Reset scratch from the committed checkpoint before a new propose attempt.
    pub fn prepare_propose(&mut self, committed: &State) {
        self.checkpoint = committed.clone();
        self.scratch = committed.clone();
    }

    /// Discard speculative work after a failed QC / invalid proposal.
    pub fn rollback(&mut self) {
        self.scratch = self.checkpoint.clone();
    }

    /// Advance checkpoint after a block is committed on-chain.
    pub fn commit_through(&mut self, committed: &State) {
        self.checkpoint = committed.clone();
        self.scratch = committed.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fractal_core::{Account, HARDHAT_DEFAULT_SIGNER_0};

    #[test]
    fn rollback_restores_checkpoint() {
        let mut committed = State::default();
        committed.accounts.insert(
            HARDHAT_DEFAULT_SIGNER_0,
            Account {
                nonce: 0,
                balance: 100,
            },
        );
        let mut opt = OptimisticExecution::new(&committed);
        opt.scratch.accounts.get_mut(&HARDHAT_DEFAULT_SIGNER_0).unwrap().balance = 50;
        opt.rollback();
        assert_eq!(
            opt.scratch.accounts.get(&HARDHAT_DEFAULT_SIGNER_0).unwrap().balance,
            100
        );
    }
}
