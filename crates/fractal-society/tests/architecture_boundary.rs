//! Gate P02-N09: the generic kernel and canonical-schema modules must contain
//! no trading/venue code.
//!
//! This is a source-level guard: it reads the generic module sources and
//! asserts no trading-domain tokens appear, and that no generic module imports
//! from the domain `adapters`. A stronger future version splits the kernel into
//! its own crate so cargo dependencies enforce this at compile time; until then
//! this test prevents regressions.
//!
//! Note: generic words like "order" or "trading" are intentionally NOT banned,
//! because they appear legitimately in architecture comments (e.g. "field
//! order", "trading as the first domain adapter"). Only unambiguous
//! trading/venue identifiers are banned.

use std::path::PathBuf;

/// Tokens that unambiguously indicate trading code has leaked in. "venue" is
/// intentionally NOT banned: it is a legitimate domain-neutral field name in
/// the canonical data-source schema (`DataSource::Live { venue }`).
const BANNED: &[&str] = &["hyperliquid", "perpetual", "perp", "adapters::"];

/// Generic (domain-neutral) modules that must stay trading-free.
const GENERIC_MODULES: &[&str] = &[
    "src/protocol.rs",
    "src/artifact.rs",
    "src/simulation.rs",
    "src/verifier.rs",
    "src/canonical.rs",
    "src/signing.rs",
    "src/kernel.rs",
];

#[test]
fn generic_modules_have_no_trading_tokens() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for rel in GENERIC_MODULES {
        let path = root.join(rel);
        let src = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let lower = src.to_ascii_lowercase();
        for token in BANNED {
            assert!(
                !lower.contains(token),
                "banned trading token '{}' found in generic module {}",
                token,
                rel
            );
        }
    }
}
