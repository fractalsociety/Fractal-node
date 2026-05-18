//! Three-stage HyperBFT pipeline: propose / vote / commit (`docs/prd.md` §7.9.3).

use fractal_crypto::hash::Hash256;

use crate::hyperbft::HyperBftConfig;
use crate::Block;
use crate::vote::FormedQc;

/// One in-flight block in the pipeline.
#[derive(Debug, Clone)]
pub struct PipelineSlot {
    pub block: Block,
    pub header_hash: Hash256,
    pub formed_qc: Option<FormedQc>,
}

impl PipelineSlot {
    #[must_use]
    pub fn new(block: Block, header_hash: Hash256) -> Self {
        Self {
            block,
            header_hash,
            formed_qc: None,
        }
    }
}

/// Concurrent stages (heights relative to committed chain tip).
#[derive(Debug, Clone, Default)]
pub struct ThreeStagePipeline {
    pub config: HyperBftConfig,
    /// Commit stage — block ready to apply once QC is verified.
    pub commit: Option<PipelineSlot>,
    /// Vote stage — collecting votes / forming QC.
    pub vote: Option<PipelineSlot>,
    /// Propose stage — reserved for in-flight leader work (optional).
    pub propose: Option<PipelineSlot>,
}

impl ThreeStagePipeline {
    #[must_use]
    pub fn new(config: HyperBftConfig) -> Self {
        Self {
            config,
            commit: None,
            vote: None,
            propose: None,
        }
    }

    pub fn clear(&mut self) {
        self.commit = None;
        self.vote = None;
        self.propose = None;
    }

    pub fn clear_from_height(&mut self, height: u64) {
        let drop = |slot: &Option<PipelineSlot>| {
            slot.as_ref()
                .map(|s| s.block.header.height >= height)
                .unwrap_or(false)
        };
        if drop(&self.commit) {
            self.commit = None;
        }
        if drop(&self.vote) {
            self.vote = None;
        }
        if drop(&self.propose) {
            self.propose = None;
        }
    }

    /// Stage 2 → 1: form QC on vote slot; promote to commit when ready.
    pub fn advance_vote<F>(&mut self, mut try_form_qc: F)
    where
        F: FnMut(u64, u64, Hash256) -> Option<FormedQc>,
    {
        let Some(slot) = self.vote.as_mut() else {
            return;
        };
        if slot.formed_qc.is_none() {
            slot.formed_qc = try_form_qc(
                slot.block.header.view,
                slot.block.header.height,
                slot.header_hash,
            );
        }
        if slot.formed_qc.is_some() && self.commit.is_none() {
            self.commit = self.vote.take();
        }
    }

    /// Take commit slot block for on-chain application (stage 1).
    pub fn take_commit_block(&mut self) -> Option<Block> {
        self.commit.take().map(|s| s.block)
    }

    /// Stage 3: enqueue newly proposed block into vote stage (shift pipeline).
    pub fn enqueue_proposed(&mut self, block: Block, header_hash: Hash256) {
        let h = block.header.height;
        if self
            .commit
            .as_ref()
            .is_some_and(|s| s.block.header.height >= h)
        {
            return;
        }
        if let Some(p) = self.propose.take() {
            if self.vote.is_none() {
                self.vote = Some(p);
            }
        }
        if let Some(v) = self.vote.take() {
            if self.commit.is_none() {
                self.commit = Some(v);
            }
        }
        self.vote = Some(PipelineSlot::new(block, header_hash));
    }

    #[must_use]
    pub fn in_flight_count(&self) -> usize {
        self.commit.is_some() as usize + self.vote.is_some() as usize + self.propose.is_some() as usize
    }
}

/// Result of one three-stage tick (all stages attempted).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreeStageTickSummary {
    Idle,
    Committed(u64),
    Proposed(u64),
    RolledBack,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genesis_parent_qc;
    use crate::validators::ValidatorSet;
    use crate::vote::{Vote, VoteSignBody};
    fn empty_block(height: u64, view: u64, hh: Hash256) -> Block {
        Block {
            header: crate::BlockHeader {
                version: 1,
                chain_id: 41,
                height,
                view,
                parent_hash: [0u8; 32],
                parent_qc_hash: [0u8; 32],
                proposer: [0u8; 32],
                timestamp_ms: 0,
                state_root: [0u8; 32],
                tx_root: [0u8; 32],
                gas_used: 0,
                gas_limit: 30_000_000,
                shard_id: 0,
                extra: [0u8; 32],
            },
            transactions: vec![],
            eth_signed_raw: vec![],
            parent_qc: genesis_parent_qc(),
            parent_qc_signer_indices: vec![],
        }
    }

    #[test]
    fn vote_promotes_to_commit_when_qc_forms() {
        let validators = ValidatorSet::phase1_singleton();
        let sk = validators.dev_bls_secret(0).expect("sk");
        let hh = [0xcd; 32];
        let body = VoteSignBody {
            view: 1,
            height: 1,
            header_hash: hh,
        };
        let v = Vote::sign(body, 0, &sk);
        let mut pool = crate::vote::VotePool::new();
        let w = [0u128];
        pool.record(v, &validators, Some(&w));

        let mut pipe = ThreeStagePipeline::default();
        pipe.vote = Some(PipelineSlot::new(empty_block(1, 1, hh), hh));
        pipe.advance_vote(|view, height, hash| {
            pool.try_form_qc(view, height, hash, &validators, Some(&w))
        });
        assert!(pipe.commit.is_some());
        assert!(pipe.vote.is_none());
    }
}
