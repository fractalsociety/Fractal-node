//! Local persistence helpers for node state that must survive restarts.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use borsh::{BorshDeserialize, BorshSerialize};
use fractal_consensus::BlockValidityProof;
use fractal_crypto::Hash256;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProofFinalityStoreError {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("proof-finality store is corrupt")]
    Decode,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct StoredProofFinalityRecord {
    pub block_hash: Hash256,
    pub height: u64,
    pub accepted_at_ms: u64,
    pub proof: BlockValidityProof,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, Default, PartialEq, Eq)]
struct ProofFinalityStoreFile {
    records: Vec<StoredProofFinalityRecord>,
}

#[derive(Clone, Debug)]
pub struct ProofFinalityStore {
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
