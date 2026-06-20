//! Trading portfolio simulator adapter.
//!
//! PHASE-04 Slice A lives entirely under this module. The adapter uses
//! deterministic synthetic fixtures and integer ledger accounting; it performs
//! no live network calls and reads no secrets.

/// Trading adapter implementation.
pub mod adapter;
/// Deterministic baseline strategies.
pub mod baselines;
/// Deterministic fill model.
pub mod fill_model;
/// Deterministic synthetic fixtures.
pub mod fixtures;
/// Integer portfolio ledger.
pub mod ledger;
/// Trading scorecard builder.
pub mod scorecard;
/// Trading domain types.
pub mod types;

pub use adapter::{TradingAdapter, TradingAgent, STARTER_TRADING_AGENT_ID, TRADING_ADAPTER_ID};
pub use baselines::{BuyAndHoldBaseline, CashBaseline, MovingAverageBaseline, RandomBaseline};
pub use fixtures::{funding_bars, golden_bars, liquidation_bars, outage_bars, synthetic_bars};
pub use ledger::Ledger;
pub use scorecard::build_scorecard;
pub use types::{
    Asset, Fill, MarketBar, OrderId, OrderType, PositionView, Side, TradingAction, TradingConfig,
    TradingObservation, TradingOutcome,
};
