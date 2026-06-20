use fractal_society::pkgs::challenge_window::ChallengeWindow;

#[test]
fn is_open_before_deadline() {
    let window = ChallengeWindow::new(100, 50);

    assert!(window.is_open(149));
}

#[test]
fn is_closed_at_and_after_deadline() {
    let window = ChallengeWindow::new(100, 50);

    assert!(!window.is_open(150));
    assert!(!window.is_open(151));
}

#[test]
fn deadline_is_opened_at_plus_duration() {
    let window = ChallengeWindow::new(100, 50);

    assert_eq!(window.deadline(), 150);
}

#[test]
fn deadline_saturates_on_overflow() {
    let window = ChallengeWindow::new(u64::MAX - 1, 10);

    assert_eq!(window.deadline(), u64::MAX);
}
