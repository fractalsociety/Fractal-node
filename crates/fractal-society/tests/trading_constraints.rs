use fractal_society::adapters::trading::{
    Asset, OrderType, Side, TradingAction, TradingAdapter, TradingConfig,
};
use fractal_society::simulation::{DomainAdapter, PolicyDecision, RuntimeState};

fn runtime_state() -> RuntimeState {
    RuntimeState {
        episode: 0,
        step: 0,
        reward: 0.0,
        state_data: serde_json::Value::Null,
    }
}

fn rejected(decision: PolicyDecision) -> bool {
    matches!(decision, PolicyDecision::Rejected { .. })
}

#[tokio::test]
async fn p04_n02_constraints_reject_unsafe_orders() {
    let mut adapter = TradingAdapter::new(7, TradingConfig::default()).unwrap();
    adapter.reset().await.unwrap();
    let too_much = TradingAction::PlaceOrder {
        asset: Asset::Btc,
        side: Side::Buy,
        order_type: OrderType::MarketableIoc,
        qty: 5.0,
        limit_price: None,
        reduce_only: false,
    };
    assert!(rejected(
        adapter
            .validate_action(&too_much, &runtime_state())
            .unwrap()
    ));

    let dust = TradingAction::PlaceOrder {
        asset: Asset::Eth,
        side: Side::Buy,
        order_type: OrderType::MarketableIoc,
        qty: 0.000_001,
        limit_price: None,
        reduce_only: false,
    };
    assert!(rejected(
        adapter.validate_action(&dust, &runtime_state()).unwrap()
    ));

    let reduce_only_open = TradingAction::PlaceOrder {
        asset: Asset::Btc,
        side: Side::Sell,
        order_type: OrderType::MarketableIoc,
        qty: 0.1,
        limit_price: None,
        reduce_only: true,
    };
    assert!(rejected(
        adapter
            .validate_action(&reduce_only_open, &runtime_state())
            .unwrap()
    ));

    adapter
        .step(TradingAction::PlaceOrder {
            asset: Asset::Btc,
            side: Side::Buy,
            order_type: OrderType::MarketableIoc,
            qty: 0.5,
            limit_price: None,
            reduce_only: false,
        })
        .await
        .unwrap();
    let sign_flip = TradingAction::PlaceOrder {
        asset: Asset::Btc,
        side: Side::Sell,
        order_type: OrderType::MarketableIoc,
        qty: 0.6,
        limit_price: None,
        reduce_only: false,
    };
    assert!(rejected(
        adapter
            .validate_action(&sign_flip, &runtime_state())
            .unwrap()
    ));
}
