//! Versioned **checkpoint + STWO proof** bundle (PRD §7.8 / M9 wire-format sketch).
//!
//! Operators store `borsh(CheckpointStwoArtifactV1)` alongside the published digest; verifiers
//! deserialize, check digest, and run [`crate::checkpoint_stwo::verify_checkpoint_stwo_proof_json`].

use borsh::{BorshDeserialize, BorshSerialize};

use crate::checkpoint_stwo::{
    checkpoint_stwo_digest_from_json, prove_checkpoint_stwo, verify_checkpoint_stwo_proof_json,
    StwoCheckpointError,
};
use crate::CheckpointJob;

pub const CHECKPOINT_STWO_ARTIFACT_VERSION: u8 = 1;

/// Serialized checkpoint job + STWO proof JSON (`serde_json` of [`stwo::core::proof::StarkProof`]).
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CheckpointStwoArtifactV1 {
    pub version: u8,
    pub job: CheckpointJob,
    pub stark_proof_json: Vec<u8>,
}

impl CheckpointStwoArtifactV1 {
    /// Build from a successful prove; `expected_digest` should match the prover’s
    /// [`crate::checkpoint_stwo::checkpoint_stwo_digest_from_json`] output.
    #[must_use]
    pub fn new(job: CheckpointJob, stark_proof_json: Vec<u8>) -> Self {
        Self {
            version: CHECKPOINT_STWO_ARTIFACT_VERSION,
            job,
            stark_proof_json,
        }
    }

    /// Prove, self-verify, and wrap as v1 artifact + digest.
    pub fn prove(job: &CheckpointJob) -> Result<(Self, [u8; 32]), StwoCheckpointError> {
        let (json, digest) = prove_checkpoint_stwo(job)?;
        Ok((Self::new(job.clone(), json), digest))
    }

    #[must_use]
    pub fn digest(&self) -> [u8; 32] {
        checkpoint_stwo_digest_from_json(&self.stark_proof_json)
    }

    /// Check `version`, digest equality, and STWO verification.
    pub fn verify(&self, expected_digest: &[u8; 32]) -> Result<(), StwoCheckpointError> {
        if self.version != CHECKPOINT_STWO_ARTIFACT_VERSION {
            return Err(StwoCheckpointError::UnsupportedArtifactVersion(
                self.version,
            ));
        }
        if &self.digest() != expected_digest {
            return Err(StwoCheckpointError::DigestMismatch);
        }
        verify_checkpoint_stwo_proof_json(&self.stark_proof_json, &self.job)
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, std::io::Error> {
        borsh::to_vec(self)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, std::io::Error> {
        borsh::from_slice(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkpoint_job_from_block;
    use fractal_consensus::{genesis_parent_qc, Block, BlockHeader};

    fn job() -> CheckpointJob {
        let block = Block {
            header: BlockHeader {
                version: 1,
                chain_id: 41,
                height: 7,
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
    fn artifact_borsh_round_trip_and_verify() {
        let j = job();
        let Ok((art, digest)) = CheckpointStwoArtifactV1::prove(&j) else {
            return;
        };
        art.verify(&digest).expect("verify");
        let bytes = art.to_bytes().expect("borsh");
        let art2 = CheckpointStwoArtifactV1::from_bytes(&bytes).expect("decode");
        assert_eq!(art, art2);
        art2.verify(&digest).expect("verify decoded");
    }

    #[test]
    fn artifact_rejects_wrong_digest() {
        let j = job();
        let Ok((art, digest)) = CheckpointStwoArtifactV1::prove(&j) else {
            return;
        };
        let mut bad = digest;
        bad[0] ^= 0xff;
        assert!(matches!(
            art.verify(&bad),
            Err(StwoCheckpointError::DigestMismatch)
        ));
    }

    #[test]
    fn artifact_rejects_swapped_job_same_proof_json() {
        let j1 = job();
        let mut j2 = j1.clone();
        j2.height = j1.height.wrapping_add(1);
        let Ok((mut art, digest)) = CheckpointStwoArtifactV1::prove(&j1) else {
            return;
        };
        art.job = j2;
        assert!(matches!(
            art.verify(&digest),
            Err(StwoCheckpointError::Verify(_))
        ));
    }
}
