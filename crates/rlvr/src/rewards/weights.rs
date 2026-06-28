//! RLVR-025: configurable reward weights.
//!
//! Each training target (router, assistant, critic, compressor, tool-use) weighs
//! the [`RewardVector`] dimensions differently. Weights load from YAML so that
//! "changing reward config changes the final reward without code edits":
//! [`RewardWeights::weighted_reward`] and [`apply_reward_weights`] recompute an
//! artifact's `final_reward` from a [`RewardWeights`] profile, and
//! [`RewardWeightProfiles`] holds one profile per training target.

use serde::{Deserialize, Serialize};

use super::RewardVectorArtifact;
use crate::{RewardVector, RlvrError};

/// The five RLVR training targets, each with its own reward-weight profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TrainingTarget {
    Router,
    Assistant,
    Critic,
    Compressor,
    ToolUse,
}

impl TrainingTarget {
    pub const ALL: [TrainingTarget; 5] = [
        TrainingTarget::Router,
        TrainingTarget::Assistant,
        TrainingTarget::Critic,
        TrainingTarget::Compressor,
        TrainingTarget::ToolUse,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            TrainingTarget::Router => "router",
            TrainingTarget::Assistant => "assistant",
            TrainingTarget::Critic => "critic",
            TrainingTarget::Compressor => "compressor",
            TrainingTarget::ToolUse => "tool_use",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "router" => Some(Self::Router),
            "assistant" => Some(Self::Assistant),
            "critic" => Some(Self::Critic),
            "compressor" => Some(Self::Compressor),
            "tool_use" | "tooluse" | "tool-use" => Some(Self::ToolUse),
            _ => None,
        }
    }
}

/// Weights over the ten [`RewardVector`] dimensions for one training target.
///
/// `weighted_reward` computes `sum(w_i * v_i) / sum(w_i)`, so the absolute scale
/// of the weights is irrelevant — only their relative emphasis matters. A
/// uniform profile (all `1.0`) reproduces the unweighted average.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RewardWeights {
    pub correctness: f64,
    pub checkpoint_coverage: f64,
    pub clarification_quality: f64,
    pub false_premise_detection: f64,
    pub route_correctness: f64,
    pub tool_use_correctness: f64,
    pub cost_efficiency: f64,
    pub latency_efficiency: f64,
    pub privacy_compliance: f64,
    pub non_redundancy: f64,
}

impl RewardWeights {
    /// Uniform weights — reproduces the unweighted reward average.
    pub fn uniform() -> Self {
        Self {
            correctness: 1.0,
            checkpoint_coverage: 1.0,
            clarification_quality: 1.0,
            false_premise_detection: 1.0,
            route_correctness: 1.0,
            tool_use_correctness: 1.0,
            cost_efficiency: 1.0,
            latency_efficiency: 1.0,
            privacy_compliance: 1.0,
            non_redundancy: 1.0,
        }
    }

    /// Router training emphasises route, cost, latency, privacy, and tool use.
    pub fn router() -> Self {
        Self {
            correctness: 1.0,
            checkpoint_coverage: 0.5,
            clarification_quality: 0.5,
            false_premise_detection: 0.5,
            route_correctness: 3.0,
            tool_use_correctness: 1.0,
            cost_efficiency: 2.0,
            latency_efficiency: 2.0,
            privacy_compliance: 2.5,
            non_redundancy: 0.5,
        }
    }

    /// Assistant training emphasises answer correctness, coverage, and clarification.
    pub fn assistant() -> Self {
        Self {
            correctness: 3.0,
            checkpoint_coverage: 2.0,
            clarification_quality: 2.0,
            false_premise_detection: 1.5,
            route_correctness: 0.5,
            tool_use_correctness: 0.5,
            cost_efficiency: 0.5,
            latency_efficiency: 0.5,
            privacy_compliance: 1.0,
            non_redundancy: 1.5,
        }
    }

    /// Critic training emphasises false-premise detection and coverage.
    pub fn critic() -> Self {
        Self {
            correctness: 1.5,
            checkpoint_coverage: 2.5,
            clarification_quality: 1.0,
            false_premise_detection: 3.0,
            route_correctness: 0.5,
            tool_use_correctness: 0.5,
            cost_efficiency: 0.5,
            latency_efficiency: 0.5,
            privacy_compliance: 1.0,
            non_redundancy: 1.5,
        }
    }

    /// Compressor training emphasises non-redundancy and fidelity.
    pub fn compressor() -> Self {
        Self {
            correctness: 2.5,
            checkpoint_coverage: 1.5,
            clarification_quality: 0.5,
            false_premise_detection: 1.0,
            route_correctness: 0.5,
            tool_use_correctness: 0.5,
            cost_efficiency: 1.0,
            latency_efficiency: 1.0,
            privacy_compliance: 1.0,
            non_redundancy: 3.0,
        }
    }

    /// Tool-use training emphasises correct tool usage and cost efficiency.
    pub fn tool_use() -> Self {
        Self {
            correctness: 2.0,
            checkpoint_coverage: 1.5,
            clarification_quality: 0.5,
            false_premise_detection: 0.5,
            route_correctness: 1.0,
            tool_use_correctness: 3.0,
            cost_efficiency: 1.5,
            latency_efficiency: 1.0,
            privacy_compliance: 1.5,
            non_redundancy: 1.0,
        }
    }

    pub fn for_target(target: TrainingTarget) -> Self {
        match target {
            TrainingTarget::Router => Self::router(),
            TrainingTarget::Assistant => Self::assistant(),
            TrainingTarget::Critic => Self::critic(),
            TrainingTarget::Compressor => Self::compressor(),
            TrainingTarget::ToolUse => Self::tool_use(),
        }
    }

    pub fn validate(&self) -> Result<(), RlvrError> {
        for (name, value) in self.dimensions() {
            require_finite_non_negative(&format!("reward_weights.{name}"), value)?;
        }
        if self.sum() == 0.0 {
            return Err(RlvrError::Config(
                "reward_weights cannot all be zero (sum must be greater than zero)".into(),
            ));
        }
        Ok(())
    }

    /// `sum(w_i * v_i) / sum(w_i)`, clamped to `[0, 1]`.
    pub fn weighted_reward(&self, vector: &RewardVector) -> f64 {
        let w = self.dimensions();
        let v = vector.dimensions();
        let numerator = w
            .iter()
            .zip(v.iter())
            .map(|((_, weight), value)| weight * value)
            .sum::<f64>();
        let denominator = self.sum();
        if denominator == 0.0 {
            return 0.0;
        }
        (numerator / denominator).clamp(0.0, 1.0)
    }

    fn dimensions(&self) -> [(&'static str, f64); 10] {
        [
            ("correctness", self.correctness),
            ("checkpoint_coverage", self.checkpoint_coverage),
            ("clarification_quality", self.clarification_quality),
            ("false_premise_detection", self.false_premise_detection),
            ("route_correctness", self.route_correctness),
            ("tool_use_correctness", self.tool_use_correctness),
            ("cost_efficiency", self.cost_efficiency),
            ("latency_efficiency", self.latency_efficiency),
            ("privacy_compliance", self.privacy_compliance),
            ("non_redundancy", self.non_redundancy),
        ]
    }

    fn set_dimension(&mut self, name: &str, value: f64) -> Result<(), RlvrError> {
        match name.trim() {
            "correctness" => self.correctness = value,
            "checkpoint_coverage" => self.checkpoint_coverage = value,
            "clarification_quality" => self.clarification_quality = value,
            "false_premise_detection" => self.false_premise_detection = value,
            "route_correctness" => self.route_correctness = value,
            "tool_use_correctness" => self.tool_use_correctness = value,
            "cost_efficiency" => self.cost_efficiency = value,
            "latency_efficiency" => self.latency_efficiency = value,
            "privacy_compliance" => self.privacy_compliance = value,
            "non_redundancy" => self.non_redundancy = value,
            other => {
                return Err(RlvrError::Config(format!(
                    "unknown reward weight dimension {other:?}"
                )));
            }
        }
        Ok(())
    }

    fn sum(&self) -> f64 {
        self.dimensions().iter().map(|(_, value)| value).sum()
    }

    /// Serialize as flat `key: value` YAML.
    pub fn to_yaml(&self) -> String {
        self.dimensions()
            .iter()
            .map(|(name, value)| format!("{name}: {value}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Parse flat `key: value` lines, starting from a uniform profile and
    /// overriding the listed dimensions. Accepts `:` or `=` separators so the
    /// same file parses as YAML or flat TOML.
    pub fn from_yaml_str(raw: &str) -> Result<Self, RlvrError> {
        let mut weights = Self::uniform();
        for (line_no, line) in raw.lines().enumerate() {
            let cleaned = line.split('#').next().unwrap_or("").trim();
            if cleaned.is_empty() {
                continue;
            }
            let (key, value) = split_kv(cleaned, line_no)?;
            weights.set_dimension(key, parse_weight_value(value, line_no)?)?;
        }
        weights.validate()?;
        Ok(weights)
    }
}

/// One [`RewardWeights`] profile per training target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RewardWeightProfiles {
    pub router: RewardWeights,
    pub assistant: RewardWeights,
    pub critic: RewardWeights,
    pub compressor: RewardWeights,
    pub tool_use: RewardWeights,
}

impl Default for RewardWeightProfiles {
    fn default() -> Self {
        Self::defaults()
    }
}

impl RewardWeightProfiles {
    /// PRD-aligned default profiles for every training target.
    pub fn defaults() -> Self {
        Self {
            router: RewardWeights::router(),
            assistant: RewardWeights::assistant(),
            critic: RewardWeights::critic(),
            compressor: RewardWeights::compressor(),
            tool_use: RewardWeights::tool_use(),
        }
    }

    pub fn get(&self, target: TrainingTarget) -> &RewardWeights {
        match target {
            TrainingTarget::Router => &self.router,
            TrainingTarget::Assistant => &self.assistant,
            TrainingTarget::Critic => &self.critic,
            TrainingTarget::Compressor => &self.compressor,
            TrainingTarget::ToolUse => &self.tool_use,
        }
    }

    pub fn validate(&self) -> Result<(), RlvrError> {
        self.router.validate()?;
        self.assistant.validate()?;
        self.critic.validate()?;
        self.compressor.validate()?;
        self.tool_use.validate()
    }

    /// Serialize every profile as flat namespaced YAML, e.g.
    /// `router.route_correctness: 3.0`.
    pub fn to_yaml(&self) -> String {
        let targets = [
            (TrainingTarget::Router, &self.router),
            (TrainingTarget::Assistant, &self.assistant),
            (TrainingTarget::Critic, &self.critic),
            (TrainingTarget::Compressor, &self.compressor),
            (TrainingTarget::ToolUse, &self.tool_use),
        ];
        let mut out = String::new();
        for (target, weights) in targets {
            for (name, value) in weights.dimensions() {
                out.push_str(&format!("{}.{}: {}\n", target.as_str(), name, value));
            }
        }
        out
    }

    /// Parse flat namespaced `target.dimension: value` lines, starting from the
    /// default profiles and overriding only the listed weights. Accepts `:` or
    /// `=` separators (YAML or flat TOML).
    pub fn from_yaml_str(raw: &str) -> Result<Self, RlvrError> {
        let mut profiles = Self::defaults();
        for (line_no, line) in raw.lines().enumerate() {
            let cleaned = line.split('#').next().unwrap_or("").trim();
            if cleaned.is_empty() {
                continue;
            }
            let (key, value) = split_kv(cleaned, line_no)?;
            let (target_name, dimension) = key.split_once('.').ok_or_else(|| {
                RlvrError::Config(format!(
                    "line {}: expected target.dimension, got {:?}",
                    line_no + 1,
                    key
                ))
            })?;
            let target = TrainingTarget::parse(target_name).ok_or_else(|| {
                RlvrError::Config(format!(
                    "line {}: unknown training target {:?}",
                    line_no + 1,
                    target_name
                ))
            })?;
            let weights = profiles.get_mut(target);
            weights.set_dimension(dimension, parse_weight_value(value, line_no)?)?;
        }
        profiles.validate()?;
        Ok(profiles)
    }

    fn get_mut(&mut self, target: TrainingTarget) -> &mut RewardWeights {
        match target {
            TrainingTarget::Router => &mut self.router,
            TrainingTarget::Assistant => &mut self.assistant,
            TrainingTarget::Critic => &mut self.critic,
            TrainingTarget::Compressor => &mut self.compressor,
            TrainingTarget::ToolUse => &mut self.tool_use,
        }
    }
}

/// Recompute an artifact's `final_reward` under `weights`, leaving the
/// per-dimension vector untouched. This is how reward config changes the final
/// reward without code edits.
pub fn apply_reward_weights(
    artifact: &RewardVectorArtifact,
    weights: &RewardWeights,
) -> Result<RewardVectorArtifact, RlvrError> {
    artifact.validate()?;
    weights.validate()?;
    let final_reward = weights.weighted_reward(&artifact.reward_vector);
    let weighted = RewardVectorArtifact {
        reward_vector: artifact.reward_vector.clone(),
        final_reward,
    };
    weighted.validate()?;
    Ok(weighted)
}

/// Access [`RewardVector`] dimensions as `(name, value)` pairs in a fixed order.
trait RewardVectorDimensions {
    fn dimensions(&self) -> [f64; 10];
}

impl RewardVectorDimensions for RewardVector {
    fn dimensions(&self) -> [f64; 10] {
        [
            self.correctness,
            self.checkpoint_coverage,
            self.clarification_quality,
            self.false_premise_detection,
            self.route_correctness,
            self.tool_use_correctness,
            self.cost_efficiency,
            self.latency_efficiency,
            self.privacy_compliance,
            self.non_redundancy,
        ]
    }
}

fn split_kv(cleaned: &str, line_no: usize) -> Result<(&str, &str), RlvrError> {
    let separator = cleaned
        .find(|c| c == ':' || c == '=')
        .ok_or_else(|| RlvrError::Config(format!("line {} is not key: value", line_no + 1)))?;
    Ok((cleaned[..separator].trim(), cleaned[separator + 1..].trim()))
}

fn parse_weight_value(value: &str, line_no: usize) -> Result<f64, RlvrError> {
    value.trim_matches('"').parse::<f64>().map_err(|_| {
        RlvrError::Config(format!(
            "line {}: weight value is not a number",
            line_no + 1
        ))
    })
}

fn require_finite_non_negative(name: &str, value: f64) -> Result<(), RlvrError> {
    if !value.is_finite() {
        return Err(RlvrError::Config(format!("{name} must be finite")));
    }
    if value < 0.0 {
        return Err(RlvrError::Config(format!("{name} cannot be negative")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skewed_vector() -> RewardVector {
        // Route/tool/cost/latency/privacy perfect; answer-quality dimensions low.
        RewardVector {
            correctness: 0.0,
            checkpoint_coverage: 0.0,
            clarification_quality: 0.0,
            false_premise_detection: 0.0,
            route_correctness: 1.0,
            tool_use_correctness: 1.0,
            cost_efficiency: 1.0,
            latency_efficiency: 1.0,
            privacy_compliance: 1.0,
            non_redundancy: 1.0,
        }
    }

    #[test]
    fn weighted_reward_changes_with_training_target() {
        // Done-when: changing the reward config changes the final reward.
        let vector = skewed_vector();
        let router = RewardWeights::router().weighted_reward(&vector);
        let assistant = RewardWeights::assistant().weighted_reward(&vector);

        // Router weights emphasise the perfect dimensions -> higher reward.
        assert!(router > assistant);
        assert!(router > 0.5);
        assert!(assistant < 0.5);
    }

    #[test]
    fn uniform_weights_match_unweighted_average() {
        let vector = skewed_vector();
        let uniform = RewardWeights::uniform().weighted_reward(&vector);
        // Unweighted average of the ten dimensions is 0.6.
        assert!((uniform - 0.6).abs() < f64::EPSILON);
    }

    #[test]
    fn apply_reward_weights_recomputes_final_reward_without_touching_vector() {
        let artifact = RewardVectorArtifact {
            reward_vector: skewed_vector(),
            // Original (unweighted average) final reward.
            final_reward: 0.6,
        };
        let weighted = apply_reward_weights(&artifact, &RewardWeights::router()).unwrap();
        assert_eq!(weighted.reward_vector, artifact.reward_vector);
        assert_ne!(weighted.final_reward, artifact.final_reward);
        assert!(
            (weighted.final_reward
                - RewardWeights::router().weighted_reward(&artifact.reward_vector))
            .abs()
                < f64::EPSILON
        );
    }

    #[test]
    fn loading_weights_from_yaml_changes_final_reward_without_code_edits() {
        let vector = skewed_vector();
        let default_router = RewardWeights::router().weighted_reward(&vector);

        // Heavily emphasise the (zero) answer-correctness dimension via config.
        let yaml = "correctness: 10.0\nroute_correctness: 1.0\n";
        let configured = RewardWeights::from_yaml_str(yaml).unwrap();
        let configured_reward = configured.weighted_reward(&vector);

        assert_ne!(default_router, configured_reward);
        assert!(configured_reward < default_router);
    }

    #[test]
    fn reward_weights_yaml_round_trip_preserves_profile() {
        let weights = RewardWeights::router();
        let parsed = RewardWeights::from_yaml_str(&weights.to_yaml()).unwrap();
        // Only the relative weights matter; compare via weighted_reward on a sample.
        let vector = skewed_vector();
        assert!((parsed.weighted_reward(&vector) - weights.weighted_reward(&vector)).abs() < 1e-9);
    }

    #[test]
    fn reward_weights_parse_accepts_toml_style_equals_separator() {
        let yaml =
            RewardWeights::from_yaml_str("route_correctness: 3.0\ncost_efficiency: 2.0").unwrap();
        let toml =
            RewardWeights::from_yaml_str("route_correctness = 3.0\ncost_efficiency = 2.0").unwrap();
        assert_eq!(yaml, toml);
    }

    #[test]
    fn reward_weights_validation_rejects_negative_and_all_zero() {
        let mut negative = RewardWeights::router();
        negative.cost_efficiency = -1.0;
        assert!(negative.validate().is_err());

        let zero = RewardWeights {
            correctness: 0.0,
            checkpoint_coverage: 0.0,
            clarification_quality: 0.0,
            false_premise_detection: 0.0,
            route_correctness: 0.0,
            tool_use_correctness: 0.0,
            cost_efficiency: 0.0,
            latency_efficiency: 0.0,
            privacy_compliance: 0.0,
            non_redundancy: 0.0,
        };
        assert!(zero.validate().is_err());
    }

    #[test]
    fn reward_weights_from_yaml_rejects_unknown_dimension() {
        let err = RewardWeights::from_yaml_str("made_up_dimension: 1.0").unwrap_err();
        assert!(err.to_string().contains("unknown reward weight dimension"));
    }

    #[test]
    fn profiles_default_cover_all_five_targets() {
        let profiles = RewardWeightProfiles::defaults();
        profiles.validate().unwrap();
        for target in TrainingTarget::ALL {
            assert_eq!(*profiles.get(target), RewardWeights::for_target(target));
        }
    }

    #[test]
    fn profiles_override_via_namespaced_yaml() {
        let vector = skewed_vector();
        let defaults = RewardWeightProfiles::defaults();
        let default_critic = defaults
            .get(TrainingTarget::Critic)
            .weighted_reward(&vector);

        // Override the critic profile to emphasise correctness (a zero dimension here).
        let yaml = "critic.correctness: 9.0\ncritic.false_premise_detection: 1.0";
        let configured = RewardWeightProfiles::from_yaml_str(yaml).unwrap();
        let configured_critic = configured
            .get(TrainingTarget::Critic)
            .weighted_reward(&vector);

        assert_ne!(default_critic, configured_critic);
        assert!(configured_critic < default_critic);
        // Router profile is untouched by the critic-only override.
        assert_eq!(
            configured.get(TrainingTarget::Router),
            defaults.get(TrainingTarget::Router)
        );
    }

    #[test]
    fn profiles_yaml_round_trip_preserves_all_targets() {
        let profiles = RewardWeightProfiles::defaults();
        let parsed = RewardWeightProfiles::from_yaml_str(&profiles.to_yaml()).unwrap();
        assert_eq!(parsed, profiles);
    }
}
