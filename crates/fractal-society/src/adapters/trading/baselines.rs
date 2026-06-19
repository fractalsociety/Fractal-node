//! Deterministic baseline strategies (PHASE-04 gate P04-N08).
//!
//! Each baseline is an [`Agent`](crate::simulation::Agent) that runs through the
//! same generic kernel as any candidate trading agent. They are reference
//! points for scorecards and leaderboards, not competitive strategies. All are
//! deterministic given their construction: Cash and BuyAndHold are stateless in
//! randomness; Random and MovingAverage derive any variation from a constructor
//! seed or the deterministic bar series. A run therefore reproduces from a
//! frozen manifest.

use async_trait::async_trait;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use crate::adapters::trading::{
    Asset, OrderType, Side, TradingAction, TradingAdapter, TradingObservation,
};
use crate::error::Result;
use crate::simulation::Agent;

/// Cash baseline: never trades. Equity stays at the initial value; no fees.
#[derive(Debug, Default)]
pub struct CashBaseline;

impl CashBaseline {
    /// Create a new cash baseline.
    pub const fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Agent<TradingAdapter> for CashBaseline {
    fn id(&self) -> &str {
        "baseline-cash"
    }

    async fn act(&mut self, _observation: &TradingObservation) -> Result<TradingAction> {
        Ok(TradingAction::Hold)
    }
}

/// Buy-and-hold baseline: spend roughly 1x equity on BTC at step 0, then hold.
#[derive(Debug, Default)]
pub struct BuyAndHoldBaseline {
    invested: bool,
}

impl BuyAndHoldBaseline {
    /// Create a new buy-and-hold baseline.
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl Agent<TradingAdapter> for BuyAndHoldBaseline {
    fn id(&self) -> &str {
        "baseline-buy-and-hold"
    }

    async fn act(&mut self, observation: &TradingObservation) -> Result<TradingAction> {
        if self.invested || observation.btc.close <= 0.0 || observation.equity <= 0.0 {
            return Ok(TradingAction::Hold);
        }
        // Buy ~1x equity worth of BTC, rounded to 0.1 units.
        let qty = (observation.equity / observation.btc.close * 10.0).round() / 10.0;
        if qty <= 0.0 {
            return Ok(TradingAction::Hold);
        }
        self.invested = true;
        Ok(TradingAction::PlaceOrder {
            asset: Asset::Btc,
            side: Side::Buy,
            order_type: OrderType::MarketableIoc,
            qty,
            limit_price: None,
            reduce_only: false,
        })
    }
}

/// Random baseline: deterministic seeded choice among hold/buy/reduce.
#[derive(Debug)]
pub struct RandomBaseline {
    rng: StdRng,
}

impl RandomBaseline {
    /// Create a new random baseline from a deterministic seed.
    pub fn new(seed: u64) -> Self {
        Self {
            rng: StdRng::seed_from_u64(seed),
        }
    }
}

#[async_trait]
impl Agent<TradingAdapter> for RandomBaseline {
    fn id(&self) -> &str {
        "baseline-random"
    }

    async fn act(&mut self, _observation: &TradingObservation) -> Result<TradingAction> {
        match self.rng.gen_range(0..4u8) {
            0 => Ok(TradingAction::PlaceOrder {
                asset: Asset::Btc,
                side: Side::Buy,
                order_type: OrderType::MarketableIoc,
                qty: 0.1,
                limit_price: None,
                reduce_only: false,
            }),
            1 => Ok(TradingAction::PlaceOrder {
                asset: Asset::Eth,
                side: Side::Buy,
                order_type: OrderType::MarketableIoc,
                qty: 0.2,
                limit_price: None,
                reduce_only: false,
            }),
            2 => Ok(TradingAction::ReducePosition {
                asset: Asset::Btc,
                qty: 0.1,
            }),
            _ => Ok(TradingAction::Hold),
        }
    }
}

/// Simple moving-average trend baseline on BTC closes (3-bar window).
#[derive(Debug, Default)]
pub struct MovingAverageBaseline {
    window: Vec<f64>,
}

impl MovingAverageBaseline {
    /// Create a new moving-average baseline.
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl Agent<TradingAdapter> for MovingAverageBaseline {
    fn id(&self) -> &str {
        "baseline-moving-average"
    }

    async fn act(&mut self, observation: &TradingObservation) -> Result<TradingAction> {
        self.window.push(observation.btc.close);
        if self.window.len() < 4 {
            return Ok(TradingAction::Hold);
        }
        let sma = self.window[self.window.len() - 4..self.window.len() - 1]
            .iter()
            .sum::<f64>()
            / 3.0;
        let want_long = observation.btc.close > sma;
        let btc_qty: f64 = observation
            .positions
            .iter()
            .find(|p| p.asset == Asset::Btc)
            .map(|p| p.qty.abs())
            .unwrap_or(0.0);
        let has_btc = btc_qty > 0.0;
        if want_long && !has_btc {
            Ok(TradingAction::PlaceOrder {
                asset: Asset::Btc,
                side: Side::Buy,
                order_type: OrderType::MarketableIoc,
                qty: 0.5,
                limit_price: None,
                reduce_only: false,
            })
        } else if !want_long && has_btc {
            Ok(TradingAction::ReducePosition {
                asset: Asset::Btc,
                qty: btc_qty,
            })
        } else {
            Ok(TradingAction::Hold)
        }
    }
}
