//! Persisted checkpoint proof records: in-memory registry + optional **RocksDB** and/or legacy
//! **filesystem** sidecar (one `borsh` file per block height). Used by `fractal-node` and surfaced
//! over JSON-RPC (`fractal_*`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use borsh::{BorshDeserialize, BorshSerialize};
use fractal_storage::RocksCheckpointProofStore;

use crate::artifact::{CheckpointStwoArtifactV1, CHECKPOINT_STWO_ARTIFACT_VERSION};
use crate::CheckpointJob;

pub const PERSISTED_CHECKPOINT_PROOF_VERSION: u8 = 1;

/// Stored outcome of async checkpoint proving (STWO artifact or stub digest only).
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PersistedCheckpointProofV1 {
    pub version: u8,
    pub start_block: u64,
    pub end_block: u64,
    pub height: u64,
    pub chain_id: u64,
    pub header_hash: [u8; 32],
    pub proof_digest: [u8; 32],
    pub stub_fallback: bool,
    /// `Some(borsh(CheckpointStwoArtifactV1))` when STWO prove succeeded.
    pub artifact_v1_borsh: Option<Vec<u8>>,
}

impl PersistedCheckpointProofV1 {
    /// JSON-RPC-friendly view (`fractal_getCheckpointProof`).
    #[must_use]
    pub fn to_rpc_json(&self) -> serde_json::Value {
        serde_json::json!({
            "height": format!("0x{:x}", self.height),
            "startBlock": format!("0x{:x}", self.start_block),
            "endBlock": format!("0x{:x}", self.end_block),
            "chainId": format!("0x{:x}", self.chain_id),
            "headerHash": format!("0x{}", hex::encode(self.header_hash)),
            "proofDigest": format!("0x{}", hex::encode(self.proof_digest)),
            "stubFallback": self.stub_fallback,
            "artifactVersion": self.artifact_v1_borsh.as_ref().map(|_| CHECKPOINT_STWO_ARTIFACT_VERSION),
            "artifactHex": self.artifact_v1_borsh.as_ref().map(|b| format!("0x{}", hex::encode(b))),
        })
    }
}

/// Build a persisted record: full STWO [`CheckpointStwoArtifactV1`] when prove succeeds, else stub digest only.
#[must_use]
pub fn build_persisted_checkpoint_proof(job: &CheckpointJob) -> PersistedCheckpointProofV1 {
    match CheckpointStwoArtifactV1::prove(job) {
        Ok((art, digest)) => match art.to_bytes() {
            Ok(bytes) => PersistedCheckpointProofV1 {
                version: PERSISTED_CHECKPOINT_PROOF_VERSION,
                start_block: job.start_block,
                end_block: job.end_block,
                height: job.height,
                chain_id: job.chain_id,
                header_hash: job.header_hash,
                proof_digest: digest,
                stub_fallback: false,
                artifact_v1_borsh: Some(bytes),
            },
            Err(e) => {
                eprintln!(
                    "fractal-proof-condenser: borsh encode artifact failed height={}: {e}; digest still STWO",
                    job.height
                );
                PersistedCheckpointProofV1 {
                    version: PERSISTED_CHECKPOINT_PROOF_VERSION,
                    start_block: job.start_block,
                    end_block: job.end_block,
                    height: job.height,
                    chain_id: job.chain_id,
                    header_hash: job.header_hash,
                    proof_digest: digest,
                    stub_fallback: false,
                    artifact_v1_borsh: None,
                }
            }
        },
        Err(e) => {
            eprintln!(
                "fractal-proof-condenser: STWO artifact prove failed height={}: {e}; using blake3 stub digest only",
                job.height
            );
            PersistedCheckpointProofV1 {
                version: PERSISTED_CHECKPOINT_PROOF_VERSION,
                start_block: job.start_block,
                end_block: job.end_block,
                height: job.height,
                chain_id: job.chain_id,
                header_hash: job.header_hash,
                proof_digest: job.stwo_commitment_stub(),
                stub_fallback: true,
                artifact_v1_borsh: None,
            }
        }
    }
}

fn sidecar_path(dir: &Path, height: u64) -> PathBuf {
    dir.join(format!("{height:016}.proof.borsh"))
}

/// Where to persist proofs in addition to the in-memory cache.
#[derive(Clone, Debug, Default)]
pub struct ProofPersistenceConfig {
    /// Legacy flat files: `{height:016}.proof.borsh` (`FRACTAL_PROOF_ARTIFACT_DIR`).
    pub filesystem_dir: Option<PathBuf>,
    /// RocksDB directory (`FRACTAL_PROOF_ROCKSDB_PATH`). Ignored when `shared_rocksdb` is set.
    pub rocksdb_path: Option<PathBuf>,
    /// Open handle shared with `fractal-node` (same directory as `FRACTAL_CHAIN_ROCKSDB_PATH`).
    pub shared_rocksdb: Option<fractal_storage::FractalRocksDb>,
    /// Execution shard for RocksDB key namespacing (M10).
    pub shard_id: u32,
    pub shard_count: u32,
}

/// In-memory index + optional RocksDB and/or filesystem sidecars.
#[derive(Debug)]
pub struct ProofArtifactRegistry {
    entries: Mutex<BTreeMap<u64, PersistedCheckpointProofV1>>,
    filesystem_dir: Option<PathBuf>,
    rocksdb: Option<RocksCheckpointProofStore>,
    shard_id: u32,
    shard_count: u32,
}

impl ProofArtifactRegistry {
    #[must_use]
    pub fn new(config: ProofPersistenceConfig) -> Self {
        let rocksdb = if let Some(db) = config.shared_rocksdb {
            Some(db)
        } else {
            config.rocksdb_path.as_ref().and_then(|p| {
                match RocksCheckpointProofStore::open(p) {
                    Ok(db) => Some(db),
                    Err(e) => {
                        eprintln!(
                            "fractal-proof-condenser: open RocksDB checkpoint store {:?} failed: {e}",
                            p
                        );
                        None
                    }
                }
            })
        };
        Self {
            entries: Mutex::new(BTreeMap::new()),
            filesystem_dir: config.filesystem_dir,
            rocksdb,
            shard_id: config.shard_id,
            shard_count: config.shard_count.max(1),
        }
    }

    fn persist_durable(&self, entry: &PersistedCheckpointProofV1) {
        let height = entry.height;
        let Ok(bytes) = borsh::to_vec(entry) else {
            eprintln!(
                "fractal-proof-condenser: borsh encode PersistedCheckpointProofV1 height={height} failed"
            );
            return;
        };
        if let Some(ref db) = self.rocksdb {
            if let Err(e) = db.put_proof_blob(self.shard_id, self.shard_count, height, &bytes) {
                eprintln!(
                    "fractal-proof-condenser: RocksDB put checkpoint proof height={height} failed: {e}"
                );
            }
        }
        if let Some(dir) = &self.filesystem_dir {
            if let Err(e) = std::fs::create_dir_all(dir) {
                eprintln!(
                    "fractal-proof-condenser: create_dir_all {:?} failed: {e}",
                    dir
                );
            } else {
                let path = sidecar_path(dir, height);
                if let Err(e) = std::fs::write(&path, &bytes) {
                    eprintln!(
                        "fractal-proof-condenser: write proof sidecar {:?} failed: {e}",
                        path
                    );
                }
            }
        }
    }

    /// Insert into memory and durable store(s) when configured.
    pub fn record(&self, entry: PersistedCheckpointProofV1) {
        self.persist_durable(&entry);
        let mut g = self.entries.lock().expect("proof registry mutex poisoned");
        g.insert(entry.height, entry);
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.lock().map(|g| g.len()).unwrap_or_default()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[must_use]
    pub fn get(&self, height: u64) -> Option<PersistedCheckpointProofV1> {
        {
            let g = self.entries.lock().expect("proof registry mutex poisoned");
            if let Some(e) = g.get(&height) {
                return Some(e.clone());
            }
        }
        if let Some(ref db) = self.rocksdb {
            if let Ok(Some(bytes)) = db.get_proof_blob(self.shard_id, self.shard_count, height) {
                if let Ok(entry) = borsh::from_slice::<PersistedCheckpointProofV1>(&bytes) {
                    let mut g = self.entries.lock().expect("proof registry mutex poisoned");
                    g.insert(height, entry.clone());
                    return Some(entry);
                }
            }
        }
        let dir = self.filesystem_dir.as_ref()?;
        let path = sidecar_path(dir, height);
        let bytes = std::fs::read(&path).ok()?;
        let entry: PersistedCheckpointProofV1 = borsh::from_slice(&bytes).ok()?;
        let mut g = self.entries.lock().expect("proof registry mutex poisoned");
        g.insert(height, entry.clone());
        Some(entry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkpoint_job_from_block;
    use fractal_consensus::{genesis_parent_qc, Block, BlockHeader};

    fn job_at_height(height: u64) -> CheckpointJob {
        let block = Block {
            header: BlockHeader {
                version: 1,
                chain_id: 41,
                height,
                view: 0,
                parent_hash: [1u8; 32],
                parent_qc_hash: [2u8; 32],
                proposer: [3u8; 32],
                timestamp_ms: 0,
                parent_state_root: [0u8; 32],
                state_root: [4u8; 32],
                tx_root: [0u8; 32],
                receipt_root: [0u8; 32],
                native_event_root: [0u8; 32],
                evm_log_root: [0u8; 32],
                gas_used: 21_000,
                gas_limit: 30_000_000,
                shard_id: 0,
                extra: [6u8; 32],
            },
            transactions: vec![],
            parent_qc: genesis_parent_qc(),
            parent_qc_signer_indices: vec![],
            eth_signed_raw: vec![],
        };
        checkpoint_job_from_block(41, &block).expect("job")
    }

    #[test]
    fn registry_memory_round_trip() {
        let reg = ProofArtifactRegistry::new(ProofPersistenceConfig::default());
        let job = job_at_height(3);
        let p = build_persisted_checkpoint_proof(&job);
        reg.record(p.clone());
        assert_eq!(reg.get(3), Some(p));
        assert_eq!(reg.get(99), None);
    }

    #[test]
    fn registry_sidecar_reload() {
        let dir =
            std::env::temp_dir().join(format!("fractal_proof_sidecar_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let job = job_at_height(5);
        let p = build_persisted_checkpoint_proof(&job);
        {
            let reg = ProofArtifactRegistry::new(ProofPersistenceConfig {
                filesystem_dir: Some(dir.clone()),
                ..Default::default()
            });
            reg.record(p.clone());
        }
        let reg2 = ProofArtifactRegistry::new(ProofPersistenceConfig {
            filesystem_dir: Some(dir.clone()),
            ..Default::default()
        });
        let got = reg2.get(5).expect("read from sidecar");
        assert_eq!(got.proof_digest, p.proof_digest);
        assert_eq!(got.stub_fallback, p.stub_fallback);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn registry_rocksdb_reload() {
        let dir = std::env::temp_dir().join(format!("fractal_proof_rocks_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let job = job_at_height(11);
        let p = build_persisted_checkpoint_proof(&job);
        {
            let reg = ProofArtifactRegistry::new(ProofPersistenceConfig {
                rocksdb_path: Some(dir.clone()),
                ..Default::default()
            });
            reg.record(p.clone());
        }
        let reg2 = ProofArtifactRegistry::new(ProofPersistenceConfig {
            rocksdb_path: Some(dir.clone()),
            ..Default::default()
        });
        let got = reg2.get(11).expect("read from rocks");
        assert_eq!(got.proof_digest, p.proof_digest);
        assert_eq!(got.stub_fallback, p.stub_fallback);
        drop(reg2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
