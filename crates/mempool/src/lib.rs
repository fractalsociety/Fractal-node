//! In-memory mempool + simplified EIP-1559 base-fee update (`docs/prd.md` §9.4, §18 M2).

use fractal_core::Transaction;

#[derive(Clone, Debug)]
pub struct PooledTx {
    pub tx: Transaction,
    pub max_priority_fee_per_gas: u128,
    pub max_fee_per_gas: u128,
}

#[derive(Clone, Debug, Default)]
pub struct Mempool {
    pending: Vec<PooledTx>,
}

impl Mempool {
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn insert(&mut self, tx: PooledTx) {
        self.pending.push(tx);
    }

    pub fn drain_ready_gas_budget(&mut self, max_gas: u64, base_fee: u128) -> Vec<Transaction> {
        self.pending
            .sort_by(|a, b| b.max_priority_fee_per_gas.cmp(&a.max_priority_fee_per_gas));
        let mut taken = Vec::new();
        let mut rest = Vec::new();
        let mut used: u64 = 0;
        for p in self.pending.drain(..) {
            if p.max_fee_per_gas < base_fee {
                rest.push(p);
                continue;
            }
            let Ok(g) = fractal_core::intrinsic_gas(&p.tx) else {
                rest.push(p);
                continue;
            };
            if used.saturating_add(g) <= max_gas {
                used = used.saturating_add(g);
                taken.push(p.tx);
            } else {
                rest.push(p);
            }
        }
        self.pending = rest;
        taken
    }

    /// Drain up to `max_txs` transactions that satisfy `max_fee_per_gas >= base_fee`,
    /// highest effective priority first.
    pub fn drain_ready(&mut self, max_txs: usize, base_fee: u128) -> Vec<Transaction> {
        self.pending
            .sort_by(|a, b| b.max_priority_fee_per_gas.cmp(&a.max_priority_fee_per_gas));
        let mut taken = Vec::new();
        let mut rest = Vec::new();
        for p in self.pending.drain(..) {
            if taken.len() < max_txs && p.max_fee_per_gas >= base_fee {
                taken.push(p.tx);
            } else {
                rest.push(p);
            }
        }
        self.pending = rest;
        taken
    }
}

/// PRD testnet targets (`docs/prd.md` §9.4): `target_gas_per_block = 30_000_000`, denominator 8.
#[derive(Clone, Debug)]
pub struct BaseFeeParams {
    pub min_base_fee: u128,
    pub target_gas_per_block: u64,
    pub denominator: u64,
}

impl Default for BaseFeeParams {
    fn default() -> Self {
        Self {
            min_base_fee: 1,
            target_gas_per_block: 30_000_000,
            denominator: 8,
        }
    }
}

/// Ethereum-style base fee update (integer math, `u128` fee space).
pub fn next_base_fee(parent_base_fee: u128, parent_gas_used: u64, p: &BaseFeeParams) -> u128 {
    let mut bf = parent_base_fee.max(p.min_base_fee);
    let target = p.target_gas_per_block as u128;
    let used = parent_gas_used as u128;
    if target == 0 {
        return bf;
    }
    if used == target {
        return bf;
    }
    if used > target {
        let gas_delta = used - target;
        let base_fee_per_gas_delta = (bf.saturating_mul(gas_delta) / target / u128::from(p.denominator)).max(1);
        bf = bf.saturating_add(base_fee_per_gas_delta);
    } else {
        let gas_delta = target - used;
        let base_fee_per_gas_delta = bf.saturating_mul(gas_delta) / target / u128::from(p.denominator);
        bf = bf.saturating_sub(base_fee_per_gas_delta);
    }
    bf.max(p.min_base_fee)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_fee_increases_when_over_target() {
        let p = BaseFeeParams {
            min_base_fee: 100,
            target_gas_per_block: 1_000_000,
            denominator: 8,
        };
        let next = next_base_fee(1000, 2_000_000, &p);
        assert!(next > 1000);
    }
}
