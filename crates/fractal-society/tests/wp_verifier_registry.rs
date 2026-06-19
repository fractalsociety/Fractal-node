use fractal_society::pkgs::verifier_registry::VerifierRegistry;
use fractal_society::verifier::{
    FixtureResult, ResourceBudget, SafetyPolicy, VerificationLogic, VerifierPackage,
};
use serde_json::json;

fn pkg(id: &str, name: &str) -> VerifierPackage {
    VerifierPackage {
        id: id.to_string(),
        version: "1.0.0".to_string(),
        name: name.to_string(),
        description: format!("{name} verifier"),
        author: "fractal".to_string(),
        input_schema: json!({"type": "object"}),
        output_schema: json!({"type": "object"}),
        verification_logic: VerificationLogic::Inline {
            code: "return true".to_string(),
            language: "rust".to_string(),
        },
        calibration_fixtures: vec![fractal_society::verifier::CalibrationFixture {
            name: "passes".to_string(),
            expected_result: FixtureResult::Pass,
            input_data: json!({}),
            description: "minimal passing fixture".to_string(),
        }],
        known_false_positives: vec![],
        known_false_negatives: vec![],
        required_evidence: vec!["evidence".to_string()],
        resource_budget: ResourceBudget {
            max_runtime_seconds: 1,
            max_memory_mb: 64,
            max_cpu_cores: 1,
        },
        safety_policy: SafetyPolicy {
            allow_network: false,
            allow_fs: false,
            allow_subprocess: false,
        },
        license: "MIT".to_string(),
    }
}

#[test]
fn insert_get_and_contains_work() {
    let mut registry = VerifierRegistry::new();
    let package = pkg("accounting", "Accounting");

    assert!(registry.insert(package.clone()));
    assert!(registry.contains("accounting"));
    assert_eq!(registry.get("accounting").unwrap().name, package.name);
    assert!(registry.get("missing").is_none());
}

#[test]
fn duplicate_id_insert_returns_false_and_preserves_original() {
    let mut registry = VerifierRegistry::new();

    assert!(registry.insert(pkg("risk", "Risk A")));
    assert!(!registry.insert(pkg("risk", "Risk B")));

    assert_eq!(registry.len(), 1);
    assert_eq!(registry.get("risk").unwrap().name, "Risk A");
}

#[test]
fn len_list_and_default_are_correct() {
    let mut registry = VerifierRegistry::default();

    assert!(registry.is_empty());
    assert!(registry.insert(pkg("zeta", "Zeta")));
    assert!(registry.insert(pkg("alpha", "Alpha")));

    let listed = registry
        .list()
        .into_iter()
        .map(|package| package.id.as_str())
        .collect::<Vec<_>>();

    assert_eq!(registry.len(), 2);
    assert_eq!(listed, vec!["alpha", "zeta"]);
}
