use fractal_society::pkgs::agent_manifest_freeze::{freeze, FreezeInput};
use fractal_society::protocol::Hash;

fn input_with_code(code_bytes: Vec<u8>) -> FreezeInput {
    FreezeInput {
        agent_id: "agent.alpha".to_string(),
        author: "alice".to_string(),
        version: "1.2.3".to_string(),
        code_bytes,
        tool_allowlist: vec!["calc".to_string(), "search".to_string()],
        license: "Apache-2.0".to_string(),
    }
}

#[test]
fn freeze_is_deterministic() {
    let a = freeze(input_with_code(b"fn act() { hold(); }".to_vec())).unwrap();
    let b = freeze(input_with_code(b"fn act() { hold(); }".to_vec())).unwrap();

    assert_eq!(a.code_hash, b.code_hash);
    assert_eq!(a.code_hash, Hash::new(b"fn act() { hold(); }"));
}

#[test]
fn one_byte_change_changes_code_hash() {
    let a = freeze(input_with_code(b"fn act() { hold(); }".to_vec())).unwrap();
    let b = freeze(input_with_code(b"fn act() { trade(); }".to_vec())).unwrap();

    assert_ne!(a.code_hash, b.code_hash);
}

#[test]
fn metadata_round_trips_and_network_is_denied() {
    let manifest = freeze(input_with_code(b"code".to_vec())).unwrap();

    assert_eq!(manifest.id, "agent.alpha");
    assert_eq!(manifest.version, "1.2.3");
    assert_eq!(manifest.author, "alice");
    assert_eq!(manifest.tool_allowlist, vec!["calc", "search"]);
    assert_eq!(manifest.license, "Apache-2.0");
    assert!(manifest.model_ref.is_none());
    assert!(manifest.system_prompt.is_none());
    assert!(manifest.skill_dependencies.is_empty());
    assert!(!manifest.network_policy.allow_network);
    assert!(manifest.network_policy.allowed_domains.is_empty());
}
