use fractal_society::pkgs::environment_validation::validate;
use fractal_society::protocol::{DomainAdapterRef, EnvironmentManifest, Hash};

fn valid_env() -> EnvironmentManifest {
    EnvironmentManifest {
        id: "env-1".to_string(),
        domain_adapter: DomainAdapterRef {
            id: "adapter".to_string(),
            version: "0.1.0".to_string(),
        },
        config: serde_json::json!({ "fixture": "synthetic" }),
        version_hash: Hash::new(b"environment-version"),
    }
}

#[test]
fn valid_manifest_passes() {
    assert!(validate(&valid_env()).is_ok());
}

#[test]
fn empty_id_fails() {
    let mut env = valid_env();
    env.id.clear();

    let errors = validate(&env).unwrap_err();

    assert!(errors.iter().any(|error| error.contains("id")));
}

#[test]
fn null_config_fails() {
    let mut env = valid_env();
    env.config = serde_json::Value::Null;

    let errors = validate(&env).unwrap_err();

    assert!(errors.iter().any(|error| error.contains("config")));
}

#[test]
fn bad_version_hash_hex_fails() {
    let mut env = valid_env();
    env.version_hash = Hash("zz".to_string());

    let errors = validate(&env).unwrap_err();

    assert!(errors.iter().any(|error| error.contains("version_hash")));
}
