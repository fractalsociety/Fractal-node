//! Normalized market-data layer (PHASE-03 recorder core, package 73).
//!
//! Converts raw exchange payloads (trades, order-book snapshots, funding updates)
//! into the trading adapter's [`MarketBar`]. Deterministic: identical raw inputs
//! always produce identical bars. This module performs **no network I/O** — feeds
//! are supplied by the caller (a future live source feeds this normalizer; see
//! package 75).

use serde::{Deserialize, Serialize};

use crate::adapters::trading::{Asset, MarketBar};

/// A single executed trade from an exchange feed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawTrade {
    /// Exchange timestamp (seconds).
    pub ts: i64,
    /// Execution price in USDC.
    pub price: f64,
    /// Execution size in base units.
    pub size: f64,
}

/// Best bid/ask snapshot at an instant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BookSnapshot {
    /// Exchange timestamp (seconds).
    pub ts: i64,
    /// Best bid price.
    pub best_bid: f64,
    /// Best ask price.
    pub best_ask: f64,
}

/// A funding-rate update.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FundingUpdate {
    /// Exchange timestamp (seconds).
    pub ts: i64,
    /// Funding rate for the period.
    pub funding_rate: f64,
}

/// Normalization inputs for one bar window of one asset.
#[derive(Debug, Clone, PartialEq)]
pub struct BarWindow {
    /// Bar start timestamp (inclusive), seconds.
    pub start: i64,
    /// Bar end timestamp (exclusive), seconds.
    pub end: i64,
    /// Asset this window covers.
    pub asset: Asset,
    /// Trades within `[start, end)`, in chronological order.
    pub trades: Vec<RawTrade>,
    /// Most recent book snapshot at or before `end` (mark source).
    pub mark_snapshot: Option<BookSnapshot>,
    /// Most recent funding update at or before `end`.
    pub funding: Option<FundingUpdate>,
    /// Set true when the window had no usable trades (data outage / gap).
    pub stale: bool,
}

/// Normalize a single bar window into a [`MarketBar`].
///
/// OHLCV is aggregated from the window's trades (`open`=first, `high`=max,
/// `low`=min, `close`=last, `volume`=sum). For a stale window (no trades),
/// every OHLCV field is set to the snapshot mid (or 0.0 if no snapshot) and
/// `volume` is 0. The bar's `ts` is the window start.
///
/// Note: `MarketBar` currently has no `funding_rate` field (funding accrual is
/// not yet wired into the trading adapter). The window's funding rate is exposed
/// separately via [`funding_rate`] for the future wiring task.
pub fn normalize_bar(window: &BarWindow) -> Result<MarketBar, NormalizeError> {
    if window.start >= window.end {
        return Err(NormalizeError::InvalidWindow);
    }

    let (open, high, low, close, volume) = aggregate(&window.trades)?;

    // Mark = snapshot mid if available, else the close (0.0 for empty trades).
    let mark = window
        .mark_snapshot
        .as_ref()
        .map(|snapshot| (snapshot.best_bid + snapshot.best_ask) / 2.0)
        .unwrap_or(close);

    // Stale windows have no trades: fill OHLCV with the mark so the bar is still
    // internally valid (high >= max(open, close), low <= min(open, close)).
    let (open, high, low, close) = if window.stale {
        (mark, mark, mark, mark)
    } else {
        (open, high, low, close)
    };

    Ok(MarketBar {
        ts: window.start,
        asset: window.asset,
        open,
        high,
        low,
        close,
        volume,
        stale: window.stale,
    })
}

/// The funding rate for a window (from its funding update), or 0.0 if none.
pub fn funding_rate(window: &BarWindow) -> f64 {
    window
        .funding
        .as_ref()
        .map(|f| f.funding_rate)
        .unwrap_or(0.0)
}

/// Normalize a chronological series of windows into bars.
pub fn normalize_series(windows: &[BarWindow]) -> Result<Vec<MarketBar>, NormalizeError> {
    windows.iter().map(normalize_bar).collect()
}

/// Aggregate trades into `(open, high, low, close, volume)`.
fn aggregate(trades: &[RawTrade]) -> Result<(f64, f64, f64, f64, f64), NormalizeError> {
    if trades.is_empty() {
        return Ok((0.0, 0.0, 0.0, 0.0, 0.0));
    }
    let open = trades[0].price;
    if !open.is_finite() || open < 0.0 {
        return Err(NormalizeError::InvalidPrice);
    }
    let mut high = open;
    let mut low = open;
    let mut close = open;
    let mut volume = 0.0;
    for trade in trades {
        if !trade.price.is_finite() || trade.price < 0.0 {
            return Err(NormalizeError::InvalidPrice);
        }
        if !trade.size.is_finite() || trade.size < 0.0 {
            return Err(NormalizeError::InvalidSize);
        }
        high = high.max(trade.price);
        low = low.min(trade.price);
        close = trade.price;
        volume += trade.size;
    }
    Ok((open, high, low, close, volume))
}

/// Normalization error.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NormalizeError {
    /// Window `start >= end`.
    #[error("invalid bar window: start must be < end")]
    InvalidWindow,
    /// A trade had a non-finite or negative price.
    #[error("invalid trade price")]
    InvalidPrice,
    /// A trade had a non-finite or negative size.
    #[error("invalid trade size")]
    InvalidSize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_trades_aggregate_to_zeros() {
        assert_eq!(aggregate(&[]).unwrap(), (0.0, 0.0, 0.0, 0.0, 0.0));
    }
}
