use std::collections::HashMap;

use fractal_society::artifact::{ArtifactManifest, ArtifactType};
use fractal_society::pkgs::manifest_registry::ArtifactRegistry;
use fractal_society::protocol::{Hash, Visibility};

fn manifest(id: &str, content: &[u8]) -> ArtifactManifest {
    ArtifactManifest {
        id: id.to_string(),
        version: "1.0.0".to_string(),
        artifact_type: ArtifactType::AgentPackage,
        content_hash: Hash::new(content),
        size_bytes: content.len() as u64,
        author: "author-1".to_string(),
        visibility: Visibility::Private,
        license: "MIT".to_string(),
        dependencies: HashMap::new(),
        metadata: serde_json::json!({}),
        created_at: chrono::DateTime::from_timestamp(0, 0).unwrap(),
        signature: None,
    }
}

#[test]
fn insert_get_and_contains_work() {
    let mut registry = ArtifactRegistry::new();
    let manifest = manifest("artifact-1", b"artifact-1");
    let hash = manifest.content_hash.clone();

    assert!(registry.insert(manifest));
    assert!(registry.contains(&hash));
    assert_eq!(registry.get(&hash).unwrap().id, "artifact-1");
}

#[test]
fn duplicate_hash_insert_returns_false_and_does_not_clobber() {
    let mut registry = ArtifactRegistry::new();
    let first = manifest("artifact-1", b"same-content");
    let second = manifest("artifact-2", b"same-content");
    let hash = first.content_hash.clone();

    assert!(registry.insert(first));
    assert!(!registry.insert(second));

    assert_eq!(registry.len(), 1);
    assert_eq!(registry.get(&hash).unwrap().id, "artifact-1");
}

#[test]
fn len_is_empty_and_list_are_correct() {
    let mut registry = ArtifactRegistry::new();

    assert!(registry.is_empty());
    assert_eq!(registry.len(), 0);
    assert!(registry.list().is_empty());

    registry.insert(manifest("artifact-1", b"artifact-1"));
    registry.insert(manifest("artifact-2", b"artifact-2"));

    assert!(!registry.is_empty());
    assert_eq!(registry.len(), 2);
    assert_eq!(registry.list().len(), 2);
}

#[test]
fn default_creates_empty_registry() {
    let registry = ArtifactRegistry::default();

    assert!(registry.is_empty());
    assert_eq!(registry.len(), 0);
}
