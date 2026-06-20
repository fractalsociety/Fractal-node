//! Season state-machine package.
//!
//! Models the Agent-Arena season lifecycle and freezes season rules when a
//! draft season opens.

use crate::error::Error;
use crate::protocol::Hash;

/// Lifecycle state for an Agent-Arena season.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeasonState {
    /// Rules are editable and the season is not accepting submissions.
    Draft,
    /// Season is accepting submissions; rules are frozen.
    Open,
    /// Submissions are frozen while results are computed or reviewed.
    Frozen,
    /// Results are final but the season has not been archived.
    Final,
    /// Season is closed and archived.
    Closed,
}

/// Agent-Arena season record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Season {
    /// Stable season identifier.
    pub id: String,
    /// Current lifecycle state.
    pub state: SeasonState,
    /// Hash of the season rules.
    pub rules_hash: Hash,
    /// Whether rules are frozen against further edits.
    pub rules_frozen: bool,
}

/// Create a new draft season with unfrozen rules.
pub fn new_season(id: impl Into<String>, rules_hash: Hash) -> Season {
    Season {
        id: id.into(),
        state: SeasonState::Draft,
        rules_hash,
        rules_frozen: false,
    }
}

/// Transition a draft season to open and freeze its rules.
pub fn open(season: &mut Season) -> crate::Result<()> {
    transition(season, SeasonState::Draft, SeasonState::Open)?;
    season.rules_frozen = true;
    Ok(())
}

/// Transition an open season to frozen.
pub fn freeze(season: &mut Season) -> crate::Result<()> {
    transition(season, SeasonState::Open, SeasonState::Frozen)
}

/// Transition a frozen season to final.
pub fn finalize(season: &mut Season) -> crate::Result<()> {
    transition(season, SeasonState::Frozen, SeasonState::Final)
}

/// Transition a final season to closed.
pub fn close(season: &mut Season) -> crate::Result<()> {
    transition(season, SeasonState::Final, SeasonState::Closed)
}

fn transition(season: &mut Season, expected: SeasonState, next: SeasonState) -> crate::Result<()> {
    if season.state != expected {
        return Err(Error::ProtocolViolation(format!(
            "cannot transition season '{}' from {:?} to {:?}; expected {:?}",
            season.id, season.state, next, expected
        )));
    }
    season.state = next;
    Ok(())
}
