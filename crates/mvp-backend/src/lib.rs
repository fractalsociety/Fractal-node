//! **PRD §18 M5** — Core MVP bridge library (`docs/prd.md`).
//!
//! ## Modules
//!
//! - [`receipt_json`] — load [`fractal_core::SettleBatchPayload`] from an operator JSON export
//!   (shape: `crates/mvp-backend/testdata/mvp_receipts_sample.json`).
//!
//! ## Binary
//!
//! Run **`fractal-mvp-bridge`** (`src/main.rs`): one `SETTLE_BATCH` then `CLAIM_PAYOUT` per receipt
//! over `eth_sendRawTransaction` (native borsh `Transaction`). Use **`MVP_RECEIPTS_JSON`** for a real
//! export, or **`MVP_RECEIPT_COUNT`** for synthetic PRD-scale batches (default 100).

pub mod receipt_json;
