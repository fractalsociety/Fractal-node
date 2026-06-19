//! Deterministic synthetic fill model.

use crate::adapters::trading::types::{
    price_to_micro, qty_to_micro, Asset, MarketBar, OrderId, OrderType, Side, TradingAction,
};
use crate::error::Result;

/// Resting deterministic order.
#[derive(Debug, Clone, PartialEq)]
pub struct RestingOrder {
    /// Order id.
    pub id: OrderId,
    /// Asset.
    pub asset: Asset,
    /// Side.
    pub side: Side,
    /// Quantity.
    pub qty: f64,
    /// Limit price.
    pub limit_price: f64,
    /// Reduce-only flag.
    pub reduce_only: bool,
}

/// Deterministic fill intent returned by the fill model.
#[derive(Debug, Clone, PartialEq)]
pub struct FillIntent {
    /// Order id, if any.
    pub order_id: Option<OrderId>,
    /// Asset.
    pub asset: Asset,
    /// Side.
    pub side: Side,
    /// Quantity.
    pub qty: f64,
    /// Fill price.
    pub price: f64,
    /// Reduce-only flag.
    pub reduce_only: bool,
}

/// Convert a place-order action into either an immediate fill or resting order.
pub fn place_order(
    id: OrderId,
    action: &TradingAction,
    bar: &MarketBar,
) -> Result<(Option<FillIntent>, Option<RestingOrder>)> {
    match action {
        TradingAction::PlaceOrder {
            asset,
            side,
            order_type,
            qty,
            limit_price,
            reduce_only,
        } => match order_type {
            OrderType::MarketableIoc => Ok((
                Some(FillIntent {
                    order_id: Some(id),
                    asset: *asset,
                    side: *side,
                    qty: *qty,
                    price: bar.close,
                    reduce_only: *reduce_only,
                }),
                None,
            )),
            OrderType::LimitGtc => {
                let price = limit_price.unwrap_or(bar.close);
                let order = RestingOrder {
                    id,
                    asset: *asset,
                    side: *side,
                    qty: *qty,
                    limit_price: price,
                    reduce_only: *reduce_only,
                };
                if crosses_limit(bar, *side, price)? {
                    Ok((
                        Some(FillIntent {
                            order_id: Some(id),
                            asset: *asset,
                            side: *side,
                            qty: *qty,
                            price,
                            reduce_only: *reduce_only,
                        }),
                        None,
                    ))
                } else {
                    Ok((None, Some(order)))
                }
            }
        },
        _ => Ok((None, None)),
    }
}

/// Return fills for resting orders crossed by the current bar and remove them.
pub fn fill_resting_orders(
    orders: &mut Vec<RestingOrder>,
    bar: &MarketBar,
) -> Result<Vec<FillIntent>> {
    let mut fills = Vec::new();
    let mut kept = Vec::new();
    for order in orders.drain(..) {
        if order.asset == bar.asset && crosses_limit(bar, order.side, order.limit_price)? {
            let fill_qty = order.qty / 2.0;
            let remaining_qty = order.qty - fill_qty;
            fills.push(FillIntent {
                order_id: Some(order.id),
                asset: order.asset,
                side: order.side,
                qty: fill_qty,
                price: order.limit_price,
                reduce_only: order.reduce_only,
            });
            if remaining_qty > 0.000_001 {
                kept.push(RestingOrder {
                    qty: remaining_qty,
                    ..order
                });
            }
        } else {
            kept.push(order);
        }
    }
    *orders = kept;
    Ok(fills)
}

/// Return true when the current bar crosses the limit price.
pub fn crosses_limit(bar: &MarketBar, _side: Side, limit_price: f64) -> Result<bool> {
    let limit = price_to_micro(limit_price)?;
    let high = bar.high_micro()?;
    let low = bar.low_micro()?;
    Ok(low <= limit && limit <= high)
}

/// Clamp reduce-only quantity to existing exposure.
pub fn reduce_only_qty(requested_qty: f64, side: Side, existing_qty_micro: i64) -> Result<f64> {
    let requested = qty_to_micro(requested_qty)?;
    let reduces = (existing_qty_micro > 0 && side == Side::Sell)
        || (existing_qty_micro < 0 && side == Side::Buy);
    if !reduces {
        return Ok(0.0);
    }
    let clamped = requested.min(existing_qty_micro.abs());
    Ok(crate::adapters::trading::types::micro_to_f64(clamped))
}
