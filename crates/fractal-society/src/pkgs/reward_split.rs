//! Deterministic largest-remainder reward splitting.
//!
//! Split a reward pool among winners proportionally to score weights, with
//! deterministic rounding and zero-sum guarantees.

/// A recipient's integer reward share.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewardShare {
    /// Reward recipient identifier.
    pub recipient: String,
    /// Integer amount assigned to this recipient.
    pub amount: u64,
}

#[derive(Debug, Clone)]
struct Allocation {
    index: usize,
    recipient: String,
    amount: u64,
    remainder: u128,
}

/// Split `pool` among positive-weight recipients using largest remainders.
///
/// Zero-weight recipients are excluded. If `pool` is zero or the positive
/// weight total is zero, the split is empty. Remainder ties are resolved by
/// original input order, and returned shares preserve original input order.
pub fn split(pool: u64, weights: &[(String, u64)]) -> Vec<RewardShare> {
    if pool == 0 {
        return Vec::new();
    }

    let total_weight: u128 = weights.iter().map(|(_, weight)| u128::from(*weight)).sum();
    if total_weight == 0 {
        return Vec::new();
    }

    let pool_u128 = u128::from(pool);
    let mut allocations = weights
        .iter()
        .enumerate()
        .filter_map(|(index, (recipient, weight))| {
            if *weight == 0 {
                return None;
            }

            let numerator = pool_u128 * u128::from(*weight);
            let amount = (numerator / total_weight) as u64;
            let remainder = numerator % total_weight;
            Some(Allocation {
                index,
                recipient: recipient.clone(),
                amount,
                remainder,
            })
        })
        .collect::<Vec<_>>();

    let assigned = allocations
        .iter()
        .map(|allocation| allocation.amount)
        .sum::<u64>();
    let mut leftover = pool - assigned;
    let mut remainder_order = (0..allocations.len()).collect::<Vec<_>>();
    remainder_order.sort_by(|&left, &right| {
        allocations[right]
            .remainder
            .cmp(&allocations[left].remainder)
            .then_with(|| allocations[left].index.cmp(&allocations[right].index))
    });

    for allocation_index in remainder_order {
        if leftover == 0 {
            break;
        }
        allocations[allocation_index].amount += 1;
        leftover -= 1;
    }

    allocations.sort_by_key(|allocation| allocation.index);
    allocations
        .into_iter()
        .map(|allocation| RewardShare {
            recipient: allocation.recipient,
            amount: allocation.amount,
        })
        .collect()
}
