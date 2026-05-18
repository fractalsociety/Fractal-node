//! Stake-weighted quorum helpers shared by vote/QC verification and on-chain slashing.

fn ceil_div_u128(a: u128, b: u128) -> u128 {
    if b == 0 {
        return 0;
    }
    a.saturating_add(b.saturating_sub(1)) / b
}

/// Stake mass mirroring PBFT `k`-of-`n` count quorum: `ceil(total_stake * k / n)` (min 1 when `total_stake > 0`).
#[must_use]
pub fn quorum_stake_threshold(total_stake: u128, k_validators: usize, n_validators: usize) -> u128 {
    if total_stake == 0 || n_validators == 0 || k_validators == 0 {
        return 0;
    }
    ceil_div_u128(
        total_stake.saturating_mul(k_validators as u128),
        n_validators as u128,
    )
    .max(1)
}
