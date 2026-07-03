//! RLVR-035: adapter export.
//!
//! The export path lives in `crates/rlvr/src/adapters/export.rs`
//! (`export_adapter_bundle`, `load_adapter_bundle`, `synthesize_weights`). These
//! integration tests prove the RLVR-035 contract end to end: a real GRPO report
//! (built from `demo_rollout_tasks`) exports a self-describing, hash-verified
//! bundle that the Fractal router/chat runtime can load back, and the manifest
//! `adapter_hash` survives the round-trip.

use std::fs;

use fractal_rlvr::{
    default_target_modules, demo_rollout_tasks, export_adapter_bundle, hash_bytes,
    load_adapter_bundle, run_rollout_batch, synthesize_weights, train_grpo_adapter,
    AdapterArtifactRole, AdapterConfig, AdapterExportInput, DeterministicLocalActorRuntime,
    GrpoTrainerInput, RolloutRunnerInput, SimulatorMode, DEFAULT_ADAPTER_RANK, DEFAULT_MODEL_DIM,
    MVP_REWARD_POLICY_V01_ID,
};

fn temp_dir(label: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "fractal-rlvr-it-export-{label}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    dir
}

/// A real GRPO report built from demo rollouts (>= 2 rollouts per task_id).
fn demo_report(adapter_id: &str, base_model_id: &str) -> fractal_rlvr::GrpoTrainerReport {
    let runtime = DeterministicLocalActorRuntime::new(base_model_id);
    let rollouts = run_rollout_batch(
        &runtime,
        RolloutRunnerInput {
            tasks: demo_rollout_tasks(2),
            actor_id: base_model_id.into(),
            trace_id_prefix: "it-export-rollout".into(),
            max_turns: 3,
            simulator_mode: SimulatorMode::Clean,
        },
    )
    .unwrap();
    let mut traces = rollouts.traces;
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

fn export_input(report: &fractal_rlvr::GrpoTrainerReport) -> AdapterExportInput {
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
fn export_then_load_round_trips_all_artifacts() {
    let dir = temp_dir("round-trip");
    let report = demo_report("it-router-a", "tiny-router-base");
    let out = export_adapter_bundle(export_input(&report), &report, &dir).unwrap();

    // All five artifacts + manifest present.
    for role in AdapterArtifactRole::ALL {
        assert!(dir.join(role.file_name()).exists());
    }
    assert!(dir.join("manifest.json").exists());

    let loaded = load_adapter_bundle(&dir).unwrap();
    assert_eq!(loaded.weights.adapter_id, "it-router-a");
    assert_eq!(loaded.config.base_model_id, "tiny-router-base");
    assert_eq!(loaded.config.rank, DEFAULT_ADAPTER_RANK);
    assert_eq!(loaded.reward_policy.policy_id, MVP_REWARD_POLICY_V01_ID);
    assert!(loaded.eval.rollout_count > 0);
    assert!(loaded.model_card.privacy.data_local_only);
    assert!(!loaded.model_card.privacy.raw_data_committed);

    // The manifest's adapter_hash is the content address and must match.
    assert_eq!(loaded.manifest.adapter_hash, out.adapter_hash);
    assert_eq!(loaded.manifest.files.len(), AdapterArtifactRole::ALL.len());
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn every_manifest_hash_matches_on_disk_bytes() {
    let dir = temp_dir("manifest-bytes");
    let report = demo_report("it-router-b", "tiny-router-base");
    let out = export_adapter_bundle(export_input(&report), &report, &dir).unwrap();
    let manifest = loaded_manifest(&dir);

    for entry in &manifest.files {
        let bytes = fs::read(dir.join(&entry.file_name)).unwrap();
        assert_eq!(entry.hash, hash_bytes(&bytes));
        assert_eq!(entry.bytes, bytes.len());
    }
    assert_eq!(manifest.adapter_hash, out.adapter_hash);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn load_fails_closed_after_tampering_with_weights() {
    let dir = temp_dir("tamper");
    let report = demo_report("it-router-c", "tiny-router-base");
    let _out = export_adapter_bundle(export_input(&report), &report, &dir).unwrap();

    // Append a comment byte to the weights artifact so its hash changes.
    let weights_path = dir.join(AdapterArtifactRole::Weights.file_name());
    let mut bytes = fs::read(&weights_path).unwrap();
    bytes.push(b' ');
    fs::write(&weights_path, bytes).unwrap();

    let err = load_adapter_bundle(&dir).unwrap_err();
    assert!(err.to_string().contains("hash mismatch"));
    let _ = fs::remove_dir_all(dir);
}

fn loaded_manifest(dir: &std::path::Path) -> fractal_rlvr::AdapterManifest {
    let raw = fs::read_to_string(dir.join("manifest.json")).unwrap();
    serde_json::from_str(&raw).unwrap()
}
