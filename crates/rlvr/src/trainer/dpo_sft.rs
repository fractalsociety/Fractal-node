//! RLVR-032: fallback DPO / SFT training path.
//!
//! When GRPO is too heavy for a small machine, rollouts can instead be converted
//! into lightweight supervised datasets:
//!
//! - **DPO** — pair a high-reward response against a low-reward response for the
//!   same prompt into a [`PreferencePair`] (chosen vs rejected).
//! - **SFT** — keep high-quality rollouts (reward ≥ threshold) as direct
//!   prompt→response [`SftExample`]s.
//!
//! Inputs are [`ScoredRollout`]s (a prompt + response + final reward), which the
//! rollout runner (RLVR-030) emits and the reward engine (RLVR-024) scores. Both
//! datasets are deterministic, CPU-only, and local-only — they "work on small
//! machines" with no GPU. Raw prompts/responses stay local training data and are
//! never committed on-chain.

use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{stable_hash, RlvrError};

use super::{
    validate_training_resources, TrainingResourceGuardInput, TrainingResourceLimits,
    TrainingResourceReport,
};

/// Which fallback training path to build a dataset for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FallbackTrainMode {
    Dpo,
    Sft,
}

impl FallbackTrainMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dpo => "dpo",
            Self::Sft => "sft",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().trim_matches('"').to_ascii_lowercase().as_str() {
            "dpo" => Some(Self::Dpo),
            "sft" => Some(Self::Sft),
            _ => None,
        }
    }
}

/// One rollout plus its final reward — the unit consumed by both paths.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScoredRollout {
    pub task_id: String,
    pub prompt: String,
    pub response: String,
    /// Final reward from the reward engine, in `[0, 1]`.
    pub reward: f64,
    /// Optional link back to the dialogue trace.
    pub trace_id: Option<String>,
}

impl ScoredRollout {
    pub fn validate(&self) -> Result<(), RlvrError> {
        require_non_empty("scored_rollout.task_id", &self.task_id)?;
        require_non_empty("scored_rollout.prompt", &self.prompt)?;
        require_non_empty("scored_rollout.response", &self.response)?;
        require_reward("scored_rollout.reward", self.reward)?;
        Ok(())
    }
}

/// A chosen/rejected response pair for the same prompt (DPO).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreferencePair {
    pub task_id: String,
    pub prompt: String,
    pub chosen: String,
    pub rejected: String,
    pub chosen_reward: f64,
    pub rejected_reward: f64,
    /// `chosen_reward - rejected_reward` (always ≥ `min_reward_margin`).
    pub reward_margin: f64,
}

/// A high-quality prompt→response example (SFT).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SftExample {
    pub task_id: String,
    pub prompt: String,
    pub response: String,
    pub reward: f64,
}

/// Knobs for dataset construction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FallbackTrainConfig {
    /// Minimum `chosen - rejected` gap required to emit a DPO pair.
    pub min_reward_margin: f64,
    /// Minimum reward for a rollout to enter the SFT dataset.
    pub sft_reward_threshold: f64,
    /// Cap on DPO pairs generated per prompt (best vs the N weakest others).
    pub max_pairs_per_prompt: usize,
}

impl Default for FallbackTrainConfig {
    fn default() -> Self {
        Self {
            min_reward_margin: 0.10,
            sft_reward_threshold: 0.70,
            max_pairs_per_prompt: 1,
        }
    }
}

impl FallbackTrainConfig {
    pub fn validate(&self) -> Result<(), RlvrError> {
        require_reward("fallback_train.min_reward_margin", self.min_reward_margin)?;
        require_reward(
            "fallback_train.sft_reward_threshold",
            self.sft_reward_threshold,
        )?;
        if self.max_pairs_per_prompt == 0 {
            return Err(RlvrError::Config(
                "fallback_train.max_pairs_per_prompt must be greater than zero".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DpoDataset {
    pub pairs: Vec<PreferencePair>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SftDataset {
    pub examples: Vec<SftExample>,
}

/// Either dataset, returned by [`build_fallback_dataset`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FallbackDataset {
    Dpo(DpoDataset),
    Sft(SftDataset),
}

impl FallbackDataset {
    pub fn len(&self) -> usize {
        match self {
            Self::Dpo(dataset) => dataset.pairs.len(),
            Self::Sft(dataset) => dataset.examples.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn dataset_hash(&self) -> Result<String, RlvrError> {
        stable_hash(self)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FallbackTrainReport {
    pub mode: FallbackTrainMode,
    pub input_rollouts: usize,
    pub produced: usize,
    pub skipped_low_margin: usize,
    pub skipped_below_threshold: usize,
    /// Training data is local-only; raw prompts/responses never leave the machine.
    pub local_only: bool,
    pub resource_report: Option<TrainingResourceReport>,
    pub dataset_hash: String,
}

/// Build DPO preference pairs: for each prompt, pair the highest-reward rollout
/// (chosen) against the weakest rollouts whose reward gap meets `min_reward_margin`.
pub fn build_dpo_dataset(
    rollouts: &[ScoredRollout],
    config: &FallbackTrainConfig,
) -> Result<(DpoDataset, FallbackTrainReport), RlvrError> {
    config.validate()?;
    for rollout in rollouts {
        rollout.validate()?;
    }

    // Group rollouts by prompt, preserving first-seen order for determinism.
    let mut groups: BTreeMap<&str, Vec<&ScoredRollout>> = BTreeMap::new();
    for rollout in rollouts {
        groups
            .entry(rollout.prompt.as_str())
            .or_default()
            .push(rollout);
    }

    let mut pairs = Vec::new();
    let mut skipped_low_margin = 0usize;
    for (_, mut group) in groups {
        if group.len() < 2 {
            continue;
        }
        // Deterministic ordering: reward desc, then response asc.
        group.sort_by(|left, right| {
            right
                .reward
                .partial_cmp(&left.reward)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.response.cmp(&right.response))
                .then_with(|| left.task_id.cmp(&right.task_id))
        });
        let chosen = group[0];
        // Pair chosen against the weakest rollouts (ascending reward) first.
        let mut emitted = 0usize;
        for rejected in group.iter().skip(1).rev() {
            let margin = chosen.reward - rejected.reward;
            if emitted < config.max_pairs_per_prompt && margin >= config.min_reward_margin {
                pairs.push(PreferencePair {
                    task_id: chosen.task_id.clone(),
                    prompt: chosen.prompt.clone(),
                    chosen: chosen.response.clone(),
                    rejected: rejected.response.clone(),
                    chosen_reward: chosen.reward,
                    rejected_reward: rejected.reward,
                    reward_margin: margin,
                });
                emitted += 1;
            } else {
                // Low-margin candidate, or a valid candidate beyond the per-prompt cap.
                skipped_low_margin += 1;
            }
        }
    }

    let dataset = DpoDataset { pairs };
    let report = finalize_report(
        FallbackTrainMode::Dpo,
        rollouts.len(),
        dataset.pairs.len(),
        skipped_low_margin,
        0,
        &FallbackDataset::Dpo(dataset.clone()),
    )?;
    Ok((dataset, report))
}

/// Build SFT examples from high-quality rollouts (reward ≥ threshold).
pub fn build_sft_dataset(
    rollouts: &[ScoredRollout],
    config: &FallbackTrainConfig,
) -> Result<(SftDataset, FallbackTrainReport), RlvrError> {
    config.validate()?;
    for rollout in rollouts {
        rollout.validate()?;
    }

    let mut examples = Vec::new();
    let mut skipped_below_threshold = 0usize;
    // Deterministic order: by reward desc, then task_id.
    let mut ordered: Vec<&ScoredRollout> = rollouts.iter().collect();
    ordered.sort_by(|left, right| {
        right
            .reward
            .partial_cmp(&left.reward)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.task_id.cmp(&right.task_id))
    });
    for rollout in ordered {
        if rollout.reward >= config.sft_reward_threshold {
            examples.push(SftExample {
                task_id: rollout.task_id.clone(),
                prompt: rollout.prompt.clone(),
                response: rollout.response.clone(),
                reward: rollout.reward,
            });
        } else {
            skipped_below_threshold += 1;
        }
    }

    let dataset = SftDataset { examples };
    let report = finalize_report(
        FallbackTrainMode::Sft,
        rollouts.len(),
        dataset.examples.len(),
        0,
        skipped_below_threshold,
        &FallbackDataset::Sft(dataset.clone()),
    )?;
    Ok((dataset, report))
}

/// Dispatch to the DPO or SFT builder based on `mode`.
pub fn build_fallback_dataset(
    mode: FallbackTrainMode,
    rollouts: &[ScoredRollout],
    config: &FallbackTrainConfig,
) -> Result<(FallbackDataset, FallbackTrainReport), RlvrError> {
    match mode {
        FallbackTrainMode::Dpo => {
            let (dataset, report) = build_dpo_dataset(rollouts, config)?;
            Ok((FallbackDataset::Dpo(dataset), report))
        }
        FallbackTrainMode::Sft => {
            let (dataset, report) = build_sft_dataset(rollouts, config)?;
            Ok((FallbackDataset::Sft(dataset), report))
        }
    }
}

fn finalize_report(
    mode: FallbackTrainMode,
    input_rollouts: usize,
    produced: usize,
    skipped_low_margin: usize,
    skipped_below_threshold: usize,
    dataset: &FallbackDataset,
) -> Result<FallbackTrainReport, RlvrError> {
    Ok(FallbackTrainReport {
        mode,
        input_rollouts,
        produced,
        skipped_low_margin,
        skipped_below_threshold,
        local_only: true,
        resource_report: None,
        dataset_hash: dataset.dataset_hash()?,
    })
}

// ---------------------------------------------------------------------------
// JSONL I/O for the CLI path.
// ---------------------------------------------------------------------------

/// Read [`ScoredRollout`]s from a JSONL file (one rollout per line).
pub fn read_scored_rollouts_jsonl(path: impl AsRef<Path>) -> Result<Vec<ScoredRollout>, RlvrError> {
    let path = path.as_ref();
    let file = fs::File::open(path)?;
    let reader = BufReader::new(file);
    let mut rollouts = Vec::new();
    for (idx, line) in reader.lines().enumerate() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let rollout: ScoredRollout = serde_json::from_str(trimmed).map_err(|err| {
            RlvrError::Config(format!(
                "rollouts line {} is not a valid ScoredRollout: {err}",
                idx + 1
            ))
        })?;
        rollout.validate()?;
        rollouts.push(rollout);
    }
    Ok(rollouts)
}

/// Write DPO preference pairs to a JSONL file (`out_dir/dpo_pairs.jsonl`).
pub fn write_dpo_dataset_jsonl(
    dataset: &DpoDataset,
    out_dir: impl AsRef<Path>,
) -> Result<PathBuf, RlvrError> {
    write_jsonl(&dataset.pairs, out_dir.as_ref(), "dpo_pairs.jsonl")
}

/// Write SFT examples to a JSONL file (`out_dir/sft_examples.jsonl`).
pub fn write_sft_dataset_jsonl(
    dataset: &SftDataset,
    out_dir: impl AsRef<Path>,
) -> Result<PathBuf, RlvrError> {
    write_jsonl(&dataset.examples, out_dir.as_ref(), "sft_examples.jsonl")
}

fn write_jsonl<T: Serialize>(
    rows: &[T],
    out_dir: &Path,
    file_name: &str,
) -> Result<PathBuf, RlvrError> {
    fs::create_dir_all(out_dir)?;
    let path = out_dir.join(file_name);
    let mut file = fs::File::create(&path)?;
    for row in rows {
        let mut line = serde_json::to_string(row)?;
        line.push('\n');
        file.write_all(line.as_bytes())?;
    }
    Ok(path)
}

/// CLI entrypoint for `fractal-rlvr train --mode dpo|sft --rollouts <path> --out <dir>`.
///
/// Reads scored rollouts, builds the requested dataset, writes it to `--out`,
/// and returns a one-line summary. This is the small-machine-friendly fallback
/// path (CPU-only, no GPU).
pub fn run_fallback_train_cli(argv: &[String]) -> Result<String, RlvrError> {
    let mode_raw = value_after(argv, "--mode").ok_or_else(|| {
        RlvrError::UnsupportedCommand(
            "train --mode requires `dpo` or `sft` for the fallback path".into(),
        )
    })?;
    let mode = FallbackTrainMode::parse(&mode_raw).ok_or_else(|| {
        RlvrError::UnsupportedCommand(format!(
            "train --mode {mode_raw:?} is not supported (expected `dpo` or `sft`)"
        ))
    })?;
    let rollouts_path = value_after(argv, "--rollouts").ok_or_else(|| {
        RlvrError::UnsupportedCommand("train --mode requires --rollouts <jsonl>".into())
    })?;
    let out_dir = value_after(argv, "--out")
        .ok_or_else(|| RlvrError::UnsupportedCommand("train --mode requires --out <dir>".into()))?;

    let mut config = FallbackTrainConfig::default();
    if let Some(raw) = value_after(argv, "--min-margin") {
        config.min_reward_margin = parse_f64(&raw, "--min-margin")?;
    }
    if let Some(raw) = value_after(argv, "--sft-threshold") {
        config.sft_reward_threshold = parse_f64(&raw, "--sft-threshold")?;
    }
    config.validate()?;

    let rollouts = read_scored_rollouts_jsonl(&rollouts_path)?;
    let resource_report = validate_training_resources(TrainingResourceGuardInput {
        requested_batch_size: rollouts.len(),
        limits: TrainingResourceLimits::from_env_or_default()?,
        snapshot: None,
    })?;
    let (dataset, mut report) = build_fallback_dataset(mode, &rollouts, &config)?;
    report.resource_report = Some(resource_report);
    let written = match &dataset {
        FallbackDataset::Dpo(d) => write_dpo_dataset_jsonl(d, &out_dir)?,
        FallbackDataset::Sft(d) => write_sft_dataset_jsonl(d, &out_dir)?,
    };

    Ok(format!(
        "train --mode {}: input={} produced={} skipped_low_margin={} skipped_below_threshold={} local_only={} dataset_hash={} written={}",
        mode.as_str(),
        report.input_rollouts,
        report.produced,
        report.skipped_low_margin,
        report.skipped_below_threshold,
        report.local_only,
        report.dataset_hash,
        written.display()
    ))
}

fn value_after(argv: &[String], flag: &str) -> Option<String> {
    argv.windows(2)
        .find(|pair| pair[0] == flag)
        .map(|pair| pair[1].clone())
}

fn parse_f64(raw: &str, flag: &str) -> Result<f64, RlvrError> {
    raw.trim_matches('"')
        .parse::<f64>()
        .map_err(|_| RlvrError::Config(format!("{flag} must be a number")))
}

fn require_non_empty(name: &str, value: &str) -> Result<(), RlvrError> {
    if value.trim().is_empty() {
        return Err(RlvrError::Config(format!("{name} cannot be empty")));
    }
    Ok(())
}

fn require_reward(name: &str, value: f64) -> Result<(), RlvrError> {
    if !value.is_finite() {
        return Err(RlvrError::Config(format!("{name} must be finite")));
    }
    if !(0.0..=1.0).contains(&value) {
        return Err(RlvrError::Config(format!("{name} must be in [0, 1]")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rollout(task_id: &str, prompt: &str, response: &str, reward: f64) -> ScoredRollout {
        ScoredRollout {
            task_id: task_id.into(),
            prompt: prompt.into(),
            response: response.into(),
            reward,
            trace_id: Some(format!("trace-{task_id}")),
        }
    }

    #[test]
    fn dpo_pairs_high_reward_chosen_against_low_reward_rejected() {
        let rollouts = vec![
            rollout("good", "What is 2+2?", "4", 0.95),
            rollout("bad", "What is 2+2?", "I think maybe 5?", 0.10),
        ];
        let (dataset, report) =
            build_dpo_dataset(&rollouts, &FallbackTrainConfig::default()).unwrap();

        assert_eq!(dataset.pairs.len(), 1);
        let pair = &dataset.pairs[0];
        assert_eq!(pair.chosen, "4");
        assert_eq!(pair.rejected, "I think maybe 5?");
        assert!((pair.chosen_reward - 0.95).abs() < f64::EPSILON);
        assert!((pair.rejected_reward - 0.10).abs() < f64::EPSILON);
        assert!(pair.reward_margin >= 0.10);
        assert_eq!(report.produced, 1);
        assert_eq!(report.mode, FallbackTrainMode::Dpo);
        assert_eq!(report.dataset_hash.len(), 64);
    }

    #[test]
    fn dpo_skips_pairs_below_min_reward_margin() {
        let rollouts = vec![
            rollout("a", "prompt", "resp-a", 0.50),
            rollout("b", "prompt", "resp-b", 0.48),
        ];
        let (dataset, report) =
            build_dpo_dataset(&rollouts, &FallbackTrainConfig::default()).unwrap();
        // 0.02 gap < 0.10 default margin -> no pair.
        assert!(dataset.pairs.is_empty());
        assert_eq!(report.skipped_low_margin, 1);
    }

    #[test]
    fn dpo_requires_two_rollouts_per_prompt() {
        let rollouts = vec![rollout("solo", "only prompt", "only response", 0.9)];
        let (dataset, _report) =
            build_dpo_dataset(&rollouts, &FallbackTrainConfig::default()).unwrap();
        assert!(dataset.pairs.is_empty());
    }

    #[test]
    fn sft_keeps_only_high_reward_rollouts() {
        let rollouts = vec![
            rollout("good", "p1", "r1", 0.95),
            rollout("ok", "p2", "r2", 0.72),
            rollout("weak", "p3", "r3", 0.30),
        ];
        let (dataset, report) =
            build_sft_dataset(&rollouts, &FallbackTrainConfig::default()).unwrap();
        assert_eq!(dataset.examples.len(), 2);
        assert!(dataset.examples.iter().all(|ex| ex.reward >= 0.70));
        assert_eq!(report.skipped_below_threshold, 1);
        assert_eq!(report.mode, FallbackTrainMode::Sft);
    }

    #[test]
    fn fallback_dataset_dispatches_by_mode() {
        let rollouts = vec![
            rollout("a", "p", "good", 0.9),
            rollout("b", "p", "bad", 0.1),
        ];
        let (dpo, _) = build_fallback_dataset(
            FallbackTrainMode::Dpo,
            &rollouts,
            &FallbackTrainConfig::default(),
        )
        .unwrap();
        assert_eq!(dpo.len(), 1);
        assert!(matches!(dpo, FallbackDataset::Dpo(_)));

        let (sft, _) = build_fallback_dataset(
            FallbackTrainMode::Sft,
            &rollouts,
            &FallbackTrainConfig::default(),
        )
        .unwrap();
        assert!(matches!(sft, FallbackDataset::Sft(_)));
        assert_eq!(sft.len(), 1); // only the 0.9 rollout passes the 0.70 threshold
    }

    #[test]
    fn dataset_construction_is_deterministic() {
        let rollouts = vec![
            rollout("a", "p", "good", 0.9),
            rollout("b", "p", "bad", 0.1),
            rollout("c", "p2", "fine", 0.8),
        ];
        let (d1, r1) = build_dpo_dataset(&rollouts, &FallbackTrainConfig::default()).unwrap();
        let (d2, r2) = build_dpo_dataset(&rollouts, &FallbackTrainConfig::default()).unwrap();
        assert_eq!(d1, d2);
        assert_eq!(r1.dataset_hash, r2.dataset_hash);

        // Reordered input still yields the same dataset (groups by prompt, sorts).
        let mut shuffled = rollouts.clone();
        shuffled.reverse();
        let (d3, _) = build_dpo_dataset(&shuffled, &FallbackTrainConfig::default()).unwrap();
        assert_eq!(d1, d3);
    }

    #[test]
    fn scored_rollout_rejects_out_of_range_reward() {
        let mut bad = rollout("a", "p", "r", 0.5);
        bad.reward = 1.5;
        assert!(bad.validate().is_err());
        bad.reward = f64::NAN;
        assert!(bad.validate().is_err());
    }

    #[test]
    fn jsonl_round_trip_preserves_rollouts_and_datasets() {
        let dir = std::env::temp_dir().join(format!("fractal-rlvr-dposft-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let rollouts = vec![
            rollout("a", "p", "good", 0.9),
            rollout("b", "p", "bad", 0.1),
        ];
        let rollouts_path = dir.join("rollouts.jsonl");
        let mut content = String::new();
        for r in &rollouts {
            content.push_str(&serde_json::to_string(r).unwrap());
            content.push('\n');
        }
        fs::write(&rollouts_path, content).unwrap();

        let read_back = read_scored_rollouts_jsonl(&rollouts_path).unwrap();
        assert_eq!(read_back, rollouts);

        let (dpo, _) = build_dpo_dataset(&read_back, &FallbackTrainConfig::default()).unwrap();
        let written = write_dpo_dataset_jsonl(&dpo, &dir).unwrap();
        let parsed: Vec<PreferencePair> = read_jsonl(&written);
        assert_eq!(parsed, dpo.pairs);

        let _ = fs::remove_dir_all(&dir);
    }

    fn read_jsonl<T: for<'de> Deserialize<'de>>(path: &Path) -> Vec<T> {
        let reader = BufReader::new(fs::File::open(path).unwrap());
        reader
            .lines()
            .map(|line| line.unwrap())
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(&line).unwrap())
            .collect()
    }

    #[test]
    fn train_cli_builds_and_writes_dpo_dataset() {
        let dir =
            std::env::temp_dir().join(format!("fractal-rlvr-dposft-cli-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let rollouts_path = dir.join("rollouts.jsonl");
        let mut content = String::new();
        for r in [
            rollout("a", "p", "good", 0.9),
            rollout("b", "p", "bad", 0.1),
        ] {
            content.push_str(&serde_json::to_string(&r).unwrap());
            content.push('\n');
        }
        fs::write(&rollouts_path, content).unwrap();

        let out_dir = dir.join("out");
        let argv = vec![
            "fractal-rlvr".to_string(),
            "train".into(),
            "--mode".into(),
            "dpo".into(),
            "--rollouts".into(),
            rollouts_path.display().to_string(),
            "--out".into(),
            out_dir.display().to_string(),
        ];
        let summary = run_fallback_train_cli(&argv).unwrap();
        assert!(summary.contains("train --mode dpo"));
        assert!(summary.contains("produced=1"));
        assert!(out_dir.join("dpo_pairs.jsonl").exists());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn train_cli_rejects_unknown_mode_and_missing_args() {
        let argv = vec![
            "fractal-rlvr".to_string(),
            "train".into(),
            "--mode".into(),
            "grpo".into(),
        ];
        assert!(run_fallback_train_cli(&argv).is_err());

        let argv = vec![
            "fractal-rlvr".to_string(),
            "train".into(),
            "--mode".into(),
            "dpo".into(),
        ];
        assert!(run_fallback_train_cli(&argv).is_err()); // missing --rollouts/--out
    }

    #[test]
    fn dataset_hash_is_field_sensitive() {
        let rollouts = vec![
            rollout("a", "p", "good", 0.9),
            rollout("b", "p", "bad", 0.1),
        ];
        let (d1, _) = build_dpo_dataset(&rollouts, &FallbackTrainConfig::default()).unwrap();
        let mut changed = rollouts.clone();
        changed[0].reward = 0.8; // narrows the margin but still >= 0.1 -> different rewards in pair
        let (d2, _) = build_dpo_dataset(&changed, &FallbackTrainConfig::default()).unwrap();
        assert_ne!(
            FallbackDataset::Dpo(d1).dataset_hash().unwrap(),
            FallbackDataset::Dpo(d2).dataset_hash().unwrap()
        );
    }

    #[test]
    fn raw_training_data_stays_local_and_unhashed_prompt_not_in_report() {
        // The report carries only counts + a dataset hash; raw prompts/responses
        // are never embedded in the report (which could be logged/shared).
        let rollouts = vec![
            rollout("a", "SECRET-PROMPT", "SECRET-RESPONSE-GOOD", 0.9),
            rollout("b", "SECRET-PROMPT", "SECRET-RESPONSE-BAD", 0.1),
        ];
        let (_dataset, report) =
            build_dpo_dataset(&rollouts, &FallbackTrainConfig::default()).unwrap();
        let report_json = serde_json::to_string(&report).unwrap();
        assert!(!report_json.contains("SECRET-PROMPT"));
        assert!(!report_json.contains("SECRET-RESPONSE"));
        assert!(report.local_only);
        // hash is a blake3 hex.
        assert_eq!(report.dataset_hash.len(), 64);
    }
}
