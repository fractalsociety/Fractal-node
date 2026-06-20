use fractal_society::pkgs::execution_budget::ExecutionBudget;

#[test]
fn consume_within_limit_returns_true_and_updates_used() {
    let mut budget = ExecutionBudget::new(10);

    assert!(budget.consume(4));
    assert_eq!(budget.used, 4);
    assert_eq!(budget.remaining(), 6);
}

#[test]
fn over_limit_returns_false_and_leaves_used_unchanged() {
    let mut budget = ExecutionBudget::new(10);
    assert!(budget.consume(7));

    assert!(!budget.consume(4));
    assert_eq!(budget.used, 7);
    assert_eq!(budget.remaining(), 3);
}

#[test]
fn remaining_and_exhausted_are_correct() {
    let mut budget = ExecutionBudget::new(5);

    assert_eq!(budget.remaining(), 5);
    assert!(!budget.exhausted());
    assert!(budget.consume(5));
    assert_eq!(budget.remaining(), 0);
    assert!(budget.exhausted());
}

#[test]
fn overflowing_consume_is_rejected_without_mutation() {
    let mut budget = ExecutionBudget {
        limit: u64::MAX,
        used: u64::MAX - 1,
    };

    assert!(!budget.consume(2));
    assert_eq!(budget.used, u64::MAX - 1);
}
