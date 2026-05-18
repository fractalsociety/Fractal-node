//! Tier-2 **Plonky2** aggregation for the masterchain (`docs/prd.md` §6.2.2, §7.8, M11).
//!
//! Shard nodes accept tier-1 [`ProofSubmissionV1`] plus optional verified STWO artifact statements.
//! On each masterchain seal, the **Plonky2** prover hashes the canonical statement with Poseidon
//! and emits a recursive SNARK; [`global_zk_root`] is the four-limb digest committed in the
//! masterchain block.

mod aggregate;
mod plonky2_agg;
mod statement;
mod stwo;
mod submission;

pub use aggregate::{
    AggregatedZkProofV1, AggregatorError, GlobalZkStatementV1, PLONKY2_AGGREGATOR_VERSION,
    aggregate_global_zk_root, prove_and_aggregate, prove_and_aggregate_verified,
    verify_global_zk_root, verify_global_zk_root_verified,
};
pub use plonky2_agg::{
    hash_out_to_global_zk_root, poseidon_statement_digest, poseidon_verified_statement_digest,
    prove_global_aggregation, prove_global_aggregation_verified, verify_global_aggregation_snark,
    verify_global_aggregation_verified_snark,
};
pub use statement::{
    MAX_AGG_PROOFS, VerifiedStwoStatementV1, encode_statement_u64, encode_verified_statement_u64,
    hash256_to_u64_limbs, statement_field_len, verified_statement_field_len,
};
pub use stwo::verify_stwo_artifact_submission;
pub use submission::{
    SubmissionError, dedupe_submissions, proof_submission_from_checkpoint_digest,
    validate_proof_submission,
};

/// Serialized tier-2 bundle (proof bytes + public inputs) for off-chain storage / bridges.
#[derive(Debug, Clone, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct Plonky2ProofBundleV1 {
    pub version: u8,
    pub masterchain_height: u64,
    pub statement: GlobalZkStatementV1,
    pub snark_bytes: Vec<u8>,
}

impl Plonky2ProofBundleV1 {
    #[must_use]
    pub fn from_aggregated(
        masterchain_height: u64,
        statement: GlobalZkStatementV1,
        aggregated: &AggregatedZkProofV1,
    ) -> Self {
        Self {
            version: PLONKY2_AGGREGATOR_VERSION,
            masterchain_height,
            statement,
            snark_bytes: aggregated.snark_bytes.clone(),
        }
    }

    /// Verify Plonky2 SNARK + statement binding.
    pub fn verify(&self) -> Result<(), AggregatorError> {
        if self.version != PLONKY2_AGGREGATOR_VERSION {
            return Err(AggregatorError::UnsupportedVersion(self.version));
        }
        if !self.statement.verified_stwo_statements.is_empty() {
            return verify_global_zk_root_verified(
                self.masterchain_height,
                &self.statement.global_state_root,
                &self.statement.validity_proofs,
                &self.statement.verified_stwo_statements,
                &self.statement.global_zk_root,
                Some(&self.snark_bytes),
            );
        }
        verify_global_zk_root(
            self.masterchain_height,
            &self.statement.global_state_root,
            &self.statement.validity_proofs,
            &self.statement.global_zk_root,
            Some(&self.snark_bytes),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fractal_consensus::{Block, BlockHeader, genesis_parent_qc};
    use fractal_proof_condenser::{CheckpointStwoArtifactV1, checkpoint_job_from_block};
    use fractal_shard::{ProofSubmissionV1, ShardAnchor};

    fn anchor(shard: u32, h: u64) -> ShardAnchor {
        ShardAnchor {
            shard_id: shard,
            block_height: h,
            state_root: [shard as u8; 32],
            witness_commitment: [h as u8; 32],
        }
    }

    fn sub(shard: u32, start: u64, end: u64, digest: [u8; 32]) -> ProofSubmissionV1 {
        ProofSubmissionV1 {
            shard_id: shard,
            start_block: start,
            end_block: end,
            prover: [0xab; 20],
            lag_seconds: 1,
            proof_digest: digest,
        }
    }

    fn block(height: u64) -> Block {
        Block {
            header: BlockHeader {
                version: 1,
                chain_id: 41,
                height,
                view: 0,
                parent_hash: [1u8; 32],
                parent_qc_hash: [2u8; 32],
                proposer: [3u8; 32],
                timestamp_ms: 0,
                state_root: [4u8; 32],
                tx_root: [5u8; 32],
                gas_used: 21_000,
                gas_limit: 30_000_000,
                shard_id: 0,
                extra: [6u8; 32],
            },
            transactions: vec![],
            parent_qc: genesis_parent_qc(),
            parent_qc_signer_indices: vec![],
            eth_signed_raw: vec![],
        }
    }

    #[test]
    fn bundle_prove_verify_round_trip() {
        let anchors = vec![anchor(0, 100), anchor(1, 200)];
        let proofs = vec![sub(0, 1, 100, [1u8; 32]), sub(1, 1, 200, [2u8; 32])];
        for p in &proofs {
            validate_proof_submission(p, &anchors).expect("valid");
        }
        let gsr = fractal_shard::global_state_root_from_anchors(&anchors);
        let aggregated = prove_and_aggregate(3, &gsr, &proofs).expect("aggregate");
        assert_ne!(aggregated.global_zk_root, [0u8; 32]);
        assert!(!aggregated.snark_bytes.is_empty());
        let bundle = Plonky2ProofBundleV1::from_aggregated(
            3,
            GlobalZkStatementV1 {
                global_state_root: gsr,
                global_zk_root: aggregated.global_zk_root,
                validity_proofs: proofs.clone(),
                verified_stwo_statements: vec![],
            },
            &aggregated,
        );
        bundle.verify().expect("bundle");
    }

    #[test]
    fn verified_stwo_artifact_builds_plonky2_statement() {
        let job = checkpoint_job_from_block(41, &block(7)).expect("job");
        let Ok((artifact, digest)) = CheckpointStwoArtifactV1::prove(&job) else {
            return;
        };
        let bytes = artifact.to_bytes().expect("artifact borsh");
        let sub = proof_submission_from_checkpoint_digest(
            0,
            job.start_block,
            job.end_block,
            [0xab; 20],
            digest,
            0,
        );
        let stmt = verify_stwo_artifact_submission(&sub, &bytes).expect("verified statement");
        assert_eq!(stmt.chain_id, job.chain_id);
        assert_eq!(stmt.header_hash, job.header_hash);
        assert!(stmt.matches_submission(&sub));
    }
}
