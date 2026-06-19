//! Integer accounting ledger for the trading adapter.
//!
//! All decisions are made in integer micro-USDC and micro asset units. This
//! avoids NaN/Inf and float equality issues in the hot path; f64 values are
//! only emitted at the adapter boundary for JSON/evidence.

use std::collections::HashMap;

use crate::adapters::trading::types::{
    micro_to_f64, price_to_micro, qty_to_micro, Asset, Fill, PositionView, Side, TradingConfig,
    MICRO,
};
use crate::error::{Error, Result};

/// Request to apply an executed trade to the ledger.
#[derive(Debug, Clone, Copy)]
pub struct FillRequest {
    /// Asset being filled.
    pub asset: Asset,
    /// Fill side.
    pub side: Side,
    /// Quantity in asset units.
    pub qty: f64,
    /// Fill price in USDC.
    pub price: f64,
    /// Fee rate in basis points.
    pub fee_bps: u32,
    /// Optional order id associated with the fill.
    pub order_id: Option<crate::adapters::trading::types::OrderId>,
    /// Whether the fill came from liquidation.
    pub liquidation: bool,
}

/// Integer position state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    /// Signed quantity in micro asset units.
    pub qty_micro: i64,
    /// Average entry price in micro-USDC.
    pub avg_entry_price_micro: i64,
}

impl Position {
    /// Return true when the position is flat.
    pub fn is_flat(self) -> bool {
        self.qty_micro == 0
    }
}

/// Deterministic account ledger.
#[derive(Debug, Clone)]
pub struct Ledger {
    initial_equity_micro: i64,
    cash_micro: i64,
    realized_pnl_micro: i64,
    fees_micro: i64,
    positions: HashMap<Asset, Position>,
}

impl Ledger {
    /// Create a new ledger from config.
    pub fn new(config: &TradingConfig) -> Result<Self> {
        let initial_equity_micro = price_to_micro(config.initial_equity)?;
        Ok(Self {
            initial_equity_micro,
            cash_micro: initial_equity_micro,
            realized_pnl_micro: 0,
            fees_micro: 0,
            positions: HashMap::new(),
        })
    }

    /// Starting equity in micro-USDC.
    pub fn initial_equity_micro(&self) -> i64 {
        self.initial_equity_micro
    }

    /// Starting equity in USDC.
    pub fn initial_equity(&self) -> f64 {
        micro_to_f64(self.initial_equity_micro)
    }

    /// Cash balance in micro-USDC.
    pub fn cash_micro(&self) -> i64 {
        self.cash_micro
    }

    /// Cash balance in USDC.
    pub fn cash(&self) -> f64 {
        micro_to_f64(self.cash_micro)
    }

    /// Realized PnL before fees in micro-USDC.
    pub fn realized_pnl_micro(&self) -> i64 {
        self.realized_pnl_micro
    }

    /// Fees in micro-USDC.
    pub fn fees_micro(&self) -> i64 {
        self.fees_micro
    }

    /// Current position for an asset.
    pub fn position(&self, asset: Asset) -> Position {
        self.positions.get(&asset).copied().unwrap_or(Position {
            qty_micro: 0,
            avg_entry_price_micro: 0,
        })
    }

    /// Position views at marks.
    pub fn position_views(&self, marks: &HashMap<Asset, i64>) -> Vec<PositionView> {
        [Asset::Btc, Asset::Eth]
            .into_iter()
            .filter_map(|asset| {
                let pos = self.position(asset);
                if pos.is_flat() {
                    return None;
                }
                let mark = *marks.get(&asset).unwrap_or(&pos.avg_entry_price_micro);
                let notional = mul_micro(pos.qty_micro, mark);
                let unrealized = mul_micro(pos.qty_micro, mark - pos.avg_entry_price_micro);
                Some(PositionView {
                    asset,
                    qty: micro_to_f64(pos.qty_micro),
                    avg_entry_price: micro_to_f64(pos.avg_entry_price_micro),
                    mark_price: micro_to_f64(mark),
                    notional: micro_to_f64(notional),
                    unrealized_pnl: micro_to_f64(unrealized),
                })
            })
            .collect()
    }

    /// Equity in micro-USDC: cash plus signed position value at mark.
    pub fn equity_micro(&self, marks: &HashMap<Asset, i64>) -> i64 {
        self.cash_micro
            + [Asset::Btc, Asset::Eth]
                .into_iter()
                .map(|asset| {
                    let pos = self.position(asset);
                    let mark = *marks.get(&asset).unwrap_or(&pos.avg_entry_price_micro);
                    mul_micro(pos.qty_micro, mark)
                })
                .sum::<i64>()
    }

    /// Equity in USDC.
    pub fn equity(&self, marks: &HashMap<Asset, i64>) -> f64 {
        micro_to_f64(self.equity_micro(marks))
    }

    /// Gross exposure in micro-USDC.
    pub fn gross_exposure_micro(&self, marks: &HashMap<Asset, i64>) -> i64 {
        [Asset::Btc, Asset::Eth]
            .into_iter()
            .map(|asset| {
                let pos = self.position(asset);
                let mark = *marks.get(&asset).unwrap_or(&pos.avg_entry_price_micro);
                mul_micro(pos.qty_micro.abs(), mark)
            })
            .sum()
    }

    /// Unrealized PnL in micro-USDC.
    pub fn unrealized_pnl_micro(&self, marks: &HashMap<Asset, i64>) -> i64 {
        [Asset::Btc, Asset::Eth]
            .into_iter()
            .map(|asset| {
                let pos = self.position(asset);
                let mark = *marks.get(&asset).unwrap_or(&pos.avg_entry_price_micro);
                mul_micro(pos.qty_micro, mark - pos.avg_entry_price_micro)
            })
            .sum()
    }

    /// Total PnL after fees in micro-USDC.
    pub fn total_pnl_micro(&self, marks: &HashMap<Asset, i64>) -> i64 {
        self.equity_micro(marks) - self.initial_equity_micro
    }

    /// Apply a fill at the supplied fee rate.
    pub fn apply_fill(&mut self, request: FillRequest) -> Result<Fill> {
        let qty_micro = qty_to_micro(request.qty)?;
        let price_micro = price_to_micro(request.price)?;
        let signed_qty = qty_micro * request.side.sign();
        let notional_micro = mul_micro(qty_micro, price_micro);
        let fee_micro = notional_micro * i64::from(request.fee_bps) / 10_000;
        self.fees_micro += fee_micro;
        self.cash_micro -= request.side.sign() * notional_micro;
        self.cash_micro -= fee_micro;
        self.update_position(request.asset, signed_qty, price_micro)?;
        Ok(Fill {
            order_id: request.order_id,
            asset: request.asset,
            side: request.side,
            qty: request.qty,
            price: request.price,
            fee: micro_to_f64(fee_micro),
            liquidation: request.liquidation,
        })
    }

    fn update_position(&mut self, asset: Asset, delta_qty: i64, price_micro: i64) -> Result<()> {
        let mut pos = self.position(asset);
        if pos.qty_micro == 0 || pos.qty_micro.signum() == delta_qty.signum() {
            let old_abs = pos.qty_micro.abs();
            let delta_abs = delta_qty.abs();
            let total_abs = old_abs + delta_abs;
            let avg = if total_abs == 0 {
                0
            } else {
                (i128::from(pos.avg_entry_price_micro) * i128::from(old_abs)
                    + i128::from(price_micro) * i128::from(delta_abs))
                    / i128::from(total_abs)
            };
            pos.qty_micro += delta_qty;
            pos.avg_entry_price_micro = avg as i64;
        } else {
            let close_qty = pos.qty_micro.abs().min(delta_qty.abs());
            let direction = pos.qty_micro.signum();
            self.realized_pnl_micro += mul_micro(
                close_qty * direction,
                price_micro - pos.avg_entry_price_micro,
            );
            pos.qty_micro += delta_qty;
            if pos.qty_micro == 0 {
                pos.avg_entry_price_micro = 0;
            } else if pos.qty_micro.signum() != direction {
                // This should only happen if validation allowed sign flipping.
                pos.avg_entry_price_micro = price_micro;
            }
        }
        if pos.qty_micro == 0 {
            self.positions.remove(&asset);
        } else {
            self.positions.insert(asset, pos);
        }
        Ok(())
    }

    /// Liquidate all open positions at current marks.
    pub fn liquidate_all(
        &mut self,
        marks: &HashMap<Asset, i64>,
        fee_bps: u32,
    ) -> Result<Vec<Fill>> {
        let mut fills = Vec::new();
        for asset in [Asset::Btc, Asset::Eth] {
            let pos = self.position(asset);
            if pos.qty_micro == 0 {
                continue;
            }
            let side = if pos.qty_micro > 0 {
                Side::Sell
            } else {
                Side::Buy
            };
            let qty = micro_to_f64(pos.qty_micro.abs());
            let price_micro = *marks.get(&asset).ok_or_else(|| {
                Error::InvalidAction(format!("missing mark for {}", asset.symbol()))
            })?;
            let fill = self.apply_fill(FillRequest {
                asset,
                side,
                qty,
                price: micro_to_f64(price_micro),
                fee_bps,
                order_id: None,
                liquidation: true,
            })?;
            fills.push(fill);
        }
        Ok(fills)
    }
}

/// Multiply two micro-scaled values and return a micro-scaled value.
pub fn mul_micro(a: i64, b: i64) -> i64 {
    (i128::from(a) * i128::from(b) / i128::from(MICRO)) as i64
}

/// Convert an integer micro value to f64.
pub fn usdc(value: i64) -> f64 {
    micro_to_f64(value)
}
