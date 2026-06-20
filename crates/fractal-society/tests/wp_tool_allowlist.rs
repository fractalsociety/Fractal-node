use fractal_society::pkgs::tool_allowlist::{allowed, disallowed_subset};
use fractal_society::protocol::{AgentManifest, Hash, NetworkPolicy, ResourceLimits};

fn manifest() -> AgentManifest {
    AgentManifest {
        id: "agent-1".to_string(),
        version: "0.1.0".to_string(),
        author: "author-1".to_string(),
        model_ref: None,
        system_prompt: None,
        code_hash: Hash::new(b"agent-code"),
        tool_allowlist: vec!["calc".to_string(), "search".to_string()],
        skill_dependencies: Vec::new(),
        resource_limits: ResourceLimits {
            max_memory_mb: 128,
            max_runtime_seconds: 30,
            max_cpu_cores: 1,
        },
        network_policy: NetworkPolicy {
            allow_network: false,
            allowed_domains: Vec::new(),
        },
        license: "MIT".to_string(),
    }
}

#[test]
fn allowed_tool_returns_true() {
    assert!(allowed(&manifest(), "calc"));
}

#[test]
fn disallowed_tool_returns_false() {
    assert!(!allowed(&manifest(), "shell"));
}

#[test]
fn disallowed_subset_returns_exact_violators() {
    let requested = vec![
        "calc".to_string(),
        "shell".to_string(),
        "search".to_string(),
        "browser".to_string(),
    ];

    assert_eq!(
        disallowed_subset(&manifest(), &requested),
        vec!["shell".to_string(), "browser".to_string()]
    );
}
