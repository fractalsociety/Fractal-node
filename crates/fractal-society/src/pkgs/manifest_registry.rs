//! In-memory artifact manifest registry package.
//!
//! In-memory artifact registry: insert/lookup/list `ArtifactManifest`s by
//! content hash (a minimal stand-in for the PRD's Artifact Registry).

use std::collections::HashMap;

use crate::artifact::{ArtifactHash, ArtifactManifest};

/// In-memory artifact registry keyed by content hash.
#[derive(Debug, Clone)]
pub struct ArtifactRegistry {
    manifests: HashMap<ArtifactHash, ArtifactManifest>,
}

impl ArtifactRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            manifests: HashMap::new(),
        }
    }

    /// Insert a manifest. Returns `false` if the content hash already exists.
    pub fn insert(&mut self, manifest: ArtifactManifest) -> bool {
        let hash = manifest.content_hash.clone();
        match self.manifests.entry(hash) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(manifest);
                true
            }
            std::collections::hash_map::Entry::Occupied(_) => false,
        }
    }

    /// Get a manifest by content hash.
    pub fn get(&self, hash: &ArtifactHash) -> Option<&ArtifactManifest> {
        self.manifests.get(hash)
    }

    /// Return true if a content hash exists in the registry.
    pub fn contains(&self, hash: &ArtifactHash) -> bool {
        self.manifests.contains_key(hash)
    }

    /// Number of registered manifests.
    pub fn len(&self) -> usize {
        self.manifests.len()
    }

    /// Return true if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.manifests.is_empty()
    }

    /// List registered manifests.
    pub fn list(&self) -> Vec<&ArtifactManifest> {
        self.manifests.values().collect()
    }
}

impl Default for ArtifactRegistry {
    fn default() -> Self {
        Self::new()
    }
}
