//! Forecasting domain types (AR-09).
//!
//! A non-trading domain: each step the agent observes a feature and predicts the
//! probability of a binary outcome; it is scored by the Brier score (mean
//! squared error of the probability forecast). This proves the generic kernel
//! runs a second, entirely different domain end-to-end.

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::simulation::{Action, Observation, Outcome};

/// Observation: the feature for the current step plus the last realized outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForecastingObservation {
    /// Step index within the episode.
    pub step: u64,
    /// Real-valued feature the agent forecasts from.
    pub feature: f64,
    /// The outcome realized on the previous step (0.0 or 1.0).
    pub last_outcome: f64,
}

impl Observation for ForecastingObservation {
    fn to_json(&self) -> Result<serde_json::Value> {
        Ok(serde_json::to_value(self)?)
    }
}

/// Action: a predicted probability in `[0, 1]` that the outcome is 1.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForecastingAction {
    /// Predicted probability of a positive (1) outcome.
    pub probability: f64,
}

impl Action for ForecastingAction {
    fn validate(&self) -> Result<()> {
        if !(0.0..=1.0).contains(&self.probability) {
            return Err(crate::error::Error::InvalidAction(format!(
                "probability must be in [0, 1], got {}",
                self.probability
            )));
        }
        Ok(())
    }
    fn to_json(&self) -> Result<serde_json::Value> {
        Ok(serde_json::to_value(self)?)
    }
}

/// Outcome of one forecast: the realized binary value, the prediction, and the
/// per-step Brier score `(p - actual)^2`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForecastingOutcome {
    /// Step index.
    pub step: u64,
    /// Realized outcome (0.0 or 1.0).
    pub actual: f64,
    /// Predicted probability.
    pub predicted: f64,
    /// Brier score for this step.
    pub brier: f64,
    /// Whether the episode terminated after this step.
    pub terminal: bool,
}

impl Outcome for ForecastingOutcome {
    /// `1 - brier` so higher is better (Brier is a loss; the kernel maximizes
    /// primary score).
    fn primary_score(&self) -> f64 {
        1.0 - self.brier
    }
    fn is_terminal(&self) -> bool {
        self.terminal
    }
    fn to_json(&self) -> Result<serde_json::Value> {
        Ok(serde_json::to_value(self)?)
    }
}
