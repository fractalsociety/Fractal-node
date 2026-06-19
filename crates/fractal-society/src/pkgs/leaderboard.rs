//! Leaderboard ranking package.
//!
//! Rank candidate entries by a risk/robustness-weighted score (never raw PnL
//! alone) with deterministic tie-breaking.

/// Raw leaderboard candidate metrics.
#[derive(Debug, Clone, PartialEq)]
pub struct LeaderboardEntry {
    /// Agent identifier.
    pub agent_id: String,
    /// Net return as a fraction.
    pub net_return: f64,
    /// Maximum drawdown as a fraction.
    pub max_drawdown: f64,
    /// Count of policy violations.
    pub policy_violations: u64,
}

/// Ranked leaderboard entry with the derived risk-adjusted score.
#[derive(Debug, Clone, PartialEq)]
pub struct RankedEntry {
    /// One-based rank after sorting.
    pub rank: u32,
    /// Original candidate entry.
    pub entry: LeaderboardEntry,
    /// Risk/robustness-weighted score.
    pub score: f64,
}

/// Rank entries by `net_return - 0.5 * max_drawdown - 0.1 * policy_violations`.
///
/// Sort order is score descending, then `agent_id` ascending for deterministic
/// tie-breaking. Ranks are contiguous from 1 in sorted order.
pub fn rank(entries: &[LeaderboardEntry]) -> Vec<RankedEntry> {
    let mut ranked: Vec<RankedEntry> = entries
        .iter()
        .cloned()
        .map(|entry| {
            let score = score(&entry);
            RankedEntry {
                rank: 0,
                entry,
                score,
            }
        })
        .collect();

    ranked.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.entry.agent_id.cmp(&b.entry.agent_id))
    });

    for (idx, entry) in ranked.iter_mut().enumerate() {
        entry.rank = (idx + 1) as u32;
    }

    ranked
}

fn score(entry: &LeaderboardEntry) -> f64 {
    entry.net_return - 0.5 * entry.max_drawdown - 0.1 * entry.policy_violations as f64
}
