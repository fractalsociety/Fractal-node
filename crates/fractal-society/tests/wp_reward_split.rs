use fractal_society::pkgs::reward_split::{split, RewardShare};

fn amount(shares: &[RewardShare], recipient: &str) -> u64 {
    shares
        .iter()
        .find(|share| share.recipient == recipient)
        .map(|share| share.amount)
        .unwrap_or(0)
}

#[test]
fn shares_sum_exactly_to_pool() {
    let shares = split(
        101,
        &[
            ("alice".to_string(), 3),
            ("bob".to_string(), 2),
            ("carol".to_string(), 1),
        ],
    );

    assert_eq!(shares.iter().map(|share| share.amount).sum::<u64>(), 101);
}

#[test]
fn shares_are_proportional_with_largest_remainder_rounding() {
    let shares = split(
        10,
        &[
            ("alice".to_string(), 1),
            ("bob".to_string(), 1),
            ("carol".to_string(), 1),
        ],
    );

    assert_eq!(
        shares,
        vec![
            RewardShare {
                recipient: "alice".to_string(),
                amount: 4,
            },
            RewardShare {
                recipient: "bob".to_string(),
                amount: 3,
            },
            RewardShare {
                recipient: "carol".to_string(),
                amount: 3,
            },
        ]
    );
}

#[test]
fn zero_weight_recipients_are_excluded() {
    let shares = split(
        100,
        &[
            ("alice".to_string(), 0),
            ("bob".to_string(), 3),
            ("carol".to_string(), 1),
        ],
    );

    assert_eq!(shares.len(), 2);
    assert_eq!(amount(&shares, "alice"), 0);
    assert_eq!(amount(&shares, "bob"), 75);
    assert_eq!(amount(&shares, "carol"), 25);
}

#[test]
fn zero_total_weights_return_empty_split() {
    let shares = split(100, &[("alice".to_string(), 0), ("bob".to_string(), 0)]);

    assert!(shares.is_empty());
}
