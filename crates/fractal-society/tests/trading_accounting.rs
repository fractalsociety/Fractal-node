use fractal_society::adapters::trading::{TradingAction, TradingAdapter, TradingConfig};
use fractal_society::simulation::DomainAdapter;

fn assert_close(left: f64, right: f64, eps: f64) {
    assert!(
        (left - right).abs() <= eps,
        "left={left} right={right} diff={}",
        (left - right).abs()
    );
}

#[tokio::test]
async fn p04_n01_accounting_reconciles_every_step() {
    let mut adapter = TradingAdapter::new(
        42,
        TradingConfig {
            max_steps: 50,
            ..TradingConfig::default()
        },
    )
    .unwrap();
    adapter.reset().await.unwrap();
    let first = TradingAction::PlaceOrder {
        asset: fractal_society::adapters::trading::Asset::Btc,
        side: fractal_society::adapters::trading::Side::Buy,
        order_type: fractal_society::adapters::trading::OrderType::MarketableIoc,
        qty: 0.1,
        limit_price: None,
        reduce_only: false,
    };
    let mut result = adapter.step(first).await.unwrap();
    for _ in 1..50 {
        assert_close(
            result.outcome.equity,
            result.outcome.cash + result.outcome.position_notional,
            0.000_010,
        );
        assert_close(
            result.outcome.total_pnl,
            result.outcome.equity - 100_000.0,
            0.000_010,
        );
        assert_close(
            result.outcome.total_pnl,
            result.outcome.realized_pnl + result.outcome.unrealized_pnl - result.outcome.fees,
            0.000_010,
        );
        result = adapter.step(TradingAction::Hold).await.unwrap();
    }
}
