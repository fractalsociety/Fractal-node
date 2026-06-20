//! Funding accrual: a long pays funding each step, funding folds into
//! realized PnL so the accounting invariant still holds.

use fractal_society::adapters::trading::{
    funding_bars, Asset, OrderType, Side, TradingAction, TradingAdapter, TradingConfig,
};
use fractal_society::simulation::DomainAdapter;

fn assert_close(left: f64, right: f64, eps: f64) {
    assert!(
        (left - right).abs() <= eps,
        "left={left} right={right} diff={}",
        (left - right).abs()
    );
}

#[tokio::test]
async fn funding_accrues_for_long_and_preserves_accounting_invariant() {
    let mut adapter = TradingAdapter::with_bars(TradingConfig::default(), funding_bars(5)).unwrap();
    adapter.reset().await.unwrap();

    // Open 1 BTC long at 100 (funding 0.001/step, fee 5bps).
    adapter
        .step(TradingAction::PlaceOrder {
            asset: Asset::Btc,
            side: Side::Buy,
            order_type: OrderType::MarketableIoc,
            qty: 1.0,
            limit_price: None,
            reduce_only: false,
        })
        .await
        .unwrap();
    // Hold one more step so funding accrues twice (open step + hold step).
    let out = adapter.step(TradingAction::Hold).await.unwrap();

    // Long pays 1 BTC * 100 * 0.001 = 0.10 per step, twice => 0.20.
    println!("funding_paid={:.6}", adapter.ledger().funding_paid());
    assert_close(adapter.ledger().funding_paid(), 0.20, 1e-6);

    // Invariant must still hold with funding folded into realized PnL.
    assert_close(
        out.outcome.equity,
        out.outcome.cash + out.outcome.position_notional,
        1e-4,
    );
    assert_close(
        out.outcome.total_pnl,
        out.outcome.realized_pnl + out.outcome.unrealized_pnl - out.outcome.fees,
        1e-4,
    );
    // Realized PnL is negative (funding paid; no closed trades).
    assert!(out.outcome.realized_pnl < 0.0);
}
