//! Appeals-flow package.
//!
//! Models a guarded appeal lifecycle from filing through review and resolution.

use crate::error::Error;

/// Lifecycle state for an appeal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppealState {
    /// Appeal has been filed and is waiting for review.
    Filed,
    /// Appeal is actively under review.
    UnderReview,
    /// Appeal has been resolved.
    Resolved {
        /// Whether the appeal was upheld.
        upheld: bool,
    },
}

/// Appeal record with public resolution reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Appeal {
    /// Stable appeal identifier.
    pub id: String,
    /// Current appeal state.
    pub state: AppealState,
    /// Resolution reason, present only after resolution.
    pub reason: Option<String>,
}

impl Appeal {
    /// File a new appeal.
    pub fn file(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            state: AppealState::Filed,
            reason: None,
        }
    }

    /// Begin reviewing a filed appeal.
    pub fn begin_review(&mut self) -> crate::Result<()> {
        if self.state != AppealState::Filed {
            return Err(Error::ProtocolViolation(format!(
                "cannot begin review for appeal '{}' from state {:?}",
                self.id, self.state
            )));
        }
        self.state = AppealState::UnderReview;
        Ok(())
    }

    /// Resolve an appeal that is under review and store the public reason.
    pub fn resolve(&mut self, upheld: bool, reason: impl Into<String>) -> crate::Result<()> {
        if self.state != AppealState::UnderReview {
            return Err(Error::ProtocolViolation(format!(
                "cannot resolve appeal '{}' from state {:?}",
                self.id, self.state
            )));
        }
        self.state = AppealState::Resolved { upheld };
        self.reason = Some(reason.into());
        Ok(())
    }
}
