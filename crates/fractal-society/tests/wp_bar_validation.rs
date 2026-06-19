use fractal_society::adapters::trading::{Asset, MarketBar};
use fractal_society::pkgs::bar_validation::validate;

fn bar() -> MarketBar {
    MarketBar {
        ts: 42,
        asset: Asset::Btc,
        open: 100.0,
        high: 110.0,
        low: 90.0,
        close: 105.0,
        volume: 1_000.0,
        stale: false,
    }
}

#[test]
fn well_formed_bar_is_ok() {
    assert_eq!(validate(&bar()), Ok(()));
}

#[test]
fn high_below_close_is_rejected() {
    let mut malformed = bar();
    malformed.high = 104.0;

    let errors = validate(&malformed).expect_err("high below close should fail");

    assert!(
        errors
            .iter()
            .any(|error| error.contains("max(open, close)"))
    );
}

#[test]
fn negative_price_is_rejected() {
    let mut malformed = bar();
    malformed.low = -1.0;

    let errors = validate(&malformed).expect_err("negative prices should fail");

    assert!(
        errors
            .iter()
            .any(|error| error == "low price must be non-negative")
    );
}
