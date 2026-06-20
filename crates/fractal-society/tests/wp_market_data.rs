//! Package 73 — normalized market-data layer.

use fractal_society::adapters::trading::Asset;
use fractal_society::market_data::{
    funding_rate, normalize_bar, normalize_series, BarWindow, BookSnapshot, FundingUpdate, RawTrade,
};
use fractal_society::pkgs::bar_validation;

fn trade(ts: i64, price: f64, size: f64) -> RawTrade {
    RawTrade { ts, price, size }
}

#[test]
fn aggregates_ohlcv_from_trades() {
    let window = BarWindow {
        start: 100,
        end: 200,
        asset: Asset::Btc,
        trades: vec![
            trade(110, 100.0, 1.0),
            trade(120, 105.0, 2.0),
            trade(130, 95.0, 0.5),
            trade(140, 102.0, 1.5),
        ],
        mark_snapshot: None,
        funding: None,
        stale: false,
    };

    let bar = normalize_bar(&window).unwrap();
    assert_eq!(bar.open, 100.0);
    assert_eq!(bar.high, 105.0);
    assert_eq!(bar.low, 95.0);
    assert_eq!(bar.close, 102.0);
    assert_eq!(bar.volume, 5.0);
    assert!(!bar.stale);
    assert_eq!(bar.ts, 100);

    // The normalized bar must pass OHLCV sanity validation.
    bar_validation::validate(&bar).unwrap();
}

#[test]
fn stale_bar_uses_snapshot_mid() {
    let window = BarWindow {
        start: 100,
        end: 200,
        asset: Asset::Eth,
        trades: Vec::new(),
        mark_snapshot: Some(BookSnapshot {
            ts: 150,
            best_bid: 9.0,
            best_ask: 11.0,
        }),
        funding: Some(FundingUpdate {
            ts: 160,
            funding_rate: 0.0005,
        }),
        stale: true,
    };

    let bar = normalize_bar(&window).unwrap();
    // Stale: OHLCV all = mid (10.0), volume 0, stale true.
    assert_eq!(bar.open, 10.0);
    assert_eq!(bar.high, 10.0);
    assert_eq!(bar.low, 10.0);
    assert_eq!(bar.close, 10.0);
    assert_eq!(bar.volume, 0.0);
    assert!(bar.stale);
    bar_validation::validate(&bar).unwrap();

    // Funding is exposed separately (the bar struct has no funding field yet).
    assert_eq!(funding_rate(&window), 0.0005);
}

#[test]
fn series_is_deterministic_and_valid() {
    let windows = vec![
        BarWindow {
            start: 0,
            end: 1,
            asset: Asset::Btc,
            trades: vec![trade(0, 50.0, 1.0), trade(0, 52.0, 1.0)],
            mark_snapshot: None,
            funding: None,
            stale: false,
        },
        BarWindow {
            start: 1,
            end: 2,
            asset: Asset::Btc,
            trades: vec![trade(1, 51.0, 2.0)],
            mark_snapshot: None,
            funding: None,
            stale: false,
        },
    ];

    let first = normalize_series(&windows).unwrap();
    let second = normalize_series(&windows).unwrap();
    assert_eq!(first, second, "normalization must be deterministic");
    assert_eq!(first.len(), 2);
    for bar in &first {
        bar_validation::validate(bar).unwrap();
    }
}

#[test]
fn invalid_window_is_rejected() {
    let window = BarWindow {
        start: 200,
        end: 200,
        asset: Asset::Btc,
        trades: Vec::new(),
        mark_snapshot: None,
        funding: None,
        stale: false,
    };
    assert!(normalize_bar(&window).is_err());
}
