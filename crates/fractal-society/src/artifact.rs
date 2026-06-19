//! Artifact management and content-addressed storage
//!
//! Provides types for immutable, versioned artifacts with:
//! - Stable IDs and content-based hashing
//! - Visibility controls
//! - Package digests and signatures
//! - Export/import support

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::protocol::{Hash, Visibility, Version};

/// Unique artifact identifier
pub type ArtifactId = String;

/// Content hash (alias to protocol Hash)
pub type ArtifactHash = Hash;

/// Artifact manifest - describes an immutable artifact
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactManifest {
    /// Artifact ID
    pub id: ArtifactId,
    /// Artifact version
    pub version: Version,
    /// Artifact type
    pub artifact_type: ArtifactType,
    /// Content hash
    pub content_hash: ArtifactHash,
    /// Size in bytes
    pub size_bytes: u64,
    /// Author/owner
    pub author: String,
    /// Visibility level
    pub visibility: Visibility,
    /// License
    pub license: String,
    /// Dependencies (artifact IDs and versions)
    pub dependencies: HashMap<String, Version>,
    /// Metadata
    pub metadata: serde_json::Value,
    /// Created at
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Signature
    pub signature: Option<String>,
}

/// Artifact type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ArtifactType {
    /// Agent package
    AgentPackage,
    /// Skill package
    SkillPackage,
    /// Verifier package
    VerifierPackage,
    /// Dataset
    Dataset,
    /// Environment
    Environment,
    /// Protocol
    Protocol,
    /// Evidence bundle
    EvidenceBundle,
    /// Proof manifest
    ProofManifest,
    /// Review
    Review,
    /// Replication
    Replication,
}

/// Artifact registry entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryEntry {
    /// Artifact manifest
    pub manifest: ArtifactManifest,
    /// Storage location
    pub storage_location: StorageLocation,
    /// Download count
    pub download_count: u64,
    /// Verified status
    pub verified: bool,
}

/// Storage location
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StorageLocation {
    /// Local filesystem
    Local { path: String },
    /// S3-compatible object storage
    S3 { bucket: String, key: String },
    /// IPFS
    Ipfs { cid: String },
    /// URL
    Url { url: String },
}

/// Package digest - signed hash of an artifact
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageDigest {
    /// Package ID
    pub package_id: ArtifactId,
    /// Package version
    pub version: Version,
    /// Content hash
    pub content_hash: ArtifactHash,
    /// Manifest hash
    pub manifest_hash: ArtifactHash,
    /// Signatures
    pub signatures: Vec<Signature>,
}

/// Signature on an artifact
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signature {
    /// Signer identity
    pub signer: String,
    /// Signature value
    pub signature: String,
    /// Signature algorithm
    pub algorithm: String,
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Artifact export bundle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportBundle {
    /// Bundle manifest
    pub manifest: ExportManifest,
    /// Artifacts
    pub artifacts: HashMap<ArtifactId, ArtifactData>,
}

/// Export manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportManifest {
    /// Export version
    pub export_version: Version,
    /// Export ID
    pub export_id: ArtifactId,
    /// Exported at
    pub exported_at: chrono::DateTime<chrono::Utc>,
    /// Artifact IDs included
    pub artifact_ids: Vec<ArtifactId>,
    /// Required exports
    pub required_artifacts: Vec<ArtifactId>,
}

/// Artifact data (can be inline or reference)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ArtifactData {
    /// Inline data (for small artifacts)
    Inline { data: String, encoding: String },
    /// Reference to storage location
    Reference { location: StorageLocation },
}

/// Immutable version marker
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImmutableVersion {
    /// Version string
    pub version: Version,
    /// Git commit hash (if applicable)
    pub commit_hash: Option<String>,
    /// Build timestamp
    pub build_timestamp: chrono::DateTime<chrono::Utc>,
    /// Changelog entry
    pub changelog: String,
    /// Migration metadata
    pub migration_info: Option<MigrationInfo>,
}

/// Migration information for version upgrades
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationInfo {
    /// Previous version
    pub from_version: Version,
    /// Migration script reference
    pub migration_script: String,
    /// Breaking changes
    pub breaking_changes: Vec<String>,
    /// Manual intervention required
    pub manual_intervention: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_artifact_manifest_creation() {
        let manifest = ArtifactManifest {
            id: "test-artifact".to_string(),
            version: "1.0.0".to_string(),
            artifact_type: ArtifactType::AgentPackage,
            content_hash: Hash::new(b"test"),
            size_bytes: 1024,
            author: "test-author".to_string(),
            visibility: Visibility::Private,
            license: "MIT".to_string(),
            dependencies: HashMap::new(),
            metadata: serde_json::json!({}),
            created_at: chrono::Utc::now(),
            signature: None,
        };

        assert_eq!(manifest.artifact_type, ArtifactType::AgentPackage);
        assert_eq!(manifest.visibility, Visibility::Private);
    }
}
