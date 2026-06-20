//! Challenge-bond state machine package.
//!
//! Track a challenge/dispute bond stake: post, slash on dismissed/withdrawn,
//! release on upheld. Local types, logical (non-wall-clock) settlement.

use crate::error::Error;

/// Challenge bond settlement state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BondState {
    /// Bond has been posted and is unsettled.
    Posted,
    /// Bond was slashed.
    Slashed,
    /// Bond was released back to the poster.
    Released,
}

/// Challenge or dispute bond.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChallengeBond {
    /// Account or identity that posted the bond.
    pub poster: String,
    /// Bond amount.
    pub amount: u64,
    /// Current settlement state.
    pub state: BondState,
}

impl ChallengeBond {
    /// Post a new unsettled bond.
    pub fn post(poster: impl Into<String>, amount: u64) -> Self {
        Self {
            poster: poster.into(),
            amount,
            state: BondState::Posted,
        }
    }

    /// Slash a posted bond.
    pub fn slash(&mut self) -> crate::Result<()> {
        self.settle(BondState::Slashed)
    }

    /// Release a posted bond.
    pub fn release(&mut self) -> crate::Result<()> {
        self.settle(BondState::Released)
    }

    fn settle(&mut self, next: BondState) -> crate::Result<()> {
        if self.state != BondState::Posted {
            return Err(Error::ProtocolViolation(format!(
                "challenge bond already settled as {:?}",
                self.state
            )));
        }
        self.state = next;
        Ok(())
    }
}
