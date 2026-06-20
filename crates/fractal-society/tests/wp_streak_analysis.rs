use fractal_society::pkgs::streak_analysis::{Streaks, analyze};

#[test]
fn alternating_signs_have_one_step_streaks() {
    assert_eq!(
        analyze(&[1.0, -1.0, 2.0, -2.0]),
        Streaks {
            max_win_streak: 1,
            max_loss_streak: 1,
        }
    );
}

#[test]
fn run_of_wins_sets_max_win_streak() {
    let streaks = analyze(&[-1.0, 1.0, 2.0, 3.0, 0.0, 4.0]);

    assert_eq!(streaks.max_win_streak, 3);
    assert_eq!(streaks.max_loss_streak, 1);
}

#[test]
fn empty_returns_zero_streaks() {
    assert_eq!(
        analyze(&[]),
        Streaks {
            max_win_streak: 0,
            max_loss_streak: 0,
        }
    );
}

#[test]
fn zero_breaks_both_streaks() {
    assert_eq!(
        analyze(&[1.0, 2.0, 0.0, -1.0, -2.0, 0.0, -3.0]),
        Streaks {
            max_win_streak: 2,
            max_loss_streak: 2,
        }
    );
}
