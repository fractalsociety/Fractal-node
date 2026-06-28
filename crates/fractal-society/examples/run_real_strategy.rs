//! Run a real MA-crossover strategy on real Hyperliquid data through the full
//! research pipeline, and print the proof card.
//!
//! Prereqs: /tmp/hl_btc_candles.json + /tmp/hl_eth_candles.json (fetched from
//! Hyperliquid's candleSnapshot API).

use std::fs;

use fractal_society::adapters::trading::fixtures::BarSet;
use fractal_society::adapters::trading::{
    Asset, BuyAndHoldBaseline, CashBaseline, MarketBar, OrderType, Side, TradingAction,
    TradingAdapter, TradingConfig, TradingObservation,
};
use fractal_society::error::Result;
use fractal_society::kernel::{run, KernelConfig, RunOutcome};
use fractal_society::pipeline::run_pipeline_default;
use fractal_society::pkgs::proof_card;
use fractal_society::signing::AuthorSigner;
use fractal_society::simulation::Agent;

/// Hyperliquid candle shape (subset of fields we need).
#[derive(serde::Deserialize)]
struct HlCandle {
    /// Open time (ms).
    t: u64,
    /// Open price.
    o: String,
    /// Close price.
    c: String,
    /// High price.
    h: String,
    /// Low price.
    l: String,
    /// Volume.
    v: String,
}

/// MA-crossover strategy: go long when fast MA > slow MA; exit when it crosses back.
struct MaCrossAgent {
    /// Rolling price window.
    window: Vec<f64>,
    /// Fast MA period.
    fast: usize,
    /// Slow MA period.
    slow: usize,
    /// Whether we currently hold BTC.
    in_position: bool,
}

impl MaCrossAgent {
    /// Create with fast/slow MA periods.
    fn new(fast: usize, slow: usize) -> Self {
        Self {
            window: Vec::new(),
            fast,
            slow,
            in_position: false,
        }
    }
}

#[async_trait::async_trait]
impl Agent<TradingAdapter> for MaCrossAgent {
    fn id(&self) -> &str {
        "ma-cross-5-15"
    }

    async fn act(&mut self, obs: &TradingObservation) -> Result<TradingAction> {
        let price = obs.btc.close;
        self.window.push(price);

        if self.window.len() < self.slow {
            return Ok(TradingAction::Hold);
        }

        let start = self.window.len() - self.slow;
        let slice = &self.window[start..];
        let slow_ma = slice.iter().sum::<f64>() / self.slow as f64;
        let fast_ma = slice[self.slow - self.fast..].iter().sum::<f64>() / self.fast as f64;

        if fast_ma > slow_ma && !self.in_position {
            self.in_position = true;
            Ok(TradingAction::PlaceOrder {
                asset: Asset::Btc,
                side: Side::Buy,
                order_type: OrderType::MarketableIoc,
                qty: 1.0,
                limit_price: None,
                reduce_only: false,
            })
        } else if fast_ma < slow_ma && self.in_position {
            self.in_position = false;
            Ok(TradingAction::ReducePosition {
                asset: Asset::Btc,
                qty: 1.0,
            })
        } else {
            Ok(TradingAction::Hold)
        }
    }
}

/// Load candles from a JSON file.
fn load_candles(path: &str) -> Vec<HlCandle> {
    let data = fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    serde_json::from_str(&data).unwrap_or_else(|e| panic!("parse {path}: {e}"))
}

/// Convert Hyperliquid candles to BarSets (BTC + ETH paired by index).
fn hl_to_barsets(btc: &[HlCandle], eth: &[HlCandle]) -> Vec<BarSet> {
    let len = btc.len().min(eth.len());
    (0..len)
        .map(|i| BarSet {
            step: i as u64,
            btc: MarketBar {
                ts: (btc[i].t / 1000) as i64,
                asset: Asset::Btc,
                open: btc[i].o.parse().unwrap(),
                high: btc[i].h.parse().unwrap(),
                low: btc[i].l.parse().unwrap(),
                close: btc[i].c.parse().unwrap(),
                volume: btc[i].v.parse().unwrap(),
                stale: false,
                funding_rate: 0.0,
            },
            eth: MarketBar {
                ts: (eth[i].t / 1000) as i64,
                asset: Asset::Eth,
                open: eth[i].o.parse().unwrap(),
                high: eth[i].h.parse().unwrap(),
                low: eth[i].l.parse().unwrap(),
                close: eth[i].c.parse().unwrap(),
                volume: eth[i].v.parse().unwrap(),
                stale: false,
                funding_rate: 0.0,
            },
        })
        .collect()
}

/// Run a baseline agent on the same bars.
async fn run_baseline<A: Agent<TradingAdapter>>(
    name: &str,
    agent: A,
    bars: &[BarSet],
    tcfg: &TradingConfig,
) -> (String, RunOutcome) {
    let kcfg = KernelConfig {
        episodes: 1,
        max_steps_per_episode: bars.len() as u64,
    };
    let outcome = run(
        TradingAdapter::with_bars(tcfg.clone(), bars.to_vec()).unwrap(),
        agent,
        0,
        &kcfg,
    )
    .await
    .unwrap();
    (name.to_string(), outcome)
}

#[tokio::main]
async fn main() {
    // 1. Load real Hyperliquid data.
    let btc = load_candles("/tmp/hl_btc_candles.json");
    let eth = load_candles("/tmp/hl_eth_candles.json");
    let bars = hl_to_barsets(&btc, &eth);
    println!(
        "Loaded {} real bars (BTC + ETH) from Hyperliquid",
        bars.len()
    );
    println!(
        "BTC range: ${:.0} → ${:.0}",
        bars.first().unwrap().btc.open,
        bars.last().unwrap().btc.close
    );

    let steps = bars.len() as u64;
    let tcfg = TradingConfig {
        max_steps: steps,
        ..TradingConfig::default()
    };
    let kcfg = KernelConfig {
        episodes: 1,
        max_steps_per_episode: steps,
    };
    let signer = AuthorSigner::from_seed(&[7u8; 32]);
    let ts = chrono::DateTime::from_timestamp(0, 0).unwrap();

    // 2. Run baselines on the same real data.
    println!("\nRunning baselines on real data...");
    let baselines = vec![
        run_baseline("cash", CashBaseline::new(), &bars, &tcfg).await,
        run_baseline("buy-and-hold", BuyAndHoldBaseline::new(), &bars, &tcfg).await,
    ];

    // 3. Run the MA-crossover strategy through the full pipeline.
    println!("Running MA-cross(5,15) strategy through the pipeline...");
    let result = run_pipeline_default(
        TradingAdapter::with_bars(tcfg.clone(), bars.clone()).unwrap(),
        MaCrossAgent::new(5, 15),
        0,
        kcfg,
        tcfg,
        baselines,
        &signer,
        ts,
    )
    .await
    .expect("pipeline must complete");

    // 4. Verify the proof.
    let pk = signer.public_key();
    result
        .proof_manifest
        .verify_author(&pk)
        .expect("signed proof must verify");

    // 5. Print the proof card.
    let card = proof_card::build(&result.proof_manifest, &result.scorecard);
    let all_passed = result.verifier_reports.iter().all(|r| r.passed);

    println!("\n{}", "=".repeat(60));
    println!("  PROOF CARD — MA-Cross(5,15) on Real Hyperliquid Data");
    println!("{}", "=".repeat(60));
    println!("claim             : {}", card.claim);
    println!("proof level       : {}", card.proof_level);
    println!("simulation tier   : S1 (real candle replay)");
    println!("agent             : {}", result.run.manifest.agent_id);
    println!("bars              : {} (1-minute BTC/ETH)", steps);
    println!();
    println!(
        "net return        : {:.6} ({:.4}%)",
        card.net_return,
        card.net_return * 100.0
    );
    println!(
        "max drawdown      : {:.6} ({:.4}%)",
        card.max_drawdown,
        card.max_drawdown * 100.0
    );
    println!();
    println!("--- baseline comparison ---");
    for (name, baseline) in &result.scorecard.baselines {
        let better = if baseline.is_better {
            "✓ BEATS"
        } else {
            "✗ below"
        };
        println!(
            "  vs {:<12}: {:>10.6}  (diff {:>+10.6}, {})",
            name, baseline.baseline_value, baseline.difference, better
        );
    }
    println!();
    println!("--- verification ---");
    println!("verifiers run     : {}", result.verifier_reports.len());
    println!("all passed        : {}", all_passed);
    for report in &result.verifier_reports {
        let status = if report.passed {
            "✓ PASS"
        } else {
            "✗ FAIL"
        };
        println!("  {:<25} {}", report.verifier_id, status);
    }
    println!();
    println!("reward released   : {}", result.outcome.reward_released);
    println!("pipeline complete : {}", result.outcome.is_complete());
    println!();
    println!("proof hash        : {}", card.proof_hash.0);
    println!(
        "bundle hash       : {}",
        result.bundle.bundle_hash().unwrap().0
    );
    println!();
    println!("{}", card.disclaimer);
}
