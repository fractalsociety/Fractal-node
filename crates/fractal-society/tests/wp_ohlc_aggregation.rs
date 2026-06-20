use fractal_society::adapters::trading::{Asset, MarketBar};
use fractal_society::pkgs::ohlc_aggregation::aggregate;

fn bar(ts: i64, open: f64, high: f64, low: f64, close: f64, volume: f64) -> MarketBar {
    MarketBar {
        ts,
        asset: Asset::Btc,
        open,
        high,
        low,
        close,
        volume,
        stale: ts == 1,
    }
}

#[test]
fn four_bars_grouped_by_two_produce_correct_ohlcv() {
    let aggregated = aggregate(
        &[
            bar(1, 100.0, 110.0, 95.0, 105.0, 10.0),
            bar(2, 105.0, 115.0, 101.0, 112.0, 20.0),
            bar(3, 112.0, 120.0, 111.0, 118.0, 30.0),
            bar(4, 118.0, 119.0, 108.0, 109.0, 40.0),
        ],
        2,
    );

    assert_eq!(aggregated.len(), 2);
    assert_eq!(aggregated[0].ts, 1);
    assert_eq!(aggregated[0].asset, Asset::Btc);
    assert_eq!(aggregated[0].open, 100.0);
    assert_eq!(aggregated[0].high, 115.0);
    assert_eq!(aggregated[0].low, 95.0);
    assert_eq!(aggregated[0].close, 112.0);
    assert_eq!(aggregated[0].volume, 30.0);
    assert!(aggregated[0].stale);

    assert_eq!(aggregated[1].open, 112.0);
    assert_eq!(aggregated[1].high, 120.0);
    assert_eq!(aggregated[1].low, 108.0);
    assert_eq!(aggregated[1].close, 109.0);
    assert_eq!(aggregated[1].volume, 70.0);
}

#[test]
fn group_size_zero_returns_empty() {
    assert!(aggregate(&[bar(1, 1.0, 1.0, 1.0, 1.0, 1.0)], 0).is_empty());
}

#[test]
fn remaining_bars_form_final_partial_group() {
    let aggregated = aggregate(
        &[
            bar(1, 10.0, 12.0, 9.0, 11.0, 1.0),
            bar(2, 11.0, 13.0, 10.0, 12.0, 2.0),
            bar(3, 12.0, 14.0, 8.0, 9.0, 3.0),
        ],
        2,
    );

    assert_eq!(aggregated.len(), 2);
    assert_eq!(aggregated[1].ts, 3);
    assert_eq!(aggregated[1].open, 12.0);
    assert_eq!(aggregated[1].high, 14.0);
    assert_eq!(aggregated[1].low, 8.0);
    assert_eq!(aggregated[1].close, 9.0);
    assert_eq!(aggregated[1].volume, 3.0);
}
