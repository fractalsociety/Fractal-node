//! Deterministic trading portfolio simulator adapter.

use async_trait::async_trait;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::collections::HashMap;

use crate::adapters::trading::fill_model::{
    fill_resting_orders, place_order, reduce_only_qty, FillIntent, RestingOrder,
};
use crate::adapters::trading::fixtures::{synthetic_bars, BarSet};
use crate::adapters::trading::ledger::{mul_micro, usdc, FillRequest, Ledger};
use crate::adapters::trading::types::{
    finite_positive, price_to_micro, qty_to_micro, Asset, Fill, OrderId, Side, TradingAction,
    TradingConfig, TradingObservation, TradingOutcome,
};
use crate::error::{Error, Result};
use crate::protocol::{AgentManifest, DatasetManifest, EnvironmentManifest, Protocol};
use crate::simulation::{
    Action, Agent, CapabilityManifest, DatasetHandle, DomainAdapter, Environment, EnvironmentStep,
    Episode, EpisodeStep, MetricSet, PolicyDecision, PublicEvidenceBundle, PublicEvidenceStep,
    ResourceRequirement, RunTrace, RuntimeState, StepResult, TerminalCondition,
    TerminalConditionType, ValidationReport,
};

/// Stable adapter identifier.
pub const TRADING_ADAPTER_ID: &str = "trading-portfolio-sim";
/// Adapter semantic version.
pub const TRADING_ADAPTER_VERSION: &str = "0.1.0";
/// Stable starter-agent identifier.
pub const STARTER_TRADING_AGENT_ID: &str = "starter-trading-agent";

/// Deterministic portfolio simulator for BTC/ETH synthetic contracts.
pub struct TradingAdapter {
    config: TradingConfig,
    bars: Vec<BarSet>,
    ledger: Ledger,
    open_orders: Vec<RestingOrder>,
    next_order_id: u64,
    step: u64,
    previous_equity_micro: i64,
    liquidated: bool,
}

impl TradingAdapter {
    /// Create an adapter using deterministic synthetic bars derived from `seed`.
    pub fn new(seed: u64, config: TradingConfig) -> Result<Self> {
        let bars = synthetic_bars(seed, config.max_steps.max(1));
        Self::with_bars(config, bars)
    }

    /// Create an adapter with explicit bars, useful for golden tests.
    pub fn with_bars(config: TradingConfig, bars: Vec<BarSet>) -> Result<Self> {
        if bars.is_empty() {
            return Err(Error::InvalidArtifact(
                "trading bars cannot be empty".to_string(),
            ));
        }
        let ledger = Ledger::new(&config)?;
        let marks = marks_for(&bars[0])?;
        let previous_equity_micro = ledger.equity_micro(&marks);
        Ok(Self {
            config,
            bars,
            ledger,
            open_orders: Vec::new(),
            next_order_id: 1,
            step: 0,
            previous_equity_micro,
            liquidated: false,
        })
    }

    /// Borrow the current ledger.
    pub fn ledger(&self) -> &Ledger {
        &self.ledger
    }

    /// Number of open resting orders.
    pub fn open_order_count(&self) -> usize {
        self.open_orders.len()
    }

    /// Current mark map.
    pub fn current_marks(&self) -> Result<HashMap<Asset, i64>> {
        marks_for(self.current_bars())
    }

    /// Current observation.
    pub fn observation(&self) -> Result<TradingObservation> {
        let bars = self.current_bars();
        let marks = marks_for(bars)?;
        Ok(TradingObservation {
            step: self.step,
            btc: bars.btc.clone(),
            eth: bars.eth.clone(),
            equity: self.ledger.equity(&marks),
            cash: self.ledger.cash(),
            positions: self.ledger.position_views(&marks),
            open_order_count: self.open_orders.len() as u64,
        })
    }

    /// Validate an action against the current portfolio state.
    pub fn validate_trading_action(&self, action: &TradingAction) -> Result<PolicyDecision> {
        action.validate()?;
        match action {
            TradingAction::Hold | TradingAction::CancelOrder { .. } => Ok(PolicyDecision::Approved),
            TradingAction::ReducePosition { asset, qty } => {
                let pos = self.ledger.position(*asset);
                if pos.qty_micro == 0 {
                    return Ok(PolicyDecision::Rejected {
                        reason: "cannot reduce a flat position".to_string(),
                    });
                }
                let requested = qty_to_micro(*qty)?;
                if requested > pos.qty_micro.abs() {
                    return Ok(PolicyDecision::Rejected {
                        reason: "reduce quantity exceeds current exposure".to_string(),
                    });
                }
                Ok(PolicyDecision::Approved)
            }
            TradingAction::PlaceOrder {
                asset,
                side,
                order_type: _,
                qty,
                limit_price,
                reduce_only,
            } => self.validate_order(*asset, *side, *qty, *limit_price, *reduce_only),
        }
    }

    fn validate_order(
        &self,
        asset: Asset,
        side: Side,
        qty: f64,
        limit_price: Option<f64>,
        reduce_only: bool,
    ) -> Result<PolicyDecision> {
        finite_positive(qty, "qty")?;
        let marks = self.current_marks()?;
        let mark = *marks
            .get(&asset)
            .ok_or_else(|| Error::InvalidAction(format!("missing mark for {}", asset.symbol())))?;
        let price = match limit_price {
            Some(value) => price_to_micro(value)?,
            None => mark,
        };
        let qty_micro = qty_to_micro(qty)?;
        let notional = mul_micro(qty_micro, price);
        if notional < price_to_micro(self.config.min_notional)? {
            return Ok(PolicyDecision::Rejected {
                reason: "notional below minimum".to_string(),
            });
        }
        let equity = self.ledger.equity_micro(&marks);
        let loss_stop = self.ledger.initial_equity_micro()
            - (self.ledger.initial_equity_micro() as f64 * self.config.daily_loss_stop_fraction)
                .round() as i64;
        if equity <= loss_stop {
            return Ok(PolicyDecision::Rejected {
                reason: "daily loss stop active".to_string(),
            });
        }
        let pos = self.ledger.position(asset);
        if reduce_only {
            let reduces = (pos.qty_micro > 0 && side == Side::Sell)
                || (pos.qty_micro < 0 && side == Side::Buy);
            if !reduces {
                return Ok(PolicyDecision::Rejected {
                    reason: "reduce-only order cannot open exposure".to_string(),
                });
            }
            return Ok(PolicyDecision::Approved);
        }
        let delta = qty_micro * side.sign();
        if pos.qty_micro != 0
            && pos.qty_micro.signum() != delta.signum()
            && delta.abs() > pos.qty_micro.abs()
        {
            return Ok(PolicyDecision::Rejected {
                reason: "non-reduce-only order would flip position sign".to_string(),
            });
        }
        let projected_qty = pos.qty_micro + delta;
        let current_asset_exposure = mul_micro(pos.qty_micro.abs(), mark);
        let projected_asset_exposure = mul_micro(projected_qty.abs(), mark);
        let projected_gross = self.ledger.gross_exposure_micro(&marks) - current_asset_exposure
            + projected_asset_exposure;
        let max_gross = (self.ledger.initial_equity_micro() as f64
            * self.config.max_initial_leverage)
            .round() as i64;
        if projected_gross > max_gross {
            return Ok(PolicyDecision::Rejected {
                reason: "max initial leverage exceeded".to_string(),
            });
        }
        Ok(PolicyDecision::Approved)
    }

    fn current_bars(&self) -> &BarSet {
        let idx = (self.step as usize).min(self.bars.len().saturating_sub(1));
        &self.bars[idx]
    }

    fn next_order_id(&mut self) -> OrderId {
        let id = OrderId(self.next_order_id);
        self.next_order_id += 1;
        id
    }

    fn execute_fill_intents(&mut self, intents: Vec<FillIntent>) -> Result<Vec<Fill>> {
        let mut fills = Vec::new();
        for intent in intents {
            let pos = self.ledger.position(intent.asset);
            let qty = if intent.reduce_only {
                reduce_only_qty(intent.qty, intent.side, pos.qty_micro)?
            } else {
                intent.qty
            };
            if qty <= 0.0 {
                continue;
            }
            fills.push(self.ledger.apply_fill(FillRequest {
                asset: intent.asset,
                side: intent.side,
                qty,
                price: intent.price,
                fee_bps: self.config.fee_bps,
                order_id: intent.order_id,
                liquidation: false,
            })?);
        }
        Ok(fills)
    }

    fn step_once(&mut self, action: TradingAction) -> Result<TradingOutcome> {
        let bars = self.current_bars().clone();
        let mut fills = Vec::new();
        for bar in [&bars.btc, &bars.eth] {
            let intents = fill_resting_orders(&mut self.open_orders, bar)?;
            fills.extend(self.execute_fill_intents(intents)?);
        }
        match action {
            TradingAction::Hold => {}
            TradingAction::CancelOrder { id } => {
                self.open_orders.retain(|order| order.id != id);
            }
            TradingAction::ReducePosition { asset, qty } => {
                let pos = self.ledger.position(asset);
                let side = if pos.qty_micro > 0 {
                    Side::Sell
                } else {
                    Side::Buy
                };
                let bar = bars.bar(asset);
                fills.extend(self.execute_fill_intents(vec![FillIntent {
                    order_id: None,
                    asset,
                    side,
                    qty,
                    price: bar.close,
                    reduce_only: true,
                }])?);
            }
            TradingAction::PlaceOrder { asset, .. } => {
                let id = self.next_order_id();
                let bar = bars.bar(asset);
                let (intent, resting) = place_order(id, &action, bar)?;
                if let Some(intent) = intent {
                    fills.extend(self.execute_fill_intents(vec![intent])?);
                }
                if let Some(resting) = resting {
                    self.open_orders.push(resting);
                }
            }
        }
        let marks = marks_for(&bars)?;
        let liquidation_threshold = (self.ledger.initial_equity_micro() as f64
            * self.config.liquidation_equity_fraction)
            .round() as i64;
        if self.ledger.equity_micro(&marks) <= liquidation_threshold
            && self.ledger.gross_exposure_micro(&marks) > 0
        {
            fills.extend(self.ledger.liquidate_all(&marks, self.config.fee_bps)?);
            self.open_orders.clear();
            self.liquidated = true;
        }
        let equity = self.ledger.equity_micro(&marks);
        let reward = (equity - self.previous_equity_micro) as f64
            / self.ledger.initial_equity_micro() as f64;
        self.previous_equity_micro = equity;
        self.step += 1;
        let done = self.liquidated
            || self.step >= self.config.max_steps
            || self.step as usize >= self.bars.len();
        Ok(self.outcome_from_marks(&marks, reward, fills, done))
    }

    fn outcome_from_marks(
        &self,
        marks: &HashMap<Asset, i64>,
        reward: f64,
        fills: Vec<Fill>,
        terminal: bool,
    ) -> TradingOutcome {
        TradingOutcome {
            reward,
            equity: self.ledger.equity(marks),
            cash: self.ledger.cash(),
            position_notional: usdc(
                [Asset::Btc, Asset::Eth]
                    .into_iter()
                    .map(|asset| {
                        let pos = self.ledger.position(asset);
                        let mark = *marks.get(&asset).unwrap_or(&pos.avg_entry_price_micro);
                        mul_micro(pos.qty_micro, mark)
                    })
                    .sum(),
            ),
            total_pnl: usdc(self.ledger.total_pnl_micro(marks)),
            realized_pnl: usdc(self.ledger.realized_pnl_micro()),
            unrealized_pnl: usdc(self.ledger.unrealized_pnl_micro(marks)),
            fees: usdc(self.ledger.fees_micro()),
            step: self.step,
            fills,
            liquidated: self.liquidated,
            terminal,
        }
    }

    /// Compute the metric set for a completed trace. Synchronous so it can be
    /// shared by `score` and `build_public_evidence` without blocking a runtime.
    /// Policy violations are counted from rejected steps in the trace.
    fn score_metrics(&self, trace: &RunTrace) -> MetricSet {
        let mut last: Option<TradingOutcome> = None;
        let mut violations = 0u64;
        for step in &trace.steps {
            if step.outcome.get("rejected").is_some() {
                violations += 1;
            }
            if let Ok(outcome) = serde_json::from_value::<TradingOutcome>(step.outcome.clone()) {
                last = Some(outcome);
            }
        }
        let final_outcome = last.unwrap_or_else(|| {
            let marks = self.current_marks().unwrap_or_default();
            self.outcome_from_marks(&marks, 0.0, Vec::new(), true)
        });
        let mut metrics = HashMap::new();
        metrics.insert("total_pnl".to_string(), final_outcome.total_pnl);
        metrics.insert("fees".to_string(), final_outcome.fees);
        metrics.insert("realized_pnl".to_string(), final_outcome.realized_pnl);
        metrics.insert("unrealized_pnl".to_string(), final_outcome.unrealized_pnl);
        metrics.insert(
            concat!("policy_", "violations").to_string(),
            violations as f64,
        );
        metrics.insert("steps".to_string(), trace.step_count() as f64);
        MetricSet {
            primary_metric: final_outcome.total_pnl / self.ledger.initial_equity(),
            metrics,
            confidence_intervals: HashMap::new(),
        }
    }
}

#[async_trait]
impl DomainAdapter for TradingAdapter {
    type Obs = TradingObservation;
    type Act = TradingAction;
    type Out = TradingOutcome;

    fn id(&self) -> (String, String) {
        (
            TRADING_ADAPTER_ID.to_string(),
            TRADING_ADAPTER_VERSION.to_string(),
        )
    }

    fn capability_manifest(&self) -> CapabilityManifest {
        CapabilityManifest {
            observation_types: vec!["ohlcv_bar".to_string(), "portfolio".to_string()],
            action_types: vec![
                "hold".to_string(),
                "place_order".to_string(),
                "cancel_order".to_string(),
                "reduce_position".to_string(),
            ],
            required_resources: vec![ResourceRequirement {
                resource_type: "synthetic_market_fixture".to_string(),
                minimum: self.bars.len() as u64,
                maximum: Some(self.bars.len() as u64),
            }],
            max_concurrent_episodes: 1,
        }
    }

    fn validate_protocol(&self, protocol: &Protocol) -> Result<ValidationReport> {
        let mut errors = Vec::new();
        if protocol.primary_metrics.is_empty() {
            errors.push("protocol must define net return as a primary metric".to_string());
        }
        Ok(ValidationReport {
            is_valid: errors.is_empty(),
            errors,
            warnings: vec!["funding accrual is deferred in PHASE-04 Slice A".to_string()],
        })
    }

    async fn resolve_dataset(&self, _manifest: &DatasetManifest) -> Result<Box<dyn DatasetHandle>> {
        Ok(Box::new(TradingDataset {
            episodes: self.bars.len(),
        }))
    }

    async fn create_environment(
        &self,
        _config: &EnvironmentManifest,
    ) -> Result<Box<dyn Environment>> {
        Ok(Box::new(TradingEnvironment {}))
    }

    async fn reset(&mut self) -> Result<Self::Obs> {
        let bars = self.bars.clone();
        *self = Self::with_bars(self.config.clone(), bars)?;
        self.observation()
    }

    fn normalize_observation(&self, raw: serde_json::Value) -> Result<Self::Obs> {
        Ok(serde_json::from_value(raw)?)
    }

    fn validate_action(&self, action: &Self::Act, _state: &RuntimeState) -> Result<PolicyDecision> {
        self.validate_trading_action(action)
    }

    async fn step(&mut self, action: Self::Act) -> Result<StepResult<Self::Obs, Self::Out>> {
        let outcome = self.step_once(action)?;
        let done = outcome.terminal;
        Ok(StepResult {
            observation: self.observation()?,
            outcome,
            done,
            info: serde_json::json!({ "open_orders": self.open_orders.len() }),
        })
    }

    async fn score(&self, trace: &RunTrace) -> Result<MetricSet> {
        Ok(self.score_metrics(trace))
    }

    fn build_public_evidence(&self, trace: &RunTrace) -> Result<PublicEvidenceBundle> {
        let steps = trace
            .steps
            .iter()
            .map(|step| PublicEvidenceStep {
                step: step.step,
                action_type: step
                    .action
                    .get("PlaceOrder")
                    .map(|_| "place_order")
                    .or_else(|| step.action.get("CancelOrder").map(|_| "cancel_order"))
                    .or_else(|| step.action.get("ReducePosition").map(|_| "reduce_position"))
                    .unwrap_or("hold")
                    .to_string(),
                outcome_type: if step.outcome.get("rejected").is_some() {
                    "rejected".to_string()
                } else {
                    "portfolio_step".to_string()
                },
                timestamp: step.timestamp,
            })
            .collect();
        Ok(PublicEvidenceBundle {
            evidence_id: trace.run_id.clone(),
            steps,
            metrics: self.score_metrics(trace),
            verifier_summary: Vec::new(),
        })
    }

    fn terminal_conditions(&self) -> Vec<TerminalCondition> {
        vec![
            TerminalCondition {
                condition_type: TerminalConditionType::MaxSteps,
                threshold: self.config.max_steps as f64,
                strict: true,
            },
            TerminalCondition {
                condition_type: TerminalConditionType::Custom {
                    name: "liquidation_threshold".to_string(),
                },
                threshold: self.config.liquidation_equity_fraction,
                strict: false,
            },
        ]
    }
}

/// Deterministic starter agent for smoke and kernel tests.
pub struct TradingAgent {
    rng: StdRng,
    step: u64,
}

impl TradingAgent {
    /// Create a starter trading agent from a deterministic seed.
    pub fn new(seed: u64) -> Self {
        Self {
            rng: StdRng::seed_from_u64(seed),
            step: 0,
        }
    }
}

#[async_trait]
impl Agent<TradingAdapter> for TradingAgent {
    fn id(&self) -> &str {
        STARTER_TRADING_AGENT_ID
    }

    async fn act(&mut self, observation: &TradingObservation) -> Result<TradingAction> {
        self.step += 1;
        if observation.step == 0 {
            return Ok(TradingAction::PlaceOrder {
                asset: Asset::Btc,
                side: Side::Buy,
                order_type: crate::adapters::trading::types::OrderType::MarketableIoc,
                qty: 0.1,
                limit_price: None,
                reduce_only: false,
            });
        }
        if self.step.is_multiple_of(5) {
            return Ok(TradingAction::ReducePosition {
                asset: Asset::Btc,
                qty: 0.02,
            });
        }
        if self.rng.gen_range(0..10) == 0 {
            Ok(TradingAction::PlaceOrder {
                asset: Asset::Eth,
                side: Side::Buy,
                order_type: crate::adapters::trading::types::OrderType::MarketableIoc,
                qty: 0.2,
                limit_price: None,
                reduce_only: false,
            })
        } else {
            Ok(TradingAction::Hold)
        }
    }

    fn manifest(&self) -> Option<AgentManifest> {
        Some(AgentManifest {
            id: STARTER_TRADING_AGENT_ID.to_string(),
            version: "0.1.0".to_string(),
            author: "fractal-society".to_string(),
            model_ref: None,
            system_prompt: None,
            code_hash: crate::protocol::Hash::of(&"TradingAgent::act deterministic v0.1.0")
                .expect("static agent hash must be canonical"),
            tool_allowlist: Vec::new(),
            skill_dependencies: Vec::new(),
            resource_limits: crate::protocol::ResourceLimits {
                max_memory_mb: 64,
                max_runtime_seconds: 10,
                max_cpu_cores: 1,
            },
            network_policy: crate::protocol::NetworkPolicy {
                allow_network: false,
                allowed_domains: Vec::new(),
            },
            license: "Apache-2.0".to_string(),
        })
    }
}

/// Minimal dataset handle for synthetic fixtures.
pub struct TradingDataset {
    episodes: usize,
}

#[async_trait]
impl DatasetHandle for TradingDataset {
    fn episode_count(&self) -> usize {
        self.episodes
    }

    async fn get_episode(&self, _index: usize) -> Result<Box<dyn Episode>> {
        Ok(Box::new(TradingEpisode {}))
    }
}

/// Minimal synthetic episode.
pub struct TradingEpisode {}

#[async_trait]
impl Episode for TradingEpisode {
    async fn reset(&mut self) -> Result<()> {
        Ok(())
    }

    fn current_observation(&self) -> Result<serde_json::Value> {
        Ok(serde_json::json!({ "synthetic": true }))
    }

    async fn step(&mut self, _action: serde_json::Value) -> Result<EpisodeStep> {
        Ok(EpisodeStep {
            observation: serde_json::json!({ "synthetic": true }),
            reward: 0.0,
            done: true,
            info: serde_json::json!({}),
        })
    }
}

/// Minimal untyped trading environment.
pub struct TradingEnvironment {}

#[async_trait]
impl Environment for TradingEnvironment {
    async fn reset(&mut self) -> Result<()> {
        Ok(())
    }

    fn observation(&self) -> Result<serde_json::Value> {
        Ok(serde_json::json!({ "synthetic": true }))
    }

    async fn execute(&mut self, _action: serde_json::Value) -> Result<EnvironmentStep> {
        Ok(EnvironmentStep {
            observation: serde_json::json!({ "synthetic": true }),
            reward: 0.0,
            terminal: true,
            truncated: false,
            info: serde_json::json!({}),
        })
    }

    async fn close(&mut self) -> Result<()> {
        Ok(())
    }
}

fn marks_for(bars: &BarSet) -> Result<HashMap<Asset, i64>> {
    let mut marks = HashMap::new();
    marks.insert(Asset::Btc, bars.btc.close_micro()?);
    marks.insert(Asset::Eth, bars.eth.close_micro()?);
    Ok(marks)
}
