//! Tool-allowlist policy package.
//!
//! Check requested tools against an `AgentManifest`'s declared `tool_allowlist`
//! (static manifest policy, distinct from runtime sandbox scanning).

use crate::protocol::AgentManifest;

/// Return true if `tool` is declared in the agent manifest allowlist.
pub fn allowed(manifest: &AgentManifest, tool: &str) -> bool {
    manifest
        .tool_allowlist
        .iter()
        .any(|allowed| allowed == tool)
}

/// Return requested tools that are not present in the manifest allowlist.
pub fn disallowed_subset(manifest: &AgentManifest, requested: &[String]) -> Vec<String> {
    requested
        .iter()
        .filter(|tool| !allowed(manifest, tool))
        .cloned()
        .collect()
}
