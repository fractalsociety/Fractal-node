//! Domain adapters (PHASE-02 and beyond).
//!
//! All domain-specific logic lives in adapters, never in the generic kernel or
//! canonical schema modules. This is what gate P02-N09's architecture boundary
//! guards: the kernel must not import anything from this module.

pub mod reference;

pub use reference::{
    BanditAction, BanditObservation, BanditOutcome, ReferenceAdapter, ReferenceAgent,
};
