//! Hyperliquid market-data source adapter.
//!
//! This module is feature-gated behind `live-data`. Live deployments can provide
//! a source backed by REST snapshots and websocket subscriptions; tests use the
//! same source trait with deterministic mock events.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::adapters::trading::fixtures::BarSet;
use crate::adapters::trading::Asset;
use crate::error::Error;
use crate::market_data::store::write_barsets;
use crate::market_data::{
    normalize_bar, BarWindow, BookSnapshot, FundingUpdate, NormalizeError, RawTrade,
};

/// Hyperliquid coin symbol used by the recorder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HyperliquidCoin {
    /// BTC perpetual market.
    Btc,
    /// ETH perpetual market.
    Eth,
}

impl HyperliquidCoin {
    /// Convert to the trading adapter asset enum.
    pub fn asset(self) -> Asset {
        match self {
            Self::Btc => Asset::Btc,
            Self::Eth => Asset::Eth,
        }
    }
}

/// Raw Hyperliquid feed event consumed by the recorder.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum HyperliquidRawEvent {
    /// Executed trade.
    Trade {
        /// Market symbol.
        coin: HyperliquidCoin,
        /// Exchange timestamp in seconds.
        ts: i64,
        /// Execution price.
        price: f64,
        /// Execution size.
        size: f64,
    },
    /// Best bid/ask snapshot.
    Book {
        /// Market symbol.
        coin: HyperliquidCoin,
        /// Exchange timestamp in seconds.
        ts: i64,
        /// Best bid price.
        best_bid: f64,
        /// Best ask price.
        best_ask: f64,
    },
    /// Funding-rate update.
    Funding {
        /// Market symbol.
        coin: HyperliquidCoin,
        /// Exchange timestamp in seconds.
        ts: i64,
        /// Funding rate for the period.
        funding_rate: f64,
    },
}

/// Source of Hyperliquid events.
pub trait HyperliquidEventSource: Send + Sync {
    /// Initial REST snapshot events.
    fn snapshot(&self) -> crate::Result<Vec<HyperliquidRawEvent>>;

    /// Subscription events collected after the snapshot.
    fn subscribe(&self) -> crate::Result<Vec<HyperliquidRawEvent>>;
}

/// Feature-gated Hyperliquid recorder.
#[derive(Debug, Clone)]
pub struct HyperliquidSource<S> {
    source: S,
    start: i64,
    interval_secs: i64,
    steps: u64,
}

impl<S> HyperliquidSource<S>
where
    S: HyperliquidEventSource,
{
    /// Create a source recorder.
    pub fn new(source: S, start: i64, interval_secs: i64, steps: u64) -> crate::Result<Self> {
        if interval_secs <= 0 {
            return Err(Error::InvalidArtifact(
                "interval_secs must be positive".to_string(),
            ));
        }
        Ok(Self {
            source,
            start,
            interval_secs,
            steps,
        })
    }

    /// Collect snapshot + subscription events and normalize them into barsets.
    pub fn record(&self) -> crate::Result<Vec<BarSet>> {
        let mut events = self.source.snapshot()?;
        events.extend(self.source.subscribe()?);
        events.sort_by_key(event_ts);
        events_to_barsets(&events, self.start, self.interval_secs, self.steps)
    }

    /// Record normalized bars and persist them with the package 74 store.
    pub fn record_to_store(&self, path: impl AsRef<Path>) -> crate::Result<Vec<BarSet>> {
        let barsets = self.record()?;
        write_barsets(path, &barsets)?;
        Ok(barsets)
    }
}

/// Convert raw events into deterministic BTC/ETH barsets.
pub fn events_to_barsets(
    events: &[HyperliquidRawEvent],
    start: i64,
    interval_secs: i64,
    steps: u64,
) -> crate::Result<Vec<BarSet>> {
    if interval_secs <= 0 {
        return Err(Error::InvalidArtifact(
            "interval_secs must be positive".to_string(),
        ));
    }

    let indexed = index_events(events);
    let mut sets = Vec::with_capacity(steps as usize);
    for step in 0..steps {
        let window_start = start + (step as i64 * interval_secs);
        let window_end = window_start + interval_secs;
        let btc = normalize_coin_window(&indexed, HyperliquidCoin::Btc, window_start, window_end)?;
        let eth = normalize_coin_window(&indexed, HyperliquidCoin::Eth, window_start, window_end)?;
        sets.push(BarSet { step, btc, eth });
    }
    Ok(sets)
}

#[derive(Debug, Default)]
struct IndexedEvents {
    trades: HashMap<HyperliquidCoin, Vec<RawTrade>>,
    books: HashMap<HyperliquidCoin, Vec<BookSnapshot>>,
    funding: HashMap<HyperliquidCoin, Vec<FundingUpdate>>,
}

fn index_events(events: &[HyperliquidRawEvent]) -> IndexedEvents {
    let mut indexed = IndexedEvents::default();
    for event in events {
        match *event {
            HyperliquidRawEvent::Trade {
                coin,
                ts,
                price,
                size,
            } => indexed
                .trades
                .entry(coin)
                .or_default()
                .push(RawTrade { ts, price, size }),
            HyperliquidRawEvent::Book {
                coin,
                ts,
                best_bid,
                best_ask,
            } => indexed.books.entry(coin).or_default().push(BookSnapshot {
                ts,
                best_bid,
                best_ask,
            }),
            HyperliquidRawEvent::Funding {
                coin,
                ts,
                funding_rate,
            } => indexed
                .funding
                .entry(coin)
                .or_default()
                .push(FundingUpdate { ts, funding_rate }),
        }
    }
    indexed
}

fn normalize_coin_window(
    indexed: &IndexedEvents,
    coin: HyperliquidCoin,
    start: i64,
    end: i64,
) -> crate::Result<crate::adapters::trading::MarketBar> {
    let trades = indexed
        .trades
        .get(&coin)
        .into_iter()
        .flatten()
        .filter(|trade| trade.ts >= start && trade.ts < end)
        .cloned()
        .collect::<Vec<_>>();
    let mark_snapshot = latest_at_or_before(indexed.books.get(&coin), end);
    let funding = latest_at_or_before(indexed.funding.get(&coin), end);
    let stale = trades.is_empty();
    normalize_bar(&BarWindow {
        start,
        end,
        asset: coin.asset(),
        trades,
        mark_snapshot,
        funding,
        stale,
    })
    .map_err(normalize_error)
}

fn latest_at_or_before<T>(items: Option<&Vec<T>>, end: i64) -> Option<T>
where
    T: Clone + HasTimestamp,
{
    items?
        .iter()
        .filter(|item| item.ts() <= end)
        .max_by_key(|item| item.ts())
        .cloned()
}

trait HasTimestamp {
    fn ts(&self) -> i64;
}

impl HasTimestamp for BookSnapshot {
    fn ts(&self) -> i64 {
        self.ts
    }
}

impl HasTimestamp for FundingUpdate {
    fn ts(&self) -> i64 {
        self.ts
    }
}

fn normalize_error(err: NormalizeError) -> Error {
    Error::InvalidArtifact(format!(
        "failed to normalize Hyperliquid event window: {err}"
    ))
}

fn event_ts(event: &HyperliquidRawEvent) -> i64 {
    match *event {
        HyperliquidRawEvent::Trade { ts, .. }
        | HyperliquidRawEvent::Book { ts, .. }
        | HyperliquidRawEvent::Funding { ts, .. } => ts,
    }
}
