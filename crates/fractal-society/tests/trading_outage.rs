//! PHASE-04 gate P04-N06: new exposure is rejected on stale (outage) bars,
//! risk reduction / other-asset orders are unaffected, and an outage run is
//! deterministic.

use fractal_society::adapters::trading::{
    outage_bars, Asset, OrderType, Side, TradingAction, TradingAdapter, TradingAgent, TradingConfig,
};
use fractal_society::kernel::{run, KernelConfig};
use fractal_society::simulation::{DomainAdapter, PolicyDecision, RuntimeState};

fn rs() -> RuntimeState {
    RuntimeState {
        episode: 0,
        step: 0,
        reward: 0.0,
        state_data: serde_json::Value::Null,
    }
}

fn buy_btc() -> TradingAction {
    TradingAction::PlaceOrder {
        asset: Asset::Btc,
        side: Side::Buy,
        order_type: OrderType::MarketableIoc,
        qty: 1.0,
        limit_price: None,
        reduce_only: false,
    }
}

fn buy_eth() -> TradingAction {
    TradingAction::PlaceOrder {
        asset: Asset::Eth,
        side: Side::Buy,
        order_type: OrderType::MarketableIoc,
        qty: 2.0,
        limit_price: None,
        reduce_only: false,
    }
}

fn is_approved(d: PolicyDecision) -> bool {
    matches!(d, PolicyDecision::Approved)
}

fn is_rejected(d: PolicyDecision) -> bool {
    matches!(d, PolicyDecision::Rejected { .. })
}

#[tokio::test]
async fn outage_rejects_new_exposure_only_on_stale_bars() {
    let mut adapter = TradingAdapter::with_bars(TradingConfig::default(), outage_bars(4)).unwrap();
    adapter.reset().await.unwrap();

    // Step 0: BTC is stale (outage), ETH is fresh.
    assert!(is_rejected(
        adapter.validate_action(&buy_btc(), &rs()).unwrap()
    ));
    assert!(is_approved(
        adapter.validate_action(&buy_eth(), &rs()).unwrap()
    ));

    // Advance to step 1: BTC is fresh.
    adapter.step(TradingAction::Hold).await.unwrap();
    assert!(is_approved(
        adapter.validate_action(&buy_btc(), &rs()).unwrap()
    ));
}

#[tokio::test]
async fn outage_run_is_deterministic() {
    let tcfg = TradingConfig {
        max_steps: 12,
        ..TradingConfig::default()
    };
    let kcfg = KernelConfig {
        episodes: 1,
        max_steps_per_episode: 12,
    };
    let a = run(
        TradingAdapter::with_bars(tcfg.clone(), outage_bars(12)).unwrap(),
        TradingAgent::new(5),
        5,
        &kcfg,
    )
    .await
    .unwrap();
    let b = run(
        TradingAdapter::with_bars(tcfg.clone(), outage_bars(12)).unwrap(),
        TradingAgent::new(5),
        5,
        &kcfg,
    )
    .await
    .unwrap();
    assert_eq!(a.evidence_hash, b.evidence_hash);
    // The starter agent buys BTC at step 0, which is stale -> at least one rejection.
    assert!(a.metrics.metrics["policy_violations"] > 0.0);
}
