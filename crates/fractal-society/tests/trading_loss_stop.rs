use fractal_society::adapters::trading::{
    liquidation_bars, Asset, OrderType, Side, TradingAction, TradingAdapter, TradingConfig,
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

fn small_buy() -> TradingAction {
    TradingAction::PlaceOrder {
        asset: Asset::Btc,
        side: Side::Buy,
        order_type: OrderType::MarketableIoc,
        qty: 0.1,
        limit_price: None,
        reduce_only: false,
    }
}

#[tokio::test]
async fn daily_loss_stop_rejects_new_orders_only_after_equity_drop() {
    let config = TradingConfig {
        max_steps: 10,
        ..TradingConfig::default()
    };
    let mut adapter = TradingAdapter::with_bars(config, liquidation_bars(10)).unwrap();
    adapter.reset().await.unwrap();

    // Open a long; liquidation_bars falls each step.
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

    // Immediately after opening, equity is still above the 10% loss stop.
    let before = adapter
        .validate_action(&small_buy(), &runtime_state())
        .unwrap();
    assert!(
        matches!(before, PolicyDecision::Approved),
        "expected Approved before loss stop, got {before:?}"
    );

    // Step through falling bars until equity crosses the loss stop.
    let mut triggered = false;
    for _ in 0..8 {
        adapter.step(TradingAction::Hold).await.unwrap();
        if matches!(
            adapter
                .validate_action(&small_buy(), &runtime_state())
                .unwrap(),
            PolicyDecision::Rejected { .. }
        ) {
            triggered = true;
            break;
        }
    }
    assert!(triggered, "daily loss stop never triggered");
}
