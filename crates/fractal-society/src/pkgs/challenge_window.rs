//! Challenge-window package.
//!
//! Track a challenge window over logical (non-wall-clock) time: open/closed
//! status relative to a deadline (distinct from challenge_bond, which tracks
//! stakes).

/// Logical-time challenge window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChallengeWindow {
    /// Logical opening timestamp.
    pub opened_at: u64,
    /// Window duration.
    pub duration: u64,
}

impl ChallengeWindow {
    /// Create a new challenge window.
    pub fn new(opened_at: u64, duration: u64) -> Self {
        Self {
            opened_at,
            duration,
        }
    }

    /// Logical deadline, saturating on overflow.
    pub fn deadline(&self) -> u64 {
        self.opened_at.saturating_add(self.duration)
    }

    /// Return true while `now` is before the deadline.
    pub fn is_open(&self, now: u64) -> bool {
        now < self.deadline()
    }
}
