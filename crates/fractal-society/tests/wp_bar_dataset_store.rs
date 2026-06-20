//! Package 74 — bar-dataset store: round-trip + pipeline drive.

use std::path::Path;

use fractal_society::adapters::trading::fixtures::BarSet;
use fractal_society::adapters::trading::{
    synthetic_bars, CashBaseline, TradingAdapter, TradingAgent, TradingConfig,
};
use fractal_society::kernel::{run, KernelConfig};
use fractal_society::market_data::store::{read_barsets, write_barsets};
use fractal_society::pipeline::run_pipeline_default;
use fractal_society::protocol::Hash;
use fractal_society::signing::AuthorSigner;

const SEED: u64 = 42;
const STEPS: u64 = 12;

fn dataset() -> Vec<BarSet> {
    // Reuse the deterministic synthetic fixture as the "recorded" dataset.
    synthetic_bars(SEED, STEPS)
}

#[test]
fn roundtrip_preserves_bars_and_is_byte_stable() {
    let path = std::env::temp_dir().join("fractal_wp_bar_dataset_store.jsonl");
    let original = dataset();

    write_barsets(&path, &original).unwrap();
    let read_back = read_barsets(&path).unwrap();
    assert_eq!(read_back, original, "round-trip must preserve bar sets");

    // Byte-stable: writing the same data again produces identical bytes.
    let first = std::fs::read(&path).unwrap();
    write_barsets(&path, &original).unwrap();
    let second = std::fs::read(&path).unwrap();
    assert_eq!(first, second, "serialized dataset must be byte-stable");

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn recorded_dataset_drives_pipeline_deterministically() {
    let path = std::env::temp_dir().join("fractal_wp_bar_dataset_store_run.jsonl");
    write_barsets(&path, &dataset()).unwrap();

    async fn run_once(path: &Path) -> (Hash, Hash) {
        let recorded = read_barsets(path).unwrap();
        let tcfg = TradingConfig {
            max_steps: STEPS,
            ..TradingConfig::default()
        };
        let kcfg = KernelConfig {
            episodes: 1,
            max_steps_per_episode: STEPS,
        };
        let signer = AuthorSigner::from_seed(&[2u8; 32]);
        // Baseline shares the candidate's recorded bars.
        let cash = run(
            TradingAdapter::with_bars(tcfg.clone(), recorded.clone()).unwrap(),
            CashBaseline::new(),
            SEED,
            &kcfg,
        )
        .await
        .unwrap();
        let result = run_pipeline_default(
            TradingAdapter::with_bars(tcfg.clone(), recorded).unwrap(),
            TradingAgent::new(SEED),
            SEED,
            kcfg,
            tcfg,
            vec![("cash".to_string(), cash)],
            &signer,
            chrono::DateTime::from_timestamp(0, 0).unwrap(),
        )
        .await
        .unwrap();
        (
            Hash::of(&result.proof_manifest).unwrap(),
            result.bundle.bundle_hash().unwrap(),
        )
    }

    let first = run_once(&path).await;
    let second = run_once(&path).await;
    assert_eq!(
        first.0, second.0,
        "proof hash must be deterministic on recorded data"
    );
    assert_eq!(
        first.1, second.1,
        "bundle hash must be deterministic on recorded data"
    );

    let _ = std::fs::remove_file(&path);
}
