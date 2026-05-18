//! Verify masterchain `globalStateRoot` + tier-2 Plonky2 without executing blocks.

use fractal_proof_aggregator::{
    dedupe_submissions, validate_proof_submission, verify_global_zk_root, Plonky2ProofBundleV1,
};
use fractal_shard::{global_state_root_from_anchors, MasterchainBlockV1};

use crate::error::LightClientError;
use crate::head::{LightClientHeadV1, VerifiedLightClientHead};

/// Verify `globalStateRoot` recomputation, tier-1 proof ranges, and Plonky2 SNARK binding.
pub fn verify_masterchain_block(
    block: &MasterchainBlockV1,
    plonky2: Option<&Plonky2ProofBundleV1>,
) -> Result<VerifiedLightClientHead, LightClientError> {
    let computed_gsr = global_state_root_from_anchors(&block.shard_anchors);
    if computed_gsr != block.global_state_root {
        return Err(LightClientError::GlobalStateRootMismatch);
    }

    let proofs = dedupe_submissions(&block.validity_proofs)?;
    for sub in &proofs {
        validate_proof_submission(sub, &block.shard_anchors)?;
    }

    match plonky2 {
        Some(bundle) => {
            if bundle.masterchain_height != block.height {
                return Err(LightClientError::MasterchainHeightMismatch {
                    block: block.height,
                    bundle: bundle.masterchain_height,
                });
            }
            if bundle.statement.global_state_root != block.global_state_root
                || bundle.statement.global_zk_root != block.global_zk_root
                || bundle.statement.validity_proofs != proofs
            {
                return Err(LightClientError::Plonky2StatementMismatch);
            }
            bundle.verify()?;
        }
        None => {
            if !proofs.is_empty() {
                return Err(LightClientError::MissingPlonky2Bundle);
            }
            verify_global_zk_root(
                block.height,
                &block.global_state_root,
                &proofs,
                &block.global_zk_root,
                None,
            )?;
        }
    }

    Ok(VerifiedLightClientHead {
        masterchain_height: block.height,
        global_state_root: block.global_state_root,
        global_zk_root: block.global_zk_root,
        shard_anchors: block.shard_anchors.clone(),
    })
}

/// Verify a parsed light-client head (same checks as [`verify_masterchain_block`]).
pub fn verify_light_client_head(
    head: &LightClientHeadV1,
) -> Result<VerifiedLightClientHead, LightClientError> {
    verify_masterchain_block(&head.masterchain, head.plonky2.as_ref())
}
