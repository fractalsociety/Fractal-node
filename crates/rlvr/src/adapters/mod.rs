//! Adapter registry and export model-card support.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::RlvrError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdapterTrainingMode {
    Grpo,
    Dpo,
    Sft,
}

impl AdapterTrainingMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Grpo => "grpo",
            Self::Dpo => "dpo",
            Self::Sft => "sft",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterMetadata {
    pub adapter_id: String,
    pub base_model_id: String,
    pub training_mode: AdapterTrainingMode,
    pub reward_version: String,
    pub data_local_only: bool,
    pub chain_commit_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterRegistry {
    pub adapters: Vec<AdapterMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterRegistryStore {
    path: PathBuf,
}

impl AdapterMetadata {
    pub fn validate(&self) -> Result<(), RlvrError> {
        require_non_empty("adapter_id", &self.adapter_id)?;
        require_non_empty("base_model_id", &self.base_model_id)?;
        require_non_empty("reward_version", &self.reward_version)?;
        if let Some(hash) = &self.chain_commit_hash {
            validate_hex_hash("chain_commit_hash", hash)?;
        }
        Ok(())
    }
}

impl Default for AdapterRegistry {
    fn default() -> Self {
        Self {
            adapters: Vec::new(),
        }
    }
}

impl AdapterRegistry {
    pub fn validate(&self) -> Result<(), RlvrError> {
        let mut ids = Vec::new();
        for adapter in &self.adapters {
            adapter.validate()?;
            if ids.contains(&adapter.adapter_id) {
                return Err(RlvrError::Config(format!(
                    "adapter registry contains duplicate adapter id {:?}",
                    adapter.adapter_id
                )));
            }
            ids.push(adapter.adapter_id.clone());
        }
        Ok(())
    }

    pub fn register(&mut self, metadata: AdapterMetadata) -> Result<(), RlvrError> {
        metadata.validate()?;
        if let Some(existing) = self
            .adapters
            .iter_mut()
            .find(|adapter| adapter.adapter_id == metadata.adapter_id)
        {
            *existing = metadata;
        } else {
            self.adapters.push(metadata);
        }
        self.adapters.sort_by(|left, right| {
            left.adapter_id
                .cmp(&right.adapter_id)
                .then_with(|| left.base_model_id.cmp(&right.base_model_id))
        });
        self.validate()
    }

    pub fn list(&self) -> Result<Vec<AdapterMetadata>, RlvrError> {
        self.validate()?;
        Ok(self.adapters.clone())
    }
}

impl AdapterRegistryStore {
    pub fn open(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<AdapterRegistry, RlvrError> {
        if !self.path.exists() {
            return Ok(AdapterRegistry::default());
        }
        let raw = fs::read_to_string(&self.path)?;
        if raw.trim().is_empty() {
            return Ok(AdapterRegistry::default());
        }
        let registry: AdapterRegistry = serde_json::from_str(&raw)?;
        registry.validate()?;
        Ok(registry)
    }

    pub fn save(&self, registry: &AdapterRegistry) -> Result<(), RlvrError> {
        registry.validate()?;
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        fs::write(&self.path, serde_json::to_string_pretty(registry)?)?;
        Ok(())
    }

    pub fn register(&self, metadata: AdapterMetadata) -> Result<AdapterRegistry, RlvrError> {
        let mut registry = self.load()?;
        registry.register(metadata)?;
        self.save(&registry)?;
        Ok(registry)
    }

    pub fn list(&self) -> Result<Vec<AdapterMetadata>, RlvrError> {
        self.load()?.list()
    }
}

pub fn register_adapter_metadata(
    path: impl Into<PathBuf>,
    metadata: AdapterMetadata,
) -> Result<AdapterRegistry, RlvrError> {
    AdapterRegistryStore::open(path).register(metadata)
}

pub fn list_adapter_metadata(path: impl Into<PathBuf>) -> Result<Vec<AdapterMetadata>, RlvrError> {
    AdapterRegistryStore::open(path).list()
}

fn require_non_empty(name: &str, value: &str) -> Result<(), RlvrError> {
    if value.trim().is_empty() {
        return Err(RlvrError::Config(format!("{name} cannot be empty")));
    }
    Ok(())
}

fn validate_hex_hash(name: &str, value: &str) -> Result<(), RlvrError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(RlvrError::Config(format!(
            "{name} must be a 64-character hex hash"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash_bytes;

    #[test]
    fn registry_registers_and_lists_adapter_metadata_locally() {
        let dir = std::env::temp_dir().join(format!(
            "fractal-rlvr-adapter-registry-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("adapters.json");
        let commit_hash = hash_bytes(b"chain-commit");
        let store = AdapterRegistryStore::open(&path);

        store
            .register(AdapterMetadata {
                adapter_id: "router-v1".into(),
                base_model_id: "tiny-router-base".into(),
                training_mode: AdapterTrainingMode::Grpo,
                reward_version: "reward-v0.1".into(),
                data_local_only: true,
                chain_commit_hash: Some(commit_hash.clone()),
            })
            .unwrap();
        let listed = store.list().unwrap();

        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].adapter_id, "router-v1");
        assert_eq!(listed[0].base_model_id, "tiny-router-base");
        assert_eq!(listed[0].training_mode, AdapterTrainingMode::Grpo);
        assert_eq!(listed[0].reward_version, "reward-v0.1");
        assert!(listed[0].data_local_only);
        assert_eq!(
            listed[0].chain_commit_hash.as_deref(),
            Some(commit_hash.as_str())
        );
        assert!(path.exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn registry_replaces_existing_adapter_id_and_keeps_sorted_listing() {
        let mut registry = AdapterRegistry::default();
        registry
            .register(metadata("z-adapter", "base-z", AdapterTrainingMode::Dpo))
            .unwrap();
        registry
            .register(metadata("a-adapter", "base-a", AdapterTrainingMode::Sft))
            .unwrap();
        registry
            .register(metadata("z-adapter", "base-new", AdapterTrainingMode::Grpo))
            .unwrap();

        let listed = registry.list().unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].adapter_id, "a-adapter");
        assert_eq!(listed[1].adapter_id, "z-adapter");
        assert_eq!(listed[1].base_model_id, "base-new");
        assert_eq!(listed[1].training_mode, AdapterTrainingMode::Grpo);
    }

    #[test]
    fn registry_rejects_invalid_metadata() {
        let err = AdapterRegistry::default()
            .register(AdapterMetadata {
                adapter_id: "".into(),
                base_model_id: "base".into(),
                training_mode: AdapterTrainingMode::Grpo,
                reward_version: "reward-v0.1".into(),
                data_local_only: true,
                chain_commit_hash: None,
            })
            .unwrap_err();
        assert!(err.to_string().contains("adapter_id"));

        let err = AdapterRegistry::default()
            .register(AdapterMetadata {
                adapter_id: "adapter".into(),
                base_model_id: "base".into(),
                training_mode: AdapterTrainingMode::Grpo,
                reward_version: "reward-v0.1".into(),
                data_local_only: true,
                chain_commit_hash: Some("not-a-hash".into()),
            })
            .unwrap_err();
        assert!(err.to_string().contains("chain_commit_hash"));
    }

    fn metadata(id: &str, base: &str, mode: AdapterTrainingMode) -> AdapterMetadata {
        AdapterMetadata {
            adapter_id: id.into(),
            base_model_id: base.into(),
            training_mode: mode,
            reward_version: "reward-v0.1".into(),
            data_local_only: true,
            chain_commit_hash: Some(hash_bytes(id.as_bytes())),
        }
    }
}
