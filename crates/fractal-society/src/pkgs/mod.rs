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
