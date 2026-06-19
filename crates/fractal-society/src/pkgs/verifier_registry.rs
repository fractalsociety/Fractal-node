//! In-memory verifier package registry.
//!
//! In-memory registry of `VerifierPackage`s keyed by verifier id
//! (insert/lookup/list) — parallel to the artifact manifest registry.

use std::collections::BTreeMap;

use crate::verifier::VerifierPackage;

/// In-memory verifier registry keyed by verifier package id.
#[derive(Debug, Clone, Default)]
pub struct VerifierRegistry {
    packages: BTreeMap<String, VerifierPackage>,
}

impl VerifierRegistry {
    /// Create an empty verifier registry.
    pub fn new() -> Self {
        Self {
            packages: BTreeMap::new(),
        }
    }

    /// Insert a verifier package.
    ///
    /// Returns `false` when a package with the same id already exists; the
    /// existing package is left unchanged.
    pub fn insert(&mut self, pkg: VerifierPackage) -> bool {
        if self.packages.contains_key(&pkg.id) {
            return false;
        }
        self.packages.insert(pkg.id.clone(), pkg);
        true
    }

    /// Get a verifier package by id.
    pub fn get(&self, id: &str) -> Option<&VerifierPackage> {
        self.packages.get(id)
    }

    /// Return true when `id` is registered.
    pub fn contains(&self, id: &str) -> bool {
        self.packages.contains_key(id)
    }

    /// Return the number of registered verifier packages.
    pub fn len(&self) -> usize {
        self.packages.len()
    }

    /// Return true when no verifier packages are registered.
    pub fn is_empty(&self) -> bool {
        self.packages.is_empty()
    }

    /// List verifier packages in deterministic id order.
    pub fn list(&self) -> Vec<&VerifierPackage> {
        self.packages.values().collect()
    }
}
