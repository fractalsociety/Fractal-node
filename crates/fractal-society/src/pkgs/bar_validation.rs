//! OHLCV sanity validation for trading market bars.
//!
//! Validate OHLCV sanity of a `MarketBar` (high ≥ max(open,close),
//! low ≤ min(open,close), finite non-negative prices/volume).

use crate::adapters::trading::MarketBar;

/// Validate basic OHLCV invariants for a market bar.
///
/// Returns [`Ok`] when every invariant passes. Returns all detected failures in
/// a deterministic order when the bar is malformed.
pub fn validate(bar: &MarketBar) -> std::result::Result<(), Vec<String>> {
    let mut errors = Vec::new();

    validate_price("open", bar.open, &mut errors);
    validate_price("high", bar.high, &mut errors);
    validate_price("low", bar.low, &mut errors);
    validate_price("close", bar.close, &mut errors);

    if !bar.volume.is_finite() {
        errors.push("volume must be finite".to_string());
    } else if bar.volume < 0.0 {
        errors.push("volume must be non-negative".to_string());
    }

    if bar.high < bar.open.max(bar.close) {
        errors.push("high must be greater than or equal to max(open, close)".to_string());
    }
    if bar.low > bar.open.min(bar.close) {
        errors.push("low must be less than or equal to min(open, close)".to_string());
    }
    if bar.high < bar.low {
        errors.push("high must be greater than or equal to low".to_string());
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn validate_price(name: &str, value: f64, errors: &mut Vec<String>) {
    if !value.is_finite() {
        errors.push(format!("{name} price must be finite"));
    } else if value < 0.0 {
        errors.push(format!("{name} price must be non-negative"));
    }
}
