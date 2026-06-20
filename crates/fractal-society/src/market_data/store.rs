//! Append-only bar-dataset store (PHASE-03, package 74).
//!
//! Persists a series of [`BarSet`](crate::adapters::trading::BarSet)s to a JSONL
//! file (one per line) and reads them back, so a recorded dataset can drive
//! `TradingAdapter::with_bars`. Deterministic and byte-stable: identical bar
//! sets serialize to identical bytes (struct fields are declaration-ordered, no
//! maps).

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::adapters::trading::fixtures::BarSet;
use crate::adapters::trading::MarketBar;

/// Serializable form of a [`BarSet`] (`BarSet` itself does not derive Serialize).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct StoredBarSet {
    /// Step index.
    step: u64,
    /// BTC bar.
    btc: MarketBar,
    /// ETH bar.
    eth: MarketBar,
}

fn to_stored(set: &BarSet) -> StoredBarSet {
    StoredBarSet {
        step: set.step,
        btc: set.btc.clone(),
        eth: set.eth.clone(),
    }
}

fn from_stored(stored: StoredBarSet) -> BarSet {
    BarSet {
        step: stored.step,
        btc: stored.btc,
        eth: stored.eth,
    }
}

/// Write `sets` to `path` as JSONL (one `BarSet` per line), overwriting any
/// existing file.
pub fn write_barsets(path: impl AsRef<Path>, sets: &[BarSet]) -> crate::Result<()> {
    let mut out = String::new();
    for set in sets {
        let stored = to_stored(set);
        out.push_str(&serde_json::to_string(&stored)?);
        out.push('\n');
    }
    std::fs::write(path, out)?;
    Ok(())
}

/// Read a JSONL bar-dataset file produced by [`write_barsets`].
pub fn read_barsets(path: impl AsRef<Path>) -> crate::Result<Vec<BarSet>> {
    let contents = std::fs::read_to_string(path)?;
    let mut sets = Vec::new();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let stored: StoredBarSet = serde_json::from_str(line)?;
        sets.push(from_stored(stored));
    }
    Ok(sets)
}
