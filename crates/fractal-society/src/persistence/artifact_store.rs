//! Content-addressed artifact store implementations.
//!
//! Stores serialized evidence, manifests, scorecards, and bundles by their
//! protocol [`Hash`](crate::protocol::Hash).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::protocol::Hash;

/// Content-addressed byte store.
pub trait ArtifactStore {
    /// Store `bytes` under `hash`.
    fn put(&mut self, hash: &Hash, bytes: &[u8]) -> Result<()>;

    /// Fetch bytes for `hash`, or `None` when absent.
    fn get(&self, hash: &Hash) -> Result<Option<Vec<u8>>>;

    /// Return true when the store has `hash`.
    fn contains(&self, hash: &Hash) -> Result<bool>;
}

/// In-memory artifact store.
#[derive(Debug, Default, Clone)]
pub struct InMemoryArtifactStore {
    artifacts: HashMap<Hash, Vec<u8>>,
}

impl InMemoryArtifactStore {
    /// Create an empty in-memory artifact store.
    pub fn new() -> Self {
        Self::default()
    }
}

impl ArtifactStore for InMemoryArtifactStore {
    fn put(&mut self, hash: &Hash, bytes: &[u8]) -> Result<()> {
        validate_content_hash(hash, bytes)?;
        self.artifacts.insert(hash.clone(), bytes.to_vec());
        Ok(())
    }

    fn get(&self, hash: &Hash) -> Result<Option<Vec<u8>>> {
        Ok(self.artifacts.get(hash).cloned())
    }

    fn contains(&self, hash: &Hash) -> Result<bool> {
        Ok(self.artifacts.contains_key(hash))
    }
}

/// Filesystem-backed artifact store.
#[derive(Debug, Clone)]
pub struct FileArtifactStore {
    root: PathBuf,
}

impl FileArtifactStore {
    /// Create a filesystem artifact store rooted at `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Borrow the store root path.
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn artifact_path(&self, hash: &Hash) -> PathBuf {
        self.root.join(&hash.0)
    }
}

impl ArtifactStore for FileArtifactStore {
    fn put(&mut self, hash: &Hash, bytes: &[u8]) -> Result<()> {
        validate_content_hash(hash, bytes)?;
        std::fs::create_dir_all(&self.root)?;
        std::fs::write(self.artifact_path(hash), bytes)?;
        Ok(())
    }

    fn get(&self, hash: &Hash) -> Result<Option<Vec<u8>>> {
        let path = self.artifact_path(hash);
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(std::fs::read(path)?))
    }

    fn contains(&self, hash: &Hash) -> Result<bool> {
        Ok(self.artifact_path(hash).exists())
    }
}

fn validate_content_hash(hash: &Hash, bytes: &[u8]) -> Result<()> {
    let actual = Hash::new(bytes);
    if &actual != hash {
        return Err(Error::InvalidArtifact(format!(
            "artifact hash mismatch: expected {}, got {}",
            hash.0, actual.0
        )));
    }
    Ok(())
}
