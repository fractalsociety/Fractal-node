//! Deterministic synthetic market-data fixtures.

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use crate::adapters::trading::types::{Asset, MarketBar};

/// A pair of bars for one logical step.
#[derive(Debug, Clone, PartialEq)]
pub struct BarSet {
    /// Logical step.
    pub step: u64,
    /// BTC bar.
    pub btc: MarketBar,
    /// ETH bar.
    pub eth: MarketBar,
}

impl BarSet {
    /// Return the bar for an asset.
    pub fn bar(&self, asset: Asset) -> &MarketBar {
        match asset {
            Asset::Btc => &self.btc,
            Asset::Eth => &self.eth,
        }
    }
}

/// Build deterministic synthetic bars from a seed.
pub fn synthetic_bars(seed: u64, steps: u64) -> Vec<BarSet> {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut btc = 50_000.0 + (seed % 97) as f64;
    let mut eth = 3_000.0 + (seed % 53) as f64;
    let mut out = Vec::with_capacity(steps as usize);
    for step in 0..steps {
        let btc_move = ((step as i64 % 9) - 4) as f64 * 42.0 + rng.gen_range(-12.0..12.0);
        let eth_move = ((step as i64 % 7) - 3) as f64 * 7.0 + rng.gen_range(-3.0..3.0);
        let btc_open = btc;
        let eth_open = eth;
        btc = (btc + btc_move).max(10_000.0);
        eth = (eth + eth_move).max(500.0);
        out.push(BarSet {
            step,
            btc: make_bar(
                step,
                Asset::Btc,
                btc_open,
                btc,
                160.0,
                10.0 + step as f64,
                false,
                0.0,
            ),
            eth: make_bar(
                step,
                Asset::Eth,
                eth_open,
                eth,
                28.0,
                50.0 + step as f64,
                false,
                0.0,
            ),
        });
    }
    out
}

/// A deterministic down-only fixture for liquidation tests.
pub fn liquidation_bars(steps: u64) -> Vec<BarSet> {
    let mut out = Vec::with_capacity(steps as usize);
    let mut btc = 50_000.0;
    let mut eth = 3_000.0;
    for step in 0..steps {
        let btc_open = btc;
        let eth_open = eth;
        btc -= 5_000.0;
        eth -= 50.0;
        out.push(BarSet {
            step,
            btc: make_bar(step, Asset::Btc, btc_open, btc, 1_000.0, 25.0, false, 0.0),
            eth: make_bar(step, Asset::Eth, eth_open, eth, 20.0, 25.0, false, 0.0),
        });
    }
    out
}

/// A small fixture with known prices for golden fill/cost tests.
pub fn golden_bars() -> Vec<BarSet> {
    vec![
        BarSet {
            step: 0,
            btc: MarketBar {
                ts: 0,
                asset: Asset::Btc,
                open: 100.0,
                high: 105.0,
                low: 95.0,
                close: 100.0,
                volume: 1_000.0,
                stale: false,
                funding_rate: 0.0,
            },
            eth: MarketBar {
                ts: 0,
                asset: Asset::Eth,
                open: 10.0,
                high: 11.0,
                low: 9.0,
                close: 10.0,
                volume: 1_000.0,
                stale: false,
                funding_rate: 0.0,
            },
        },
        BarSet {
            step: 1,
            btc: MarketBar {
                ts: 1,
                asset: Asset::Btc,
                open: 100.0,
                high: 112.0,
                low: 99.0,
                close: 110.0,
                volume: 1_000.0,
                stale: false,
                funding_rate: 0.0,
            },
            eth: MarketBar {
                ts: 1,
                asset: Asset::Eth,
                open: 10.0,
                high: 12.0,
                low: 10.0,
                close: 11.0,
                volume: 1_000.0,
                stale: false,
                funding_rate: 0.0,
            },
        },
    ]
}

/// Deterministic fixture with a known positive BTC funding rate (0.001/step)
/// and a flat price, for hand-verifiable funding accounting.
pub fn funding_bars(steps: u64) -> Vec<BarSet> {
    let mut out = Vec::with_capacity(steps as usize);
    for step in 0..steps {
        out.push(BarSet {
            step,
            btc: make_bar(step, Asset::Btc, 100.0, 100.0, 0.5, 10.0, false, 0.001),
            eth: make_bar(step, Asset::Eth, 10.0, 10.0, 0.2, 10.0, false, 0.0),
        });
    }
    out
}

/// Deterministic fixture where BTC experiences data outages (stale bars) on even
/// steps; ETH is always fresh. Used to test data-outage handling (P04-N06).
pub fn outage_bars(steps: u64) -> Vec<BarSet> {
    let mut out = Vec::with_capacity(steps as usize);
    let mut btc = 100.0_f64;
    let mut eth = 10.0_f64;
    for step in 0..steps {
        let btc_open = btc;
        let eth_open = eth;
        btc += 1.0;
        eth += 0.1;
        let btc_stale = step % 2 == 0;
        out.push(BarSet {
            step,
            btc: make_bar(step, Asset::Btc, btc_open, btc, 1.0, 10.0, btc_stale, 0.0),
            eth: make_bar(step, Asset::Eth, eth_open, eth, 0.5, 10.0, false, 0.0),
        });
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn make_bar(
    step: u64,
    asset: Asset,
    open: f64,
    close: f64,
    spread: f64,
    volume: f64,
    stale: bool,
    funding_rate: f64,
) -> MarketBar {
    MarketBar {
        ts: step as i64,
        asset,
        open,
        high: open.max(close) + spread,
        low: (open.min(close) - spread).max(0.01),
        close,
        volume,
        stale,
        funding_rate,
    }
}
