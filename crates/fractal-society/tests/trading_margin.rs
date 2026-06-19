use fractal_society::adapters::trading::{
    liquidation_bars, Asset, OrderType, Side, TradingAction, TradingAdapter, TradingConfig,
};
use fractal_society::simulation::DomainAdapter;

#[tokio::test]
async fn p04_n05_losing_position_liquidates_to_hand_calculated_residual() {
    let config = TradingConfig {
        liquidation_equity_fraction: 0.95,
        max_steps: 3,
        ..TradingConfig::default()
    };
    let mut adapter = TradingAdapter::with_bars(config, liquidation_bars(3)).unwrap();
    adapter.reset().await.unwrap();
    adapter
        .step(TradingAction::PlaceOrder {
            asset: Asset::Btc,
            side: Side::Buy,
            order_type: OrderType::MarketableIoc,
            qty: 2.0,
            limit_price: None,
            reduce_only: false,
        })
        .await
        .unwrap();
    let liquidated = adapter.step(TradingAction::Hold).await.unwrap();
    assert!(liquidated.outcome.liquidated);
    assert!((liquidated.outcome.equity - 89_915.0).abs() <= 0.000_001);
}
