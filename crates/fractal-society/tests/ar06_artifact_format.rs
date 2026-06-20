//! AR-06: navigable artifact directory format — write/read round-trip + tamper.

use std::env;

use fractal_society::adapters::trading::{
    CashBaseline, TradingAdapter, TradingAgent, TradingConfig,
};
use fractal_society::artifact_format::{read_artifact_dir, write_artifact_dir};
use fractal_society::exploration::{
    ExplorationGraph, ExplorationNode, NodeKind, NodeStatus, ProvenanceTag,
};
use fractal_society::kernel::{run, KernelConfig};
use fractal_society::pipeline::run_pipeline_default;
use fractal_society::protocol::Hash;
use fractal_society::signing::AuthorSigner;

const SEED: u64 = 11;
const STEPS: u64 = 12;

async fn pipeline_result() -> fractal_society::pipeline::PipelineResult {
    let signer = AuthorSigner::from_seed(&[9u8; 32]);
    let tcfg = TradingConfig {
        max_steps: STEPS,
        ..TradingConfig::default()
    };
    let kcfg = KernelConfig {
        episodes: 1,
        max_steps_per_episode: STEPS,
    };
    let ts = chrono::DateTime::from_timestamp(0, 0).unwrap();

    let cash = run(
        TradingAdapter::new(SEED, tcfg.clone()).unwrap(),
        CashBaseline::new(),
        SEED,
        &kcfg,
    )
    .await
    .unwrap();

    run_pipeline_default(
        TradingAdapter::new(SEED, tcfg.clone()).unwrap(),
        TradingAgent::new(SEED),
        SEED,
        kcfg,
        tcfg,
        vec![("cash".to_string(), cash)],
        &signer,
        ts,
    )
    .await
    .unwrap()
}

fn sample_graph() -> ExplorationGraph {
    let mut graph = ExplorationGraph::new();
    graph
        .add_node(ExplorationNode {
            id: "h1".to_string(),
            kind: NodeKind::Hypothesis,
            status: NodeStatus::Proven,
            description: "ma-cross beats cash".to_string(),
            outcome_summary: Some("yes".to_string()),
            parent: None,
            children: vec!["d1".to_string()],
            evidence_ref: None,
            provenance: ProvenanceTag::Human,
            dead_end_reason: None,
        })
        .unwrap();
    graph
        .add_node(ExplorationNode {
            id: "d1".to_string(),
            kind: NodeKind::DeadEnd,
            status: NodeStatus::Disproven,
            description: "mean reversion on 1m".to_string(),
            outcome_summary: Some("overfit".to_string()),
            parent: Some("h1".to_string()),
            children: Vec::new(),
            evidence_ref: None,
            provenance: ProvenanceTag::AiExecuted,
            dead_end_reason: Some("overfit the training window".to_string()),
        })
        .unwrap();
    graph
}

#[tokio::test]
async fn write_then_read_round_trips_artifacts() {
    let result = pipeline_result().await;
    let graph = sample_graph();
    let root = env::temp_dir().join(format!("fractal-ar06-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);

    let written_root_hash =
        write_artifact_dir(&root, &result, Some(&graph)).expect("write must succeed");

    let loaded = read_artifact_dir(&root).expect("read must succeed");

    // Float-free artifacts round-trip exactly (compare by content hash).
    assert_eq!(
        Hash::of(&loaded.manifest).unwrap(),
        Hash::of(&result.proof_manifest).unwrap(),
        "proof manifest must round-trip"
    );
    assert_eq!(
        loaded.bundle.bundle_hash().unwrap(),
        result.bundle.bundle_hash().unwrap(),
        "run bundle must round-trip"
    );
    assert_eq!(
        loaded.graph.as_ref().unwrap().content_hash().unwrap(),
        graph.content_hash().unwrap(),
        "exploration graph must round-trip"
    );
    // The scorecard contains f64 fields whose canonical (Rust-Display) form is
    // not always bit-preserved by a serde parse, so we verify the writer stored
    // the canonical bytes faithfully: the file's hash equals Hash::of(original).
    let scorecard_bytes =
        std::fs::read(root.join("evidence/scorecard.json")).expect("scorecard file present");
    assert_eq!(
        Hash::new(&scorecard_bytes),
        Hash::of(&result.scorecard).unwrap(),
        "scorecard must be stored in canonical form"
    );
    assert_eq!(loaded.scorecard.agent_id, result.scorecard.agent_id);
    assert_eq!(loaded.scorecard.proof_level, result.scorecard.proof_level);
    // Read recomputes the same root hash the writer returned.
    assert_eq!(loaded.root_hash, written_root_hash);

    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn tampering_a_file_changes_root_hash() {
    let result = pipeline_result().await;
    let root = env::temp_dir().join(format!("fractal-ar06-tamper-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);

    let written_root_hash = write_artifact_dir(&root, &result, None).expect("write must succeed");

    // Tamper with a layer file (not one of the deserialized JSON files).
    let paper = root.join("PAPER.md");
    let original = std::fs::read(&paper).unwrap();
    std::fs::write(&paper, b"tampered!").unwrap();

    let loaded = read_artifact_dir(&root).expect("read still succeeds (PAPER.md isn't parsed)");
    assert_ne!(
        loaded.root_hash, written_root_hash,
        "tampering any file must change the root hash"
    );

    // Restore and confirm the root hash returns to the original.
    std::fs::write(&paper, &original).unwrap();
    let restored = read_artifact_dir(&root).unwrap();
    assert_eq!(restored.root_hash, written_root_hash);

    let _ = std::fs::remove_dir_all(&root);
}
