use fractal_society::adapters::trading::{
    golden_bars, Asset, OrderType, Side, TradingAction, TradingAdapter, TradingConfig,
};
use fractal_society::simulation::DomainAdapter;

async fn golden_round_trip(fee_bps: u32) -> fractal_society::adapters::trading::TradingOutcome {
    let mut adapter = TradingAdapter::with_bars(
        TradingConfig {
            fee_bps,
            ..TradingConfig::default()
        },
        golden_bars(),
    )
    .unwrap();
    adapter.reset().await.unwrap();
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
    adapter
        .step(TradingAction::ReducePosition {
            asset: Asset::Btc,
            qty: 1.0,
        })
        .await
        .unwrap()
        .outcome
}

#[tokio::test]
async fn p04_n07_cost_inclusive_pnl_differs_by_fee_total() {
    let cost_free = golden_round_trip(0).await;
    let cost_inclusive = golden_round_trip(5).await;
    assert!(
        (cost_free.total_pnl - cost_inclusive.total_pnl - cost_inclusive.fees).abs() <= 0.000_001
    );
    assert!((cost_inclusive.fees - 0.105).abs() <= 0.000_001);
}
