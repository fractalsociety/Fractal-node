use fractal_society::adapters::trading::{
    Asset, OrderType, Side, TradingAction, TradingAdapter, TradingConfig,
};
use fractal_society::simulation::{DomainAdapter, RunTrace};

#[tokio::test]
async fn build_public_evidence_classifies_and_redacts() {
    let mut adapter = TradingAdapter::new(7, TradingConfig::default()).unwrap();
    adapter.reset().await.unwrap();

    let place = TradingAction::PlaceOrder {
        asset: Asset::Btc,
        side: Side::Buy,
        order_type: OrderType::MarketableIoc,
        qty: 1.0,
        limit_price: None,
        reduce_only: false,
    };
    let ts = chrono::DateTime::from_timestamp(0, 0).unwrap();
    let mut trace = RunTrace::new("pub-test");
    trace.record_step(
        0,
        serde_json::json!({}),
        serde_json::to_value(&place).unwrap(),
        serde_json::json!({"rejected":"x"}),
        ts,
    );

    let public = adapter.build_public_evidence(&trace).unwrap();
    assert_eq!(public.steps.len(), 1);
    assert_eq!(public.steps[0].action_type, "place_order");
    assert_eq!(public.steps[0].outcome_type, "rejected");

    // Content-level redaction: no per-step strategy/account detail may leak.
    let json = serde_json::to_string(&public).unwrap();
    for secret in ["qty", "limit_price", "equity", "cash", "position_notional"] {
        assert!(
            !json.contains(secret),
            "public evidence leaked '{secret}': {json}"
        );
    }
}
