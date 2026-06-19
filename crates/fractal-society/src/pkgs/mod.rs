//! Parallel work-package modules (architect-owned registry).
//!
//! Each child module here is one independently-assignable work package from
//! `crates/fractal-society/WORK_PACKAGES.md`. This file is the **only** shared
//! seam and is **architect-owned**: it is pre-declared and must NOT be edited by
//! any work-package agent. Each agent replaces the contents of their own
//! `pkgs/<name>.rs` stub and creates their own `tests/wp_<name>.rs` — nothing
//! else. That gives literal zero file-overlap across parallel agents, and the
//! crate always compiles (empty stubs are valid modules).
//!
//! When all packages land, a follow-up integration task may relocate these
//! modules into their canonical homes (`verifier.rs` siblings, a new
//! `verification/` module, `protocol.rs` helpers, etc.).

pub mod accounting_integrity;
pub mod agent_manifest_freeze;
pub mod cost_completeness;
pub mod dataset_integrity;
pub mod disclosure_tiers;
pub mod merkle_commitment;
pub mod proof_manifest;
pub mod reproducibility;
pub mod reputation_events;
pub mod reward_gate;
pub mod risk_policy;
pub mod scorecard_reproduction;

// --- Second batch: packages 13–25 (same zero-overlap rules as above) ---
pub mod baseline_correctness;
pub mod chain_commitment;
pub mod gap_detection;
pub mod graph_projection;
pub mod leaderboard;
pub mod proof_level_resolver;
pub mod review_conflicts;
pub mod reviewer_grants;
pub mod sandbox_policy;
pub mod season_state_machine;
pub mod submission_freeze;
pub mod sybil_detection;
pub mod temporal_leakage;

// --- Third batch: packages 26–37 (same zero-overlap rules as above) ---
pub mod confidence_intervals;
pub mod environment_validation;
pub mod manifest_registry;
pub mod metric_set_ops;
pub mod pipeline_contract;
pub mod proof_card;
pub mod protocol_validation;
pub mod replication_check;
pub mod risk_adjusted_metrics;
pub mod run_bundle;
pub mod seed_derivation;
pub mod verifier_summary;

// --- Fourth batch: packages 38–51 (same zero-overlap rules as above) ---
pub mod appeals_flow;
pub mod canonical_roundtrip;
pub mod challenge_bond;
pub mod data_quality_report;
pub mod dataset_window;
pub mod determinism_audit;
pub mod evidence_summary;
pub mod execution_budget;
pub mod holdout_isolation;
pub mod overfit_detector;
pub mod review_aggregation;
pub mod reward_split;
pub mod skill_graph;
pub mod tool_allowlist;
