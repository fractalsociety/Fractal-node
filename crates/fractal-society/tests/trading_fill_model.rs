use fractal_society::adapters::trading::{
    golden_bars, Asset, OrderType, Side, TradingAction, TradingAdapter, TradingConfig,
};
use fractal_society::simulation::DomainAdapter;

#[tokio::test]
async fn p04_n03_limit_above_high_does_not_fill_and_ioc_fills_at_close() {
    let mut limit_adapter =
        TradingAdapter::with_bars(TradingConfig::default(), golden_bars()).unwrap();
    limit_adapter.reset().await.unwrap();
    let limit = limit_adapter
        .step(TradingAction::PlaceOrder {
            asset: Asset::Btc,
            side: Side::Buy,
            order_type: OrderType::LimitGtc,
            qty: 1.0,
            limit_price: Some(106.0),
            reduce_only: false,
        })
        .await
        .unwrap();
    assert!(limit.outcome.fills.is_empty());
    assert_eq!(limit_adapter.open_order_count(), 1);

    let mut ioc_adapter =
        TradingAdapter::with_bars(TradingConfig::default(), golden_bars()).unwrap();
    ioc_adapter.reset().await.unwrap();
    let ioc = ioc_adapter
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
    assert_eq!(ioc.outcome.fills.len(), 1);
    assert_eq!(ioc.outcome.fills[0].price, 100.0);
}
