//! OHLC aggregation package.
//!
//! Resample a series of `MarketBar`s into a higher timeframe (combine N
//! consecutive bars: first open, max high, min low, last close, summed volume).

use crate::adapters::trading::MarketBar;

/// Aggregate consecutive market bars into groups of `group_size`.
///
/// Returns an empty vector for `group_size == 0`. A trailing partial group is
/// aggregated using the same OHLCV rules as a full group.
pub fn aggregate(bars: &[MarketBar], group_size: usize) -> Vec<MarketBar> {
    if group_size == 0 {
        return Vec::new();
    }

    bars.chunks(group_size).map(aggregate_group).collect()
}

fn aggregate_group(group: &[MarketBar]) -> MarketBar {
    let first = group.first().expect("chunks never yields empty groups");
    let last = group.last().expect("chunks never yields empty groups");

    MarketBar {
        ts: first.ts,
        asset: first.asset,
        open: first.open,
        high: group
            .iter()
            .map(|bar| bar.high)
            .fold(f64::NEG_INFINITY, f64::max),
        low: group
            .iter()
            .map(|bar| bar.low)
            .fold(f64::INFINITY, f64::min),
        close: last.close,
        volume: group.iter().map(|bar| bar.volume).sum(),
        stale: first.stale,
        funding_rate: first.funding_rate,
    }
}
