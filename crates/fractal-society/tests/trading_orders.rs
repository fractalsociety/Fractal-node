use fractal_society::adapters::trading::{
    golden_bars, Asset, OrderId, OrderType, Side, TradingAction, TradingAdapter, TradingConfig,
};
use fractal_society::simulation::DomainAdapter;

async fn partial_then_cancel() -> Vec<serde_json::Value> {
    let mut adapter = TradingAdapter::with_bars(TradingConfig::default(), golden_bars()).unwrap();
    adapter.reset().await.unwrap();
    let a = adapter
        .step(TradingAction::PlaceOrder {
            asset: Asset::Btc,
            side: Side::Buy,
            order_type: OrderType::LimitGtc,
            qty: 1.0,
            limit_price: Some(110.0),
            reduce_only: false,
        })
        .await
        .unwrap();
    let b = adapter.step(TradingAction::Hold).await.unwrap();
    let c = adapter
        .step(TradingAction::CancelOrder { id: OrderId(1) })
        .await
        .unwrap();
    vec![
        serde_json::to_value(a.outcome).unwrap(),
        serde_json::to_value(b.outcome).unwrap(),
        serde_json::to_value(c.outcome).unwrap(),
    ]
}

#[tokio::test]
async fn p04_n04_partial_fills_and_cancels_are_deterministic() {
    let first = partial_then_cancel().await;
    let second = partial_then_cancel().await;
    assert_eq!(first, second);
    assert_eq!(first[1]["fills"][0]["qty"], serde_json::json!(0.5));
}
