//! Streak-analysis package.
//!
//! Computes maximum positive and negative return streaks.

/// Maximum win and loss streaks in a return series.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Streaks {
    /// Longest consecutive run of strictly positive returns.
    pub max_win_streak: usize,
    /// Longest consecutive run of strictly negative returns.
    pub max_loss_streak: usize,
}

/// Analyze max win/loss streaks, with zero and non-finite values breaking both streaks.
pub fn analyze(returns: &[f64]) -> Streaks {
    let mut current_wins = 0usize;
    let mut current_losses = 0usize;
    let mut max_win_streak = 0usize;
    let mut max_loss_streak = 0usize;

    for value in returns {
        if value.is_finite() && *value > 0.0 {
            current_wins += 1;
            current_losses = 0;
            max_win_streak = max_win_streak.max(current_wins);
        } else if value.is_finite() && *value < 0.0 {
            current_losses += 1;
            current_wins = 0;
            max_loss_streak = max_loss_streak.max(current_losses);
        } else {
            current_wins = 0;
            current_losses = 0;
        }
    }

    Streaks {
        max_win_streak,
        max_loss_streak,
    }
}
