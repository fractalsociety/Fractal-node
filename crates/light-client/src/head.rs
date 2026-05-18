//! Verified light-client head types.

use fractal_proof_aggregator::Plonky2ProofBundleV1;
use fractal_shard::{MasterchainBlockV1, ShardAnchor};

/// Snapshot returned by `fractal_getLightClientHead` (parsed subset).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LightClientHeadV1 {
    pub masterchain: MasterchainBlockV1,
    pub plonky2: Option<Plonky2ProofBundleV1>,
    pub execution_shard_id: Option<u32>,
    pub execution_tip_height: Option<u64>,
    pub execution_tip_state_root: Option<[u8; 32]>,
}

/// Masterchain head after Plonky2 + `globalStateRoot` checks (PRD §7.10).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedLightClientHead {
    pub masterchain_height: u64,
    pub global_state_root: [u8; 32],
    pub global_zk_root: [u8; 32],
    pub shard_anchors: Vec<ShardAnchor>,
}

impl VerifiedLightClientHead {
    #[must_use]
    pub fn shard_anchor(&self, shard_id: u32) -> Option<&ShardAnchor> {
        self.shard_anchors
            .iter()
            .find(|a| a.shard_id == shard_id)
    }

    #[must_use]
    pub fn shard_state_root(&self, shard_id: u32) -> Option<[u8; 32]> {
        self.shard_anchor(shard_id).map(|a| a.state_root)
    }
}
