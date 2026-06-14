//! Local persistence helpers for node state that must survive restarts.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use borsh::{BorshDeserialize, BorshSerialize};
use fractal_consensus::{BlockValidityProof, CircuitVersion, MixedExecutionWitnessMetadataV1};
use fractal_crypto::Hash256;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProofFinalityStoreError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("proof-finality store is corrupt")]
    Decode,
}

#[derive(Debug, Error)]
pub enum WitnessMetadataStoreError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("witness metadata store is corrupt")]
    Decode,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct StoredProofFinalityRecord {
    pub block_hash: Hash256,
    pub height: u64,
    pub accepted_at_ms: u64,
    pub circuit_version: CircuitVersion,
    pub coverage_manifest_digest: Hash256,
    pub public_input_digest: Hash256,
    pub proof: BlockValidityProof,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, Default, PartialEq, Eq)]
struct ProofFinalityStoreFile {
    records: Vec<StoredProofFinalityRecord>,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, Default, PartialEq, Eq)]
struct WitnessMetadataStoreFile {
    records: Vec<MixedExecutionWitnessMetadataV1>,
}

#[derive(Clone, Debug)]
pub struct ProofFinalityStore {
    path: PathBuf,
}

#[derive(Clone, Debug)]
pub struct WitnessMetadataStore {
    path: PathBuf,
}

impl ProofFinalityStore {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, ProofFinalityStoreError> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        if !path.exists() {
            let empty = ProofFinalityStoreFile::default();
            fs::write(&path, borsh::to_vec(&empty)?)?;
        }
        Ok(Self { path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load_records(&self) -> Result<Vec<StoredProofFinalityRecord>, ProofFinalityStoreError> {
        let bytes = fs::read(&self.path)?;
        let file = ProofFinalityStoreFile::try_from_slice(&bytes)
            .map_err(|_| ProofFinalityStoreError::Decode)?;
        Ok(file.records)
    }

    pub fn put_record(
        &self,
        record: StoredProofFinalityRecord,
    ) -> Result<(), ProofFinalityStoreError> {
        let mut by_hash = self
            .load_records()?
            .into_iter()
            .map(|r| (r.block_hash, r))
            .collect::<BTreeMap<_, _>>();
        by_hash.insert(record.block_hash, record);
        self.write_records(by_hash.into_values().collect())
    }

    fn write_records(
        &self,
        records: Vec<StoredProofFinalityRecord>,
    ) -> Result<(), ProofFinalityStoreError> {
        let file = ProofFinalityStoreFile { records };
        let bytes = borsh::to_vec(&file)?;
        let tmp = self.path.with_extension("tmp");
        fs::write(&tmp, bytes)?;
        fs::rename(tmp, &self.path)?;
        Ok(())
    }
}

impl WitnessMetadataStore {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, WitnessMetadataStoreError> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        if !path.exists() {
            let empty = WitnessMetadataStoreFile::default();
            fs::write(&path, borsh::to_vec(&empty)?)?;
        }
        Ok(Self { path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load_records(
        &self,
    ) -> Result<Vec<MixedExecutionWitnessMetadataV1>, WitnessMetadataStoreError> {
        let bytes = fs::read(&self.path)?;
        let file = WitnessMetadataStoreFile::try_from_slice(&bytes)
            .map_err(|_| WitnessMetadataStoreError::Decode)?;
        Ok(file.records)
    }

    pub fn put_record(
        &self,
        record: MixedExecutionWitnessMetadataV1,
    ) -> Result<(), WitnessMetadataStoreError> {
        let mut by_hash = self
            .load_records()?
            .into_iter()
            .map(|r| (r.block_hash, r))
            .collect::<BTreeMap<_, _>>();
        by_hash.insert(record.block_hash, record);
        self.write_records(by_hash.into_values().collect())
    }

    fn write_records(
        &self,
        records: Vec<MixedExecutionWitnessMetadataV1>,
    ) -> Result<(), WitnessMetadataStoreError> {
        let file = WitnessMetadataStoreFile { records };
        let bytes = borsh::to_vec(&file)?;
        let tmp = self.path.with_extension("tmp");
        fs::write(&tmp, bytes)?;
        fs::rename(tmp, &self.path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fractal_consensus::{
        CircuitVersion, ExecutionFeatureSetV1, WitnessRetentionPolicyV1, MIXED_EXECUTION_WITNESS_V1,
    };

    #[test]
    fn witness_metadata_store_round_trips_and_replaces_by_block_hash() {
        let path = std::env::temp_dir().join(format!(
            "fractal-witness-metadata-{}-{}.borsh",
            std::process::id(),
            1
        ));
        let _ = std::fs::remove_file(&path);
        let store = WitnessMetadataStore::open(&path).expect("open");
        let mut record = MixedExecutionWitnessMetadataV1 {
            version: MIXED_EXECUTION_WITNESS_V1,
            block_hash: [1u8; 32],
            height: 7,
            witness_digest: [2u8; 32],
            public_input_digest: [3u8; 32],
            circuit_version: CircuitVersion::DevMixedV1,
            coverage_manifest_digest: [4u8; 32],
            feature_set: ExecutionFeatureSetV1 { bits: 5 },
            retention_policy: WitnessRetentionPolicyV1::MetadataOnly,
        };

        store.put_record(record.clone()).expect("put");
        assert_eq!(store.load_records().expect("load"), vec![record.clone()]);

        record.witness_digest = [9u8; 32];
        store.put_record(record.clone()).expect("replace");
        assert_eq!(store.load_records().expect("load"), vec![record]);
        let _ = std::fs::remove_file(&path);
    }
}
