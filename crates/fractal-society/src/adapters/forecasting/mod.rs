//! Forecasting domain adapter (AR-09).
//!
//! A non-trading domain proving the generic kernel runs a second, unrelated
//! research domain end-to-end. See [`adapter`] for the environment and agent,
//! and [`scorecard`] for scorecard construction.

pub mod adapter;
pub mod scorecard;
pub mod types;

pub use adapter::{
    ForecastDataset, ForecastEnvironment, ForecastEpisode, ForecastSample, ForecastingAdapter,
    ForecastingAgent, FORECASTING_ADAPTER_ID, FORECASTING_ADAPTER_VERSION, FORECASTING_AGENT_ID,
};
pub use scorecard::build_forecasting_scorecard;
pub use types::{ForecastingAction, ForecastingObservation, ForecastingOutcome};
