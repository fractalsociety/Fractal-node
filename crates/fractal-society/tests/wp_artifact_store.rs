use std::fs;
use std::path::PathBuf;

use fractal_society::persistence::artifact_store::{
    ArtifactStore, FileArtifactStore, InMemoryArtifactStore,
};
use fractal_society::protocol::Hash;

#[test]
fn in_memory_store_round_trips_bytes() {
    let bytes = b"scorecard bytes";
    let hash = Hash::new(bytes);
    let mut store = InMemoryArtifactStore::new();

    store.put(&hash, bytes).unwrap();

    assert!(store.contains(&hash).unwrap());
    assert_eq!(store.get(&hash).unwrap(), Some(bytes.to_vec()));
}

#[test]
fn file_store_round_trips_bytes_after_reopen() {
    let root = temp_dir("roundtrip");
    let bytes = b"proof manifest bytes";
    let hash = Hash::new(bytes);
    {
        let mut store = FileArtifactStore::new(&root);
        store.put(&hash, bytes).unwrap();
        assert!(store.contains(&hash).unwrap());
    }

    let reopened = FileArtifactStore::new(&root);

    assert_eq!(reopened.get(&hash).unwrap(), Some(bytes.to_vec()));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn missing_hash_returns_none() {
    let root = temp_dir("missing");
    let store = FileArtifactStore::new(&root);
    let missing = Hash::new(b"missing");

    assert!(!store.contains(&missing).unwrap());
    assert_eq!(store.get(&missing).unwrap(), None);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn same_bytes_use_same_content_hash_key() {
    let first = b"same bytes";
    let second = b"same bytes";

    assert_eq!(Hash::new(first), Hash::new(second));
}

#[test]
fn put_rejects_mismatched_hash() {
    let bytes = b"artifact";
    let wrong_hash = Hash::new(b"different");
    let mut store = InMemoryArtifactStore::new();

    let err = store.put(&wrong_hash, bytes).unwrap_err();

    assert!(err.to_string().contains("artifact hash mismatch"));
}

fn temp_dir(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "fractal_society_wp_artifact_store_{label}_{}",
        std::process::id()
    ))
}
