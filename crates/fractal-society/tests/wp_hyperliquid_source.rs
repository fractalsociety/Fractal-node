#![cfg(feature = "live-data")]

use fractal_society::market_data::hyperliquid::{
    HyperliquidCoin, HyperliquidEventSource, HyperliquidRawEvent, HyperliquidSource,
};
use fractal_society::market_data::store::read_barsets;

#[derive(Debug, Clone)]
struct MockSource {
    snapshot: Vec<HyperliquidRawEvent>,
    subscription: Vec<HyperliquidRawEvent>,
}

impl HyperliquidEventSource for MockSource {
    fn snapshot(&self) -> fractal_society::Result<Vec<HyperliquidRawEvent>> {
        Ok(self.snapshot.clone())
    }

    fn subscribe(&self) -> fractal_society::Result<Vec<HyperliquidRawEvent>> {
        Ok(self.subscription.clone())
    }
}

fn mock_source() -> MockSource {
    use HyperliquidCoin::{Btc, Eth};
    use HyperliquidRawEvent::{Book, Funding, Trade};

    MockSource {
        snapshot: vec![
            Book {
                coin: Btc,
                ts: 0,
                best_bid: 99.0,
                best_ask: 101.0,
            },
            Book {
                coin: Eth,
                ts: 0,
                best_bid: 9.0,
                best_ask: 11.0,
            },
            Funding {
                coin: Btc,
                ts: 0,
                funding_rate: 0.0001,
            },
            Funding {
                coin: Eth,
                ts: 0,
                funding_rate: 0.0002,
            },
        ],
        subscription: vec![
            Trade {
                coin: Btc,
                ts: 1,
                price: 100.0,
                size: 1.0,
            },
            Trade {
                coin: Btc,
                ts: 2,
                price: 105.0,
                size: 2.0,
            },
            Trade {
                coin: Eth,
                ts: 1,
                price: 10.0,
                size: 3.0,
            },
            Trade {
                coin: Eth,
                ts: 7,
                price: 11.0,
                size: 4.0,
            },
            Book {
                coin: Btc,
                ts: 5,
                best_bid: 104.0,
                best_ask: 106.0,
            },
            Book {
                coin: Eth,
                ts: 5,
                best_bid: 10.5,
                best_ask: 11.5,
            },
        ],
    }
}

#[test]
fn mock_source_records_normalizes_and_stores_barsets() {
    let path = std::env::temp_dir().join("fractal_wp_hyperliquid_source.jsonl");
    let source = HyperliquidSource::new(mock_source(), 0, 5, 2).unwrap();

    let recorded = source.record_to_store(&path).unwrap();
    let read_back = read_barsets(&path).unwrap();

    assert_eq!(read_back, recorded);
    assert_eq!(recorded.len(), 2);
    assert_eq!(recorded[0].btc.open, 100.0);
    assert_eq!(recorded[0].btc.high, 105.0);
    assert_eq!(recorded[0].btc.close, 105.0);
    assert_eq!(recorded[0].btc.volume, 3.0);
    assert!(!recorded[0].btc.stale);
    assert_eq!(recorded[1].btc.open, 105.0);
    assert_eq!(recorded[1].btc.close, 105.0);
    assert_eq!(recorded[1].btc.volume, 0.0);
    assert!(recorded[1].btc.stale);
    assert_eq!(recorded[1].eth.open, 11.0);
    assert_eq!(recorded[1].eth.close, 11.0);
    assert!(!recorded[1].eth.stale);

    let _ = std::fs::remove_file(path);
}

#[test]
fn invalid_interval_is_rejected() {
    let err = HyperliquidSource::new(mock_source(), 0, 0, 1).unwrap_err();

    assert!(err.to_string().contains("interval_secs"));
}
