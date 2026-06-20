//! Agent-manifest freeze package.
//!
//! Builds a deterministic, tamper-evident [`AgentManifest`] from submission
//! metadata and code bytes.

use crate::protocol::{AgentManifest, Hash, NetworkPolicy, ResourceLimits};

/// Input metadata used to freeze an agent manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FreezeInput {
    /// Stable agent identifier.
    pub agent_id: String,
    /// Author identity.
    pub author: String,
    /// Agent version.
    pub version: String,
    /// Raw code/package bytes to hash into the manifest.
    pub code_bytes: Vec<u8>,
    /// Tool identifiers the agent is allowed to use.
    pub tool_allowlist: Vec<String>,
    /// License label for the agent package.
    pub license: String,
}

/// Build a frozen [`AgentManifest`] from agent metadata and code bytes.
pub fn freeze(input: FreezeInput) -> crate::Result<AgentManifest> {
    Ok(AgentManifest {
        id: input.agent_id,
        version: input.version,
        author: input.author,
        model_ref: None,
        system_prompt: None,
        code_hash: Hash::new(&input.code_bytes),
        tool_allowlist: input.tool_allowlist,
        skill_dependencies: Vec::new(),
        resource_limits: ResourceLimits {
            max_memory_mb: 64,
            max_runtime_seconds: 10,
            max_cpu_cores: 1,
        },
        network_policy: NetworkPolicy {
            allow_network: false,
            allowed_domains: Vec::new(),
        },
        license: input.license,
    })
}
