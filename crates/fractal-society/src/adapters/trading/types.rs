//! Trading adapter domain types.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::simulation::{Action, Observation, Outcome};

/// Micro unit scale for USDC and asset quantities.
pub const MICRO: i64 = 1_000_000;

/// Supported synthetic assets for PHASE-04 Slice A.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Asset {
    /// Synthetic BTC linear contract.
    Btc,
    /// Synthetic ETH linear contract.
    Eth,
}

impl Asset {
    /// Stable lowercase symbol.
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Btc => "BTC",
            Self::Eth => "ETH",
        }
    }
}

/// Order side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    /// Buy increases long exposure or reduces short exposure.
    Buy,
    /// Sell increases short exposure or reduces long exposure.
    Sell,
}

impl Side {
    /// Signed quantity multiplier.
    pub fn sign(self) -> i64 {
        match self {
            Self::Buy => 1,
            Self::Sell => -1,
        }
    }
}

/// Supported synthetic order type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderType {
    /// Immediate marketable order filled at the current bar close.
    MarketableIoc,
    /// Resting limit order that can fill when a future bar crosses the price.
    LimitGtc,
}

/// Deterministic order identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct OrderId(pub u64);

/// Synthetic OHLCV bar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarketBar {
    /// Logical timestamp in seconds from the kernel/run fixture.
    pub ts: i64,
    /// Asset this bar describes.
    pub asset: Asset,
    /// Open price in USDC.
    pub open: f64,
    /// High price in USDC.
    pub high: f64,
    /// Low price in USDC.
    pub low: f64,
    /// Close price in USDC.
    pub close: f64,
    /// Synthetic volume.
    pub volume: f64,
    /// Data-quality flag: true when this bar represents a data outage (stale/gapped).
    pub stale: bool,
}

impl MarketBar {
    /// Return the close price in micro-USDC.
    pub fn close_micro(&self) -> Result<i64> {
        price_to_micro(self.close)
    }

    /// Return the high price in micro-USDC.
    pub fn high_micro(&self) -> Result<i64> {
        price_to_micro(self.high)
    }

    /// Return the low price in micro-USDC.
    pub fn low_micro(&self) -> Result<i64> {
        price_to_micro(self.low)
    }
}

/// Open net position for one asset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PositionView {
    /// Asset.
    pub asset: Asset,
    /// Signed quantity. Positive is long, negative is short.
    pub qty: f64,
    /// Average entry price in USDC.
    pub avg_entry_price: f64,
    /// Current mark price in USDC.
    pub mark_price: f64,
    /// Signed notional at mark.
    pub notional: f64,
    /// Unrealized PnL in USDC.
    pub unrealized_pnl: f64,
}

/// Observation emitted by the trading adapter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TradingObservation {
    /// Logical step index.
    pub step: u64,
    /// Current BTC bar.
    pub btc: MarketBar,
    /// Current ETH bar.
    pub eth: MarketBar,
    /// Current account equity in USDC.
    pub equity: f64,
    /// Available cash in USDC.
    pub cash: f64,
    /// Current positions.
    pub positions: Vec<PositionView>,
    /// Number of resting orders.
    pub open_order_count: u64,
}

impl Observation for TradingObservation {
    fn to_json(&self) -> Result<serde_json::Value> {
        Ok(serde_json::to_value(self)?)
    }
}

/// Trading action. These are structured simulation intents, never raw exchange calls.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TradingAction {
    /// Do nothing for this step.
    Hold,
    /// Place a synthetic order.
    PlaceOrder {
        /// Asset to trade.
        asset: Asset,
        /// Buy or sell.
        side: Side,
        /// Order type.
        order_type: OrderType,
        /// Quantity in asset units.
        qty: f64,
        /// Limit price for limit orders.
        limit_price: Option<f64>,
        /// If true, the order can only reduce existing exposure.
        reduce_only: bool,
    },
    /// Cancel a resting order.
    CancelOrder {
        /// Order id to cancel.
        id: OrderId,
    },
    /// Reduce an existing position without sign flip.
    ReducePosition {
        /// Asset to reduce.
        asset: Asset,
        /// Quantity to reduce.
        qty: f64,
    },
}

impl Action for TradingAction {
    fn validate(&self) -> Result<()> {
        match self {
            Self::Hold | Self::CancelOrder { .. } => Ok(()),
            Self::PlaceOrder {
                qty, limit_price, ..
            } => {
                finite_positive(*qty, "qty")?;
                if let Some(price) = limit_price {
                    finite_positive(*price, "limit_price")?;
                }
                Ok(())
            }
            Self::ReducePosition { qty, .. } => finite_positive(*qty, "qty"),
        }
    }

    fn to_json(&self) -> Result<serde_json::Value> {
        Ok(serde_json::to_value(self)?)
    }
}

/// Fill record emitted by the deterministic fill model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fill {
    /// Order id, if the fill came from an order.
    pub order_id: Option<OrderId>,
    /// Asset filled.
    pub asset: Asset,
    /// Side filled.
    pub side: Side,
    /// Filled quantity.
    pub qty: f64,
    /// Fill price.
    pub price: f64,
    /// Fee in USDC.
    pub fee: f64,
    /// Whether this was a liquidation fill.
    pub liquidation: bool,
}

/// Outcome emitted after each trading step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TradingOutcome {
    /// Step reward, defined as net return delta from initial equity.
    pub reward: f64,
    /// Account equity in USDC.
    pub equity: f64,
    /// Cash balance in USDC.
    pub cash: f64,
    /// Signed position notional at mark in USDC.
    pub position_notional: f64,
    /// Total PnL after fees.
    pub total_pnl: f64,
    /// Realized PnL before fees.
    pub realized_pnl: f64,
    /// Unrealized PnL.
    pub unrealized_pnl: f64,
    /// Total fees paid.
    pub fees: f64,
    /// Step index after the step.
    pub step: u64,
    /// Fills generated by this step.
    pub fills: Vec<Fill>,
    /// Whether liquidation occurred.
    pub liquidated: bool,
    /// Terminal flag.
    pub terminal: bool,
}

impl Outcome for TradingOutcome {
    fn primary_score(&self) -> f64 {
        self.reward
    }

    fn is_terminal(&self) -> bool {
        self.terminal
    }

    fn to_json(&self) -> Result<serde_json::Value> {
        Ok(serde_json::to_value(self)?)
    }
}

/// Adapter configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TradingConfig {
    /// Starting cash/equity in USDC.
    pub initial_equity: f64,
    /// Maximum gross exposure divided by initial equity.
    pub max_initial_leverage: f64,
    /// Minimum order notional in USDC.
    pub min_notional: f64,
    /// Fee rate in basis points.
    pub fee_bps: u32,
    /// Daily loss stop as a fraction of initial equity.
    pub daily_loss_stop_fraction: f64,
    /// Liquidation threshold as a fraction of initial equity.
    pub liquidation_equity_fraction: f64,
    /// Maximum fixture steps.
    pub max_steps: u64,
}

impl Default for TradingConfig {
    fn default() -> Self {
        Self {
            initial_equity: 100_000.0,
            max_initial_leverage: 2.0,
            min_notional: 10.0,
            fee_bps: 5,
            daily_loss_stop_fraction: 0.10,
            liquidation_equity_fraction: 0.50,
            max_steps: 100,
        }
    }
}

/// Convert a finite price to micro-USDC.
pub fn price_to_micro(value: f64) -> Result<i64> {
    finite_nonnegative(value, "price")?;
    Ok((normalize_zero(value) * MICRO as f64).round() as i64)
}

/// Convert a finite quantity to micro asset units.
pub fn qty_to_micro(value: f64) -> Result<i64> {
    finite_positive(value, "qty")?;
    Ok((normalize_zero(value) * MICRO as f64).round() as i64)
}

/// Convert micro units to f64.
pub fn micro_to_f64(value: i64) -> f64 {
    normalize_zero(value as f64 / MICRO as f64)
}

/// Normalize negative zero to positive zero.
pub fn normalize_zero(value: f64) -> f64 {
    if value == 0.0 {
        0.0
    } else {
        value
    }
}

/// Ensure finite positive value.
pub fn finite_positive(value: f64, name: &str) -> Result<()> {
    if !value.is_finite() || value <= 0.0 {
        return Err(Error::InvalidAction(format!(
            "{name} must be finite and positive"
        )));
    }
    Ok(())
}

/// Ensure finite nonnegative value.
pub fn finite_nonnegative(value: f64, name: &str) -> Result<()> {
    if !value.is_finite() || value < 0.0 {
        return Err(Error::InvalidAction(format!(
            "{name} must be finite and nonnegative"
        )));
    }
    Ok(())
}
