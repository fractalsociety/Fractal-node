//! RLVR-035: adapter export.
//!
//! Bundles everything the Fractal router / chat runtime needs to load a trained
//! adapter into a self-describing, hash-verified directory:
//!
//! - `adapter_weights.json` — LoRA-style weight tensors ([`AdapterWeights`]).
//! - `adapter_config.json` — rank, target modules, base model, local-only flag.
//! - `reward_policy.json` — the v0.1 reward policy plus its hash.
//! - `eval_report.json` — before/after reward evidence from the GRPO report.
//! - `model_card.json` — intended use, training summary, privacy statement.
//! - `manifest.json` — per-artifact blake3 hashes + the bundle `adapter_hash`.
//!
//! The manifest is the **load contract** ([`load_adapter_bundle`]): a consumer
//! reads it, re-hashes every artifact, recomputes `adapter_hash`, and only then
//! trusts the bundle. Artifacts are hash-only with respect to training data — raw
//! prompts, answers, and traces never appear in exported files (see the
//! `raw_training_data_stays_out_of_exported_artifacts` test).

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    hash_bytes, stable_hash, GrpoTrainerReport, MvpRewardPolicyV01, RlvrError,
    MVP_REWARD_POLICY_V01_ID,
};

use super::{register_adapter_metadata, AdapterMetadata, AdapterTrainingMode};

/// LoRA weight tensor format string emitted in [`AdapterWeights::format`].
pub const ADAPTER_WEIGHTS_FORMAT: &str = "fractal-rlvr-lora-v0.1";
/// Bundle manifest format string emitted in [`AdapterManifest::format_version`].
pub const ADAPTER_BUNDLE_FORMAT_VERSION: &str = "fractal-rlvr-adapter-bundle-v0.1";
/// Default LoRA rank used by [`synthesize_weights`] callers that omit `--rank`.
pub const DEFAULT_ADAPTER_RANK: u32 = 8;
/// Default base-model hidden dim used to size synthesized placeholder tensors.
pub const DEFAULT_MODEL_DIM: usize = 64;

/// Which side of a rank-decomposed LoRA update a tensor holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AdapterTensorKind {
    LoraA,
    LoraB,
}

impl AdapterTensorKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LoraA => "lora_a",
            Self::LoraB => "lora_b",
        }
    }
}

/// One flattened (row-major) LoRA tensor for a target module.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdapterTensor {
    pub module: String,
    pub kind: AdapterTensorKind,
    pub shape: Vec<usize>,
    pub values: Vec<f64>,
}

impl AdapterTensor {
    pub fn validate(&self) -> Result<(), RlvrError> {
        require_non_empty("adapter_tensor.module", &self.module)?;
        if self.shape.is_empty() {
            return Err(RlvrError::Config(
                "adapter_tensor.shape must contain at least one dimension".into(),
            ));
        }
        let expected = shape_product(&self.shape)?;
        if expected != self.values.len() {
            return Err(RlvrError::Config(format!(
                "adapter_tensor for module {:?} expected {} values from shape {:?} but got {}",
                self.module,
                expected,
                self.shape,
                self.values.len()
            )));
        }
        for (idx, value) in self.values.iter().enumerate() {
            if !value.is_finite() {
                return Err(RlvrError::Config(format!(
                    "adapter_tensor[{}].values[{idx}] must be finite",
                    self.module
                )));
            }
        }
        Ok(())
    }
}

/// LoRA-style adapter weights — the loadable weight artifact.
///
/// The harness trainer ([`crate::train_grpo_adapter`]) emits deterministic
/// advantage reports rather than real neural tensors, so callers may either
/// supply their own tensors or use [`synthesize_weights`] to produce a
/// deterministic placeholder bundle for the harness/CLI/tests.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdapterWeights {
    pub format: String,
    pub adapter_id: String,
    pub base_model_id: String,
    pub training_mode: AdapterTrainingMode,
    pub rank: u32,
    pub target_modules: Vec<String>,
    pub tensors: Vec<AdapterTensor>,
}

impl AdapterWeights {
    pub fn validate(&self) -> Result<(), RlvrError> {
        require_non_empty("adapter_weights.format", &self.format)?;
        require_non_empty("adapter_weights.adapter_id", &self.adapter_id)?;
        require_non_empty("adapter_weights.base_model_id", &self.base_model_id)?;
        if self.rank == 0 {
            return Err(RlvrError::Config(
                "adapter_weights.rank must be greater than zero".into(),
            ));
        }
        if self.target_modules.is_empty() {
            return Err(RlvrError::Config(
                "adapter_weights.target_modules must contain at least one module".into(),
            ));
        }
        let mut seen = Vec::new();
        for module in &self.target_modules {
            require_non_empty("adapter_weights.target_modules[i]", module)?;
            if seen.contains(module) {
                return Err(RlvrError::Config(format!(
                    "adapter_weights.target_modules contains duplicate {module:?}"
                )));
            }
            seen.push(module.clone());
        }
        for tensor in &self.tensors {
            tensor.validate()?;
            if !self.target_modules.contains(&tensor.module) {
                return Err(RlvrError::Config(format!(
                    "adapter tensor references unknown target module {:?}",
                    tensor.module
                )));
            }
        }
        Ok(())
    }

    pub fn stable_hash(&self) -> Result<String, RlvrError> {
        self.validate()?;
        stable_hash(self)
    }
}

/// Static adapter configuration consumed by the router/chat runtime at load time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdapterConfig {
    pub adapter_id: String,
    pub base_model_id: String,
    pub training_mode: AdapterTrainingMode,
    pub rank: u32,
    pub target_modules: Vec<String>,
    pub max_turns: u32,
    pub data_local_only: bool,
    pub base_model_hash: String,
    pub created_from_checkpoint: Option<String>,
}

impl AdapterConfig {
    pub fn validate(&self) -> Result<(), RlvrError> {
        require_non_empty("adapter_config.adapter_id", &self.adapter_id)?;
        require_non_empty("adapter_config.base_model_id", &self.base_model_id)?;
        if self.rank == 0 {
            return Err(RlvrError::Config(
                "adapter_config.rank must be greater than zero".into(),
            ));
        }
        if self.max_turns == 0 {
            return Err(RlvrError::Config(
                "adapter_config.max_turns must be greater than zero".into(),
            ));
        }
        if self.target_modules.is_empty() {
            return Err(RlvrError::Config(
                "adapter_config.target_modules must contain at least one module".into(),
            ));
        }
        validate_hex_hash("adapter_config.base_model_hash", &self.base_model_hash)?;
        Ok(())
    }

    pub fn stable_hash(&self) -> Result<String, RlvrError> {
        self.validate()?;
        stable_hash(self)
    }
}

/// The reward policy bundled with the export, plus its content hash.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RewardPolicyArtifact {
    pub policy_id: String,
    pub policy: MvpRewardPolicyV01,
    pub policy_hash: String,
}

impl RewardPolicyArtifact {
    /// The default `reward-v0.1` policy and its hash.
    pub fn default_v01() -> Result<Self, RlvrError> {
        let policy = MvpRewardPolicyV01::default();
        policy.validate()?;
        let policy_hash = stable_hash(&policy)?;
        Ok(Self {
            policy_id: MVP_REWARD_POLICY_V01_ID.into(),
            policy,
            policy_hash,
        })
    }

    pub fn validate(&self) -> Result<(), RlvrError> {
        require_non_empty("reward_policy_artifact.policy_id", &self.policy_id)?;
        self.policy.validate()?;
        validate_hex_hash("reward_policy_artifact.policy_hash", &self.policy_hash)?;
        if self.policy_hash != stable_hash(&self.policy)? {
            return Err(RlvrError::Config(
                "reward_policy_artifact.policy_hash does not match the serialized policy".into(),
            ));
        }
        Ok(())
    }
}

/// Before/after reward evidence sourced from the GRPO trainer report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdapterEvalReport {
    pub adapter_id: String,
    pub reward_version: String,
    pub before_avg_reward: f64,
    pub after_avg_reward: f64,
    pub improved: bool,
    pub rollout_count: usize,
    pub group_count: usize,
    /// `improved && after >= before` — a lightweight promotion hint. The full
    /// promotion gate is RLVR-038.
    pub promotion_recommendation: bool,
}

impl AdapterEvalReport {
    pub fn from_grpo(
        adapter_id: &str,
        reward_version: &str,
        report: &GrpoTrainerReport,
    ) -> Result<Self, RlvrError> {
        require_non_empty("adapter_eval_report.adapter_id", adapter_id)?;
        require_non_empty("adapter_eval_report.reward_version", reward_version)?;
        let eval = &report.eval;
        let improved = eval.improved || eval.after_avg_reward_estimate >= eval.before_avg_reward;
        Ok(Self {
            adapter_id: adapter_id.into(),
            reward_version: reward_version.into(),
            before_avg_reward: eval.before_avg_reward,
            after_avg_reward: eval.after_avg_reward_estimate,
            improved,
            rollout_count: report.rollout_count,
            group_count: report.group_count,
            promotion_recommendation: improved
                && eval.after_avg_reward_estimate >= eval.before_avg_reward,
        })
    }

    pub fn validate(&self) -> Result<(), RlvrError> {
        require_non_empty("adapter_eval_report.adapter_id", &self.adapter_id)?;
        require_non_empty("adapter_eval_report.reward_version", &self.reward_version)?;
        for (name, value) in [
            ("before_avg_reward", self.before_avg_reward),
            ("after_avg_reward", self.after_avg_reward),
        ] {
            if !value.is_finite() {
                return Err(RlvrError::Config(format!(
                    "adapter_eval_report.{name} must be finite"
                )));
            }
        }
        Ok(())
    }
}

/// Training summary embedded in the model card.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdapterTrainingSummary {
    pub training_mode: String,
    pub reward_version: String,
    pub rollout_count: usize,
    pub group_count: usize,
    pub format: String,
    pub rank: u32,
}

/// Privacy statement bundled with the model card. Values are pinned to the
/// global invariants (local-only, hash-only, no raw data committed).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterPrivacyStatement {
    pub data_local_only: bool,
    pub raw_data_committed: bool,
    pub hashes_only: bool,
    pub pii_tags: Vec<String>,
}

impl Default for AdapterPrivacyStatement {
    fn default() -> Self {
        Self {
            data_local_only: true,
            raw_data_committed: false,
            hashes_only: true,
            pii_tags: Vec::new(),
        }
    }
}

/// A standard model card describing the exported adapter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdapterModelCard {
    pub name: String,
    pub adapter_id: String,
    pub base_model_id: String,
    pub training_mode: String,
    pub reward_policy_id: String,
    pub format: String,
    pub intended_use: String,
    pub out_of_scope: Vec<String>,
    pub training_summary: AdapterTrainingSummary,
    pub eval: AdapterEvalReport,
    pub privacy: AdapterPrivacyStatement,
    pub license: String,
    pub version: String,
}

impl AdapterModelCard {
    pub fn validate(&self) -> Result<(), RlvrError> {
        require_non_empty("model_card.name", &self.name)?;
        require_non_empty("model_card.adapter_id", &self.adapter_id)?;
        require_non_empty("model_card.base_model_id", &self.base_model_id)?;
        require_non_empty("model_card.format", &self.format)?;
        require_non_empty("model_card.reward_policy_id", &self.reward_policy_id)?;
        self.eval.validate()?;
        if self.privacy.raw_data_committed {
            return Err(RlvrError::Config(
                "model_card.privacy.raw_data_committed must remain false".into(),
            ));
        }
        Ok(())
    }
}

/// Which artifact a manifest entry describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AdapterArtifactRole {
    Weights,
    Config,
    RewardPolicy,
    EvalReport,
    ModelCard,
}

impl AdapterArtifactRole {
    /// Canonical iteration order (also the sort key for `adapter_hash`).
    pub const ALL: [Self; 5] = [
        Self::Weights,
        Self::Config,
        Self::RewardPolicy,
        Self::EvalReport,
        Self::ModelCard,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Weights => "weights",
            Self::Config => "config",
            Self::RewardPolicy => "reward_policy",
            Self::EvalReport => "eval_report",
            Self::ModelCard => "model_card",
        }
    }

    pub const fn file_name(self) -> &'static str {
        match self {
            Self::Weights => "adapter_weights.json",
            Self::Config => "adapter_config.json",
            Self::RewardPolicy => "reward_policy.json",
            Self::EvalReport => "eval_report.json",
            Self::ModelCard => "model_card.json",
        }
    }
}

/// One per-artifact entry in the bundle manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterManifestFile {
    pub role: AdapterArtifactRole,
    pub file_name: String,
    pub hash: String,
    pub bytes: usize,
}

/// The bundle manifest — the load contract. Carries per-artifact hashes and the
/// overall `adapter_hash` (a content address over the five artifacts).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterManifest {
    pub adapter_id: String,
    pub base_model_id: String,
    pub format_version: String,
    pub timestamp_ms: u64,
    pub files: Vec<AdapterManifestFile>,
    pub adapter_hash: String,
    pub reward_policy_hash: String,
    pub route_policy_hash: Option<String>,
}

impl AdapterManifest {
    pub fn validate(&self) -> Result<(), RlvrError> {
        require_non_empty("manifest.adapter_id", &self.adapter_id)?;
        require_non_empty("manifest.base_model_id", &self.base_model_id)?;
        require_non_empty("manifest.format_version", &self.format_version)?;
        if self.timestamp_ms == 0 {
            return Err(RlvrError::Config(
                "manifest.timestamp_ms must be greater than zero".into(),
            ));
        }
        validate_hex_hash("manifest.adapter_hash", &self.adapter_hash)?;
        validate_hex_hash("manifest.reward_policy_hash", &self.reward_policy_hash)?;
        if let Some(hash) = &self.route_policy_hash {
            validate_hex_hash("manifest.route_policy_hash", hash)?;
        }
        if self.files.len() != AdapterArtifactRole::ALL.len() {
            return Err(RlvrError::Config(format!(
                "manifest must contain exactly {} file entries, found {}",
                AdapterArtifactRole::ALL.len(),
                self.files.len()
            )));
        }
        let mut roles = Vec::new();
        for entry in &self.files {
            validate_hex_hash("manifest.files[i].hash", &entry.hash)?;
            if entry.file_name != entry.role.file_name() {
                return Err(RlvrError::Config(format!(
                    "manifest file_name {:?} does not match role {:?}",
                    entry.file_name, entry.role
                )));
            }
            if roles.contains(&entry.role) {
                return Err(RlvrError::Config(format!(
                    "manifest contains duplicate role {:?}",
                    entry.role
                )));
            }
            roles.push(entry.role);
        }
        Ok(())
    }
}

#[derive(Serialize)]
struct AdapterHashFile<'a> {
    role: &'a str,
    file_name: &'a str,
    hash: &'a str,
}

#[derive(Serialize)]
struct AdapterHashInput<'a> {
    adapter_id: &'a str,
    base_model_id: &'a str,
    format_version: &'a str,
    files: Vec<AdapterHashFile<'a>>,
}

/// Compute the bundle `adapter_hash` from identity fields + per-artifact hashes,
/// sorted into canonical role order so the hash is stable regardless of entry
/// order in the manifest.
fn adapter_hash_from(
    adapter_id: &str,
    base_model_id: &str,
    format_version: &str,
    mut files: Vec<AdapterManifestFile>,
) -> Result<String, RlvrError> {
    files.sort_by_key(|entry| entry.role);
    let hash_files: Vec<AdapterHashFile> = files
        .iter()
        .map(|entry| AdapterHashFile {
            role: entry.role.as_str(),
            file_name: entry.file_name.as_str(),
            hash: entry.hash.as_str(),
        })
        .collect();
    stable_hash(&AdapterHashInput {
        adapter_id,
        base_model_id,
        format_version,
        files: hash_files,
    })
}

/// Inputs to [`export_adapter_bundle`].
#[derive(Debug, Clone, PartialEq)]
pub struct AdapterExportInput {
    pub weights: AdapterWeights,
    pub config: AdapterConfig,
    pub reward_version: String,
    pub timestamp_ms: u64,
    /// When set, the exported adapter is registered in this JSON registry
    /// (RLVR-034) so it can be listed locally.
    pub registry_path: Option<PathBuf>,
}

/// Result of [`export_adapter_bundle`].
#[derive(Debug, Clone, PartialEq)]
pub struct AdapterExportReport {
    pub adapter_id: String,
    pub adapter_hash: String,
    pub reward_policy_hash: String,
    pub out_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub files: Vec<(AdapterArtifactRole, PathBuf)>,
    pub registered: bool,
}

/// A bundle that has been read back and hash-verified by [`load_adapter_bundle`].
#[derive(Debug, Clone, PartialEq)]
pub struct LoadedAdapterBundle {
    pub manifest: AdapterManifest,
    pub weights: AdapterWeights,
    pub config: AdapterConfig,
    pub reward_policy: RewardPolicyArtifact,
    pub eval: AdapterEvalReport,
    pub model_card: AdapterModelCard,
}

/// Write the full adapter bundle to `out_dir` and return its report.
///
/// Writes the five artifact files first, hashes their on-disk bytes, then writes
/// `manifest.json` last so the manifest always describes the exact bytes on disk.
pub fn export_adapter_bundle(
    input: AdapterExportInput,
    report: &GrpoTrainerReport,
    out_dir: impl AsRef<Path>,
) -> Result<AdapterExportReport, RlvrError> {
    input.weights.validate()?;
    input.config.validate()?;
    if input.weights.adapter_id != input.config.adapter_id
        || input.weights.adapter_id != report.adapter_id
    {
        return Err(RlvrError::Config(format!(
            "adapter id mismatch: weights={}, config={}, report={}",
            input.weights.adapter_id, input.config.adapter_id, report.adapter_id
        )));
    }
    if input.weights.base_model_id != input.config.base_model_id
        || input.weights.base_model_id != report.base_model_id
    {
        return Err(RlvrError::Config(format!(
            "base model id mismatch: weights={}, config={}, report={}",
            input.weights.base_model_id, input.config.base_model_id, report.base_model_id
        )));
    }
    if input.weights.rank != input.config.rank
        || input.weights.target_modules != input.config.target_modules
    {
        return Err(RlvrError::Config(
            "adapter rank/target_modules differ between weights and config".into(),
        ));
    }
    if input.timestamp_ms == 0 {
        return Err(RlvrError::Config(
            "adapter export timestamp_ms must be greater than zero".into(),
        ));
    }
    require_non_empty("adapter_export_input.reward_version", &input.reward_version)?;

    let reward_policy = RewardPolicyArtifact::default_v01()?;
    let eval_report =
        AdapterEvalReport::from_grpo(&input.weights.adapter_id, &input.reward_version, report)?;
    let model_card = AdapterModelCard {
        name: format!("Fractal RLVR adapter {}", input.weights.adapter_id),
        adapter_id: input.weights.adapter_id.clone(),
        base_model_id: input.weights.base_model_id.clone(),
        training_mode: training_mode_str(input.weights.training_mode),
        reward_policy_id: reward_policy.policy_id.clone(),
        format: input.weights.format.clone(),
        intended_use:
            "Local-first route/clarification/answer behavior tuned with verifier rewards.".into(),
        out_of_scope: vec![
            "Committing raw prompts, answers, or traces on-chain.".into(),
            "Routing private data to external models without explicit approval.".into(),
        ],
        training_summary: AdapterTrainingSummary {
            training_mode: training_mode_str(input.weights.training_mode),
            reward_version: input.reward_version.clone(),
            rollout_count: report.rollout_count,
            group_count: report.group_count,
            format: input.weights.format.clone(),
            rank: input.weights.rank,
        },
        eval: eval_report.clone(),
        privacy: AdapterPrivacyStatement::default(),
        license: "Apache-2.0".into(),
        version: ADAPTER_BUNDLE_FORMAT_VERSION.into(),
    };
    model_card.validate()?;

    let out_dir = out_dir.as_ref();
    fs::create_dir_all(out_dir)?;

    let mut file_hashes: Vec<(AdapterArtifactRole, Vec<u8>)> = Vec::new();
    write_artifact(
        out_dir,
        AdapterArtifactRole::Weights,
        &input.weights,
        &mut file_hashes,
    )?;
    write_artifact(
        out_dir,
        AdapterArtifactRole::Config,
        &input.config,
        &mut file_hashes,
    )?;
    write_artifact(
        out_dir,
        AdapterArtifactRole::RewardPolicy,
        &reward_policy,
        &mut file_hashes,
    )?;
    write_artifact(
        out_dir,
        AdapterArtifactRole::EvalReport,
        &eval_report,
        &mut file_hashes,
    )?;
    write_artifact(
        out_dir,
        AdapterArtifactRole::ModelCard,
        &model_card,
        &mut file_hashes,
    )?;

    // Deterministic role order for both manifest and the adapter_hash input.
    file_hashes.sort_by_key(|(role, _)| *role);
    let manifest_files = file_hashes
        .iter()
        .map(|(role, bytes)| AdapterManifestFile {
            role: *role,
            file_name: role.file_name().into(),
            hash: hash_bytes(bytes),
            bytes: bytes.len(),
        })
        .collect::<Vec<_>>();
    let adapter_hash = adapter_hash_from(
        &input.weights.adapter_id,
        &input.weights.base_model_id,
        ADAPTER_BUNDLE_FORMAT_VERSION,
        manifest_files.clone(),
    )?;

    let manifest = AdapterManifest {
        adapter_id: input.weights.adapter_id.clone(),
        base_model_id: input.weights.base_model_id.clone(),
        format_version: ADAPTER_BUNDLE_FORMAT_VERSION.into(),
        timestamp_ms: input.timestamp_ms,
        files: manifest_files,
        adapter_hash: adapter_hash.clone(),
        reward_policy_hash: reward_policy.policy_hash.clone(),
        route_policy_hash: None,
    };
    manifest.validate()?;
    let manifest_path = out_dir.join("manifest.json");
    fs::write(&manifest_path, serde_json::to_string_pretty(&manifest)?)?;

    let mut registered = false;
    if let Some(registry_path) = input.registry_path.clone() {
        register_adapter_metadata(
            registry_path,
            AdapterMetadata {
                adapter_id: input.weights.adapter_id.clone(),
                base_model_id: input.weights.base_model_id.clone(),
                training_mode: input.weights.training_mode,
                reward_version: input.reward_version.clone(),
                data_local_only: input.config.data_local_only,
                chain_commit_hash: Some(adapter_hash.clone()),
            },
        )?;
        registered = true;
    }

    let files = file_hashes
        .iter()
        .map(|(role, _)| (*role, out_dir.join(role.file_name())))
        .collect::<Vec<_>>();
    Ok(AdapterExportReport {
        adapter_id: input.weights.adapter_id,
        adapter_hash,
        reward_policy_hash: reward_policy.policy_hash,
        out_dir: out_dir.to_path_buf(),
        manifest_path,
        files,
        registered,
    })
}

fn write_artifact<T: Serialize>(
    out_dir: &Path,
    role: AdapterArtifactRole,
    value: &T,
    file_hashes: &mut Vec<(AdapterArtifactRole, Vec<u8>)>,
) -> Result<(), RlvrError> {
    let bytes = serde_json::to_vec_pretty(value)?;
    let path = out_dir.join(role.file_name());
    fs::write(&path, &bytes)?;
    file_hashes.push((role, bytes));
    Ok(())
}

/// Read a bundle directory, re-hash every artifact, recompute `adapter_hash`,
/// and return the parsed bundle. Any mismatch fails closed with [`RlvrError`].
pub fn load_adapter_bundle(dir: impl AsRef<Path>) -> Result<LoadedAdapterBundle, RlvrError> {
    let dir = dir.as_ref();
    let manifest_path = dir.join("manifest.json");
    let manifest: AdapterManifest =
        serde_json::from_str(&fs::read_to_string(&manifest_path).map_err(RlvrError::from)?)?;
    manifest.validate()?;

    let mut entries = manifest.files.clone();
    entries.sort_by_key(|entry| entry.role);

    // Re-hash each artifact from its on-disk bytes and assert equality.
    let mut verified: Vec<AdapterManifestFile> = Vec::with_capacity(entries.len());
    for entry in &entries {
        let bytes = fs::read(dir.join(&entry.file_name))?;
        let actual = hash_bytes(&bytes);
        if actual != entry.hash {
            return Err(RlvrError::Config(format!(
                "adapter bundle {:?} hash mismatch for role {:?}: manifest={}, actual={}",
                dir, entry.role, entry.hash, actual
            )));
        }
        verified.push(AdapterManifestFile {
            role: entry.role,
            file_name: entry.file_name.clone(),
            hash: actual,
            bytes: bytes.len(),
        });
    }

    let recomputed = adapter_hash_from(
        &manifest.adapter_id,
        &manifest.base_model_id,
        &manifest.format_version,
        verified.clone(),
    )?;
    if recomputed != manifest.adapter_hash {
        return Err(RlvrError::Config(format!(
            "adapter_hash mismatch: manifest={}, recomputed={}",
            manifest.adapter_hash, recomputed
        )));
    }

    let read = |role: AdapterArtifactRole| -> Result<String, RlvrError> {
        Ok(fs::read_to_string(dir.join(role.file_name()))?)
    };
    let weights: AdapterWeights = serde_json::from_str(&read(AdapterArtifactRole::Weights)?)?;
    let config: AdapterConfig = serde_json::from_str(&read(AdapterArtifactRole::Config)?)?;
    let reward_policy: RewardPolicyArtifact =
        serde_json::from_str(&read(AdapterArtifactRole::RewardPolicy)?)?;
    let eval: AdapterEvalReport = serde_json::from_str(&read(AdapterArtifactRole::EvalReport)?)?;
    let model_card: AdapterModelCard =
        serde_json::from_str(&read(AdapterArtifactRole::ModelCard)?)?;
    weights.validate()?;
    config.validate()?;
    reward_policy.validate()?;
    eval.validate()?;
    model_card.validate()?;

    Ok(LoadedAdapterBundle {
        manifest,
        weights,
        config,
        reward_policy,
        eval,
        model_card,
    })
}

/// Deterministic placeholder LoRA weights for the harness/CLI/tests.
///
/// Real trainers should construct [`AdapterWeights`] directly with their own
/// tensors; this synthesizer exists so the local harness can produce a
/// reproducible, hash-stable, loadable bundle without a GPU. Values are drawn
/// from a seeded LCG into `[-0.05, 0.05]` and are a function of the GRPO
/// advantage report only.
pub fn synthesize_weights(
    report: &GrpoTrainerReport,
    rank: u32,
    target_modules: Vec<String>,
    model_dim: usize,
) -> Result<AdapterWeights, RlvrError> {
    if rank == 0 {
        return Err(RlvrError::Config(
            "synthesize rank must be greater than zero".into(),
        ));
    }
    if model_dim == 0 {
        return Err(RlvrError::Config(
            "synthesize model_dim must be greater than zero".into(),
        ));
    }
    if target_modules.is_empty() {
        return Err(RlvrError::Config(
            "synthesize target_modules must contain at least one module".into(),
        ));
    }
    let mut seen = Vec::new();
    for module in &target_modules {
        require_non_empty("synthesize target_modules[i]", module)?;
        if seen.contains(module) {
            return Err(RlvrError::Config(format!(
                "synthesize target_modules contains duplicate {module:?}"
            )));
        }
        seen.push(module.clone());
    }

    let seed_source = stable_hash(&(
        &report.adapter_id,
        &report.base_model_id,
        report.rollout_count,
        report.group_count,
        &report.advantages,
    ))?;
    let mut rng = DeterministicRng::new(seed_from_hash(&seed_source));
    let scale = 0.05f64;
    let mut tensors = Vec::new();
    for module in &target_modules {
        let a_len = rank as usize * model_dim;
        tensors.push(AdapterTensor {
            module: module.clone(),
            kind: AdapterTensorKind::LoraA,
            shape: vec![rank as usize, model_dim],
            values: fill_values(&mut rng, a_len, scale),
        });
        tensors.push(AdapterTensor {
            module: module.clone(),
            kind: AdapterTensorKind::LoraB,
            shape: vec![model_dim, rank as usize],
            values: fill_values(&mut rng, a_len, scale),
        });
    }

    let weights = AdapterWeights {
        format: ADAPTER_WEIGHTS_FORMAT.into(),
        adapter_id: report.adapter_id.clone(),
        base_model_id: report.base_model_id.clone(),
        training_mode: AdapterTrainingMode::Grpo,
        rank,
        target_modules,
        tensors,
    };
    weights.validate()?;
    Ok(weights)
}

/// Default LoRA target modules for synthesized weights.
pub fn default_target_modules() -> Vec<String> {
    vec!["q_proj".into(), "v_proj".into()]
}

struct DeterministicRng(u64);

impl DeterministicRng {
    fn new(seed: u64) -> Self {
        // Avoid an all-zero state which would stall the LCG.
        Self(if seed == 0 {
            0x9E37_79B9_7F4A_7C15
        } else {
            seed
        })
    }

    fn next_unit(&mut self) -> f64 {
        // Numerical-Recipes-style LCG constants; output the top 53 bits.
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 11) as f64) / ((1u64 << 53) as f64)
    }
}

fn fill_values(rng: &mut DeterministicRng, count: usize, scale: f64) -> Vec<f64> {
    (0..count)
        .map(|_| (rng.next_unit() * 2.0 - 1.0) * scale)
        .collect()
}

fn seed_from_hash(hash: &str) -> u64 {
    let prefix: String = hash.chars().take(16).collect();
    u64::from_str_radix(&prefix, 16).unwrap_or(0x9E37_79B9_7F4A_7C15)
}

fn shape_product(shape: &[usize]) -> Result<usize, RlvrError> {
    let mut product = 1usize;
    for dim in shape {
        if *dim == 0 {
            return Err(RlvrError::Config(
                "adapter_tensor.shape dimensions must be greater than zero".into(),
            ));
        }
        product = product.checked_mul(*dim).ok_or_else(|| {
            RlvrError::Config("adapter_tensor.shape product overflowed usize".into())
        })?;
    }
    Ok(product)
}

fn training_mode_str(mode: AdapterTrainingMode) -> String {
    mode.as_str().into()
}

fn require_non_empty(name: &str, value: &str) -> Result<(), RlvrError> {
    if value.trim().is_empty() {
        return Err(RlvrError::Config(format!("{name} cannot be empty")));
    }
    Ok(())
}

fn validate_hex_hash(name: &str, value: &str) -> Result<(), RlvrError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(RlvrError::Config(format!(
            "{name} must be a 64-character hex hash"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        demo_rollout_tasks, hash_bytes,
        trainer::{
            run_rollout_batch, train_grpo_adapter, DeterministicLocalActorRuntime,
            GrpoTrainerInput, RolloutRunnerInput,
        },
        SimulatorMode,
    };
    use std::fs;
    use std::path::PathBuf;

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fractal-rlvr-export-{label}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn demo_report(adapter_id: &str, base_model_id: &str) -> GrpoTrainerReport {
        let runtime = DeterministicLocalActorRuntime::new(base_model_id);
        let rollouts = run_rollout_batch(
            &runtime,
            RolloutRunnerInput {
                tasks: demo_rollout_tasks(2),
                actor_id: base_model_id.into(),
                trace_id_prefix: "export-rollout".into(),
                max_turns: 3,
                simulator_mode: SimulatorMode::Clean,
            },
        )
        .unwrap();
        let mut traces = rollouts.traces;
        // Duplicate each trace id-group so GRPO sees >=2 rollouts per task.
        let cloned = traces.clone();
        traces.extend(cloned);
        train_grpo_adapter(GrpoTrainerInput {
            base_model_id: base_model_id.into(),
            adapter_id: adapter_id.into(),
            rollouts: traces,
            output_dir: std::env::temp_dir(),
            learning_rate: 0.05,
            epochs: 2,
        })
        .unwrap()
    }

    fn export_input(report: &GrpoTrainerReport) -> AdapterExportInput {
        let weights = synthesize_weights(
            report,
            DEFAULT_ADAPTER_RANK,
            default_target_modules(),
            DEFAULT_MODEL_DIM,
        )
        .unwrap();
        let config = AdapterConfig {
            adapter_id: weights.adapter_id.clone(),
            base_model_id: weights.base_model_id.clone(),
            training_mode: weights.training_mode,
            rank: weights.rank,
            target_modules: weights.target_modules.clone(),
            max_turns: 3,
            data_local_only: true,
            base_model_hash: hash_bytes(weights.base_model_id.as_bytes()),
            created_from_checkpoint: None,
        };
        AdapterExportInput {
            weights,
            config,
            reward_version: MVP_REWARD_POLICY_V01_ID.into(),
            timestamp_ms: 1,
            registry_path: None,
        }
    }

    #[test]
    fn export_writes_six_files_with_stable_adapter_hash() {
        let dir = temp_dir("six-files");
        let report = demo_report("router-a", "tiny-router-base");
        let report_a = export_input(&report);
        let report_b = export_input(&report);

        let out_a = export_adapter_bundle(report_a, &report, &dir.join("a")).unwrap();
        let out_b = export_adapter_bundle(report_b, &report, &dir.join("b")).unwrap();

        for role in AdapterArtifactRole::ALL {
            assert!(dir.join("a").join(role.file_name()).exists());
        }
        assert!(dir.join("a").join("manifest.json").exists());
        assert_eq!(out_a.files.len(), AdapterArtifactRole::ALL.len());
        assert_eq!(out_a.adapter_hash.len(), 64);
        // Deterministic given identical artifacts (timestamp excluded from hash).
        assert_eq!(out_a.adapter_hash, out_b.adapter_hash);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn manifest_hashes_match_file_bytes_and_adapter_hash_is_field_sensitive() {
        let dir = temp_dir("hashes");
        let report = demo_report("router-b", "tiny-router-base");
        let out = export_adapter_bundle(export_input(&report), &report, &dir).unwrap();

        let manifest: AdapterManifest =
            serde_json::from_str(&fs::read_to_string(&out.manifest_path).unwrap()).unwrap();
        for entry in &manifest.files {
            let bytes = fs::read(dir.join(&entry.file_name)).unwrap();
            assert_eq!(entry.hash, hash_bytes(&bytes));
            assert_eq!(entry.bytes, bytes.len());
        }

        // Synthesize a different-but-valid weights artifact (same rank/modules,
        // different tensor values via a larger model dim) -> adapter_hash changes.
        let mut input2 = export_input(&report);
        input2.weights = synthesize_weights(
            &report,
            DEFAULT_ADAPTER_RANK,
            default_target_modules(),
            DEFAULT_MODEL_DIM + 4,
        )
        .unwrap();
        let out2 = export_adapter_bundle(input2, &report, &dir.join("v2")).unwrap();
        assert_ne!(out.adapter_hash, out2.adapter_hash);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn load_round_trips_and_verifies_a_clean_bundle() {
        let dir = temp_dir("round-trip");
        let report = demo_report("router-c", "tiny-router-base");
        let out = export_adapter_bundle(export_input(&report), &report, &dir).unwrap();

        let loaded = load_adapter_bundle(&out.out_dir).unwrap();
        assert_eq!(loaded.manifest.adapter_hash, out.adapter_hash);
        assert_eq!(loaded.weights.adapter_id, "router-c");
        assert_eq!(loaded.config.base_model_id, "tiny-router-base");
        assert!(loaded.eval.improved);
        assert!(loaded.model_card.privacy.data_local_only);
        assert!(!loaded.model_card.privacy.raw_data_committed);
        assert_eq!(loaded.reward_policy.policy_id, MVP_REWARD_POLICY_V01_ID);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn load_fails_when_an_artifact_is_tampered() {
        let dir = temp_dir("tamper");
        let report = demo_report("router-d", "tiny-router-base");
        let out = export_adapter_bundle(export_input(&report), &report, &dir).unwrap();

        // Flip a byte in the model card.
        let card_path = dir.join(AdapterArtifactRole::ModelCard.file_name());
        let mut bytes = fs::read(&card_path).unwrap();
        let last = bytes.len() - 2;
        bytes[last] = if bytes[last] == b'x' { b'y' } else { b'x' };
        fs::write(&card_path, bytes).unwrap();

        let err = load_adapter_bundle(&out.out_dir).unwrap_err();
        assert!(err.to_string().contains("hash mismatch"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn raw_training_data_stays_out_of_exported_artifacts() {
        let dir = temp_dir("privacy");
        let report = demo_report("router-priv", "tiny-router-base");
        let out = export_adapter_bundle(export_input(&report), &report, &dir).unwrap();

        // The demo rollouts' user prompt genuinely exists in training; assert it
        // never reaches any exported artifact (which carry only ids, hashes,
        // numeric eval fields, and the reward policy).
        let training_prompt = "What capacitor do I need for this board?";
        for role in AdapterArtifactRole::ALL {
            let raw = fs::read_to_string(out.out_dir.join(role.file_name())).unwrap();
            assert!(
                !raw.contains(training_prompt),
                "role {:?} leaked raw training content",
                role
            );
        }
        let manifest = fs::read_to_string(&out.manifest_path).unwrap();
        assert!(!manifest.contains(training_prompt));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn reward_policy_artifact_hash_matches_serialized_policy() {
        let artifact = RewardPolicyArtifact::default_v01().unwrap();
        assert_eq!(
            artifact.policy_hash,
            stable_hash(&MvpRewardPolicyV01::default()).unwrap()
        );
        artifact.validate().unwrap();
    }

    #[test]
    fn synthesize_weights_is_deterministic_and_respects_shape() {
        let report = demo_report("router-e", "tiny-router-base");
        let w1 = synthesize_weights(&report, 4, default_target_modules(), 16).unwrap();
        let w2 = synthesize_weights(&report, 4, default_target_modules(), 16).unwrap();
        assert_eq!(w1, w2);
        assert_eq!(w1.rank, 4);
        assert_eq!(w1.tensors.len(), 4); // 2 modules x (LoraA + LoraB)
        for tensor in &w1.tensors {
            assert_eq!(tensor.values.len(), tensor.shape.iter().product::<usize>(),);
            assert!(tensor.values.iter().all(|v| v.abs() <= 0.05));
        }
        // Different seed source (different report) -> different values.
        let other = demo_report("router-f", "tiny-router-base");
        let w3 = synthesize_weights(&other, 4, default_target_modules(), 16).unwrap();
        assert_ne!(w1.tensors[0].values, w3.tensors[0].values);
    }

    #[test]
    fn export_optionally_registers_adapter_metadata() {
        let dir = temp_dir("registry");
        let registry_path = dir.join("registry.json");
        let report = demo_report("router-g", "tiny-router-base");
        let mut input = export_input(&report);
        input.registry_path = Some(registry_path.clone());
        let out = export_adapter_bundle(input, &report, &dir.join("bundle")).unwrap();
        assert!(out.registered);

        let listed = crate::list_adapter_metadata(&registry_path).unwrap();
        let entry = listed.iter().find(|m| m.adapter_id == "router-g").unwrap();
        assert_eq!(
            entry.chain_commit_hash.as_deref(),
            Some(out.adapter_hash.as_str())
        );
        assert!(entry.data_local_only);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn export_rejects_mismatched_ids_and_invalid_inputs() {
        let report = demo_report("router-h", "tiny-router-base");
        let mut input = export_input(&report);
        input.weights.adapter_id = "different".into();
        let err = export_adapter_bundle(input, &report, &temp_dir("mismatch")).unwrap_err();
        assert!(err.to_string().contains("adapter id mismatch"));
    }
}
