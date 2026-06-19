use fractal_society::adapters::trading::{TradingAdapter, TradingAgent, TradingConfig};
use fractal_society::kernel::{run, KernelConfig};

#[tokio::test]
async fn p04_r01_trading_adapter_runs_through_kernel_deterministically() {
    let config = KernelConfig {
        episodes: 1,
        max_steps_per_episode: 12,
    };
    let mut hash = None;
    for _ in 0..100 {
        let outcome = run(
            TradingAdapter::new(
                91,
                TradingConfig {
                    max_steps: 12,
                    ..TradingConfig::default()
                },
            )
            .unwrap(),
            TradingAgent::new(17),
            123,
            &config,
        )
        .await
        .unwrap();
        if let Some(expected) = &hash {
            assert_eq!(expected, &outcome.evidence_hash);
        } else {
            println!("evidence_hash={}", outcome.evidence_hash.0);
            hash = Some(outcome.evidence_hash);
        }
    }
    let different = run(
        TradingAdapter::new(
            92,
            TradingConfig {
                max_steps: 12,
                ..TradingConfig::default()
            },
        )
        .unwrap(),
        TradingAgent::new(18),
        124,
        &config,
    )
    .await
    .unwrap();
    assert_ne!(hash.unwrap(), different.evidence_hash);
}
