//! HyperBFT pipelined consensus parameters and parent-QC resolution (`docs/prd.md` §7.9, M10).
//!
//! Track A keeps [`ConsensusMode::HotStuff2`] (500 ms cadence, strict parent QC before propose).
//! Track B shards use [`ConsensusMode::HyperBft`]: 70 ms target cadence and pipelined parent QC
//! via [`HyperBftPipeline`] + [`resolve_parent_qc`] so the leader can propose while votes commit.

use crate::qc::{genesis_parent_qc, QuorumCertificate};
use crate::validators::ValidatorSet;
use crate::vote::{FormedQc, VotePool};
use fractal_crypto::hash::Hash256;

/// Active consensus engine for this process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsensusMode {
    /// Track A — HotStuff-2 monolith (`docs/prd.md` §7.3).
    HotStuff2,
    /// Track B — pipelined HyperBFT-derived shard BFT (`docs/prd.md` §7.9).
    HyperBft,
}

impl ConsensusMode {
    pub const ENV: &'static str = "FRACTAL_CONSENSUS_MODE";

    #[must_use]
    pub fn parse_env(raw: Option<&str>, default_multi_shard_hyperbft: bool) -> Self {
        let Some(s) = raw.map(str::trim).filter(|x| !x.is_empty()) else {
            return if default_multi_shard_hyperbft {
                Self::HyperBft
            } else {
                Self::HotStuff2
            };
        };
        match s.to_ascii_lowercase().as_str() {
            "hyperbft" | "hyper_bft" | "b" | "trackb" | "track_b" => Self::HyperBft,
            "hotstuff2" | "hotstuff" | "hotstuff-2" | "a" | "tracka" | "track_a" => {
                Self::HotStuff2
            }
            _ => {
                eprintln!(
                    "fractal-consensus: unknown {}={s:?}; using {}",
                    Self::ENV,
                    if default_multi_shard_hyperbft {
                        "hyperbft"
                    } else {
                        "hotstuff2"
                    }
                );
                if default_multi_shard_hyperbft {
                    Self::HyperBft
                } else {
                    Self::HotStuff2
                }
            }
        }
    }
}

/// Shard HyperBFT timing (`docs/prd.md` §7.9.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HyperBftConfig {
    /// Target wall-clock cadence between produce attempts (ms).
    pub target_block_time_ms: u64,
    /// Pacemaker base timeout before emitting a local timeout (ms).
    pub pacemaker_base_ms: u64,
    /// Concurrent pipeline stages (propose / vote / commit).
    pub pipeline_depth: u32,
    /// Leader rotation interval in blocks.
    pub leader_epoch_blocks: u64,
}

impl Default for HyperBftConfig {
    fn default() -> Self {
        Self {
            target_block_time_ms: 70,
            pacemaker_base_ms: 70,
            pipeline_depth: 3,
            leader_epoch_blocks: 100,
        }
    }
}

impl HyperBftConfig {
    pub const ENV_TARGET_BLOCK_TIME_MS: &'static str = "FRACTAL_TARGET_BLOCK_TIME_MS";
    pub const ENV_PACEMAKER_BASE_MS: &'static str = "FRACTAL_PACEMAKER_BASE_MS";

    #[must_use]
    pub fn from_env_overrides(mut cfg: Self) -> Self {
        if let Ok(s) = std::env::var(Self::ENV_TARGET_BLOCK_TIME_MS) {
            if let Ok(v) = s.trim().parse::<u64>() {
                if v > 0 {
                    cfg.target_block_time_ms = v;
                    cfg.pacemaker_base_ms = v;
                }
            }
        }
        if let Ok(s) = std::env::var(Self::ENV_PACEMAKER_BASE_MS) {
            if let Ok(v) = s.trim().parse::<u64>() {
                if v > 0 {
                    cfg.pacemaker_base_ms = v;
                }
            }
        }
        cfg
    }
}

/// Parent QC bundle for pipelined propose (certifies block at `certified_height`).
#[derive(Debug, Clone)]
pub struct CertifiedParent {
    pub certified_height: u64,
    pub certified_view: u64,
    pub certified_header_hash: Hash256,
    pub qc: QuorumCertificate,
    pub signer_indices: Vec<u32>,
}

/// In-memory pipeline state (per shard node).
#[derive(Debug, Clone, Default)]
pub struct HyperBftPipeline {
    pub config: HyperBftConfig,
    /// Latest parent QC sealed for pipelined child proposals.
    pub certified_parent: Option<CertifiedParent>,
    /// Blocks produced locally awaiting commit-stage QC seal (heights).
    pub pending_commit_heights: Vec<u64>,
}

impl HyperBftPipeline {
    #[must_use]
    pub fn new(config: HyperBftConfig) -> Self {
        Self {
            config,
            certified_parent: None,
            pending_commit_heights: Vec::new(),
        }
    }

    pub fn note_formed_qc(&mut self, formed: &FormedQc) {
        self.certified_parent = Some(CertifiedParent {
            certified_height: formed.qc.block_height,
            certified_view: formed.qc.view,
            certified_header_hash: formed.qc.block_header_hash,
            qc: formed.qc.clone(),
            signer_indices: formed.signer_indices.clone(),
        });
    }

    pub fn note_block_produced(&mut self, height: u64) {
        self.pending_commit_heights.push(height);
        let max = self.config.pipeline_depth as usize;
        if self.pending_commit_heights.len() > max.saturating_mul(4) {
            let drop = self.pending_commit_heights.len() - max.saturating_mul(4);
            self.pending_commit_heights.drain(0..drop);
        }
    }

    pub fn note_block_committed(&mut self, height: u64) {
        self.pending_commit_heights.retain(|&h| h != height);
    }
}

/// Result of parent QC resolution for block production.
#[derive(Debug, Clone)]
pub enum ParentQcResolution {
    Genesis,
    Formed(FormedQc),
    Pipelined(CertifiedParent),
}

/// Resolve which parent QC to embed in the next block.
///
/// HotStuff-2: only `try_form_qc` on the current tip (strict).
/// HyperBFT: formed QC, then pipelined certified parent matching the tip, then
/// `high_prepare_qc` if it certifies the tip (optimistic responsiveness).
#[must_use]
pub fn resolve_parent_qc(
    mode: ConsensusMode,
    chain_height: u64,
    tip_height: u64,
    tip_view: u64,
    tip_header_hash: Hash256,
    vote_pool: &VotePool,
    validators: &ValidatorSet,
    _stake_weights: Option<&[u128]>,
    pipeline: &HyperBftPipeline,
    high_prepare_qc: &QuorumCertificate,
    try_form_qc: &mut impl FnMut(u64, u64, Hash256) -> Option<FormedQc>,
) -> Option<ParentQcResolution> {
    if chain_height == 0 {
        return Some(ParentQcResolution::Genesis);
    }
    debug_assert_eq!(tip_height, chain_height);

    if let Some(formed) = try_form_qc(tip_view, tip_height, tip_header_hash) {
        return Some(ParentQcResolution::Formed(formed));
    }

    if mode == ConsensusMode::HotStuff2 {
        return None;
    }

    if let Some(ref cert) = pipeline.certified_parent {
        if cert.certified_height == tip_height
            && cert.certified_view == tip_view
            && cert.certified_header_hash == tip_header_hash
        {
            return Some(ParentQcResolution::Pipelined(cert.clone()));
        }
    }

    let need = validators.quorum_threshold();
    if vote_pool.count(tip_view, tip_header_hash) >= need {
        if let Some(formed) = try_form_qc(tip_view, tip_height, tip_header_hash) {
            return Some(ParentQcResolution::Formed(formed));
        }
    }

    if high_prepare_qc.block_height == tip_height
        && high_prepare_qc.view == tip_view
        && high_prepare_qc.block_header_hash == tip_header_hash
    {
        if let Some(ref cert) = pipeline.certified_parent {
            if cert.certified_height == tip_height {
                return Some(ParentQcResolution::Pipelined(cert.clone()));
            }
        }
        return Some(ParentQcResolution::Pipelined(CertifiedParent {
            certified_height: high_prepare_qc.block_height,
            certified_view: high_prepare_qc.view,
            certified_header_hash: high_prepare_qc.block_header_hash,
            qc: high_prepare_qc.clone(),
            signer_indices: Vec::new(),
        }));
    }

    None
}

#[must_use]
pub fn parent_qc_bundle(resolution: ParentQcResolution) -> (QuorumCertificate, Vec<u32>) {
    match resolution {
        ParentQcResolution::Genesis => (genesis_parent_qc(), Vec::new()),
        ParentQcResolution::Formed(f) => (f.qc, f.signer_indices),
        ParentQcResolution::Pipelined(c) => (c.qc, c.signer_indices),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validators::ValidatorSet;
    use crate::vote::{Vote, VoteSignBody};

    #[test]
    fn hyperbft_config_defaults_70ms() {
        let c = HyperBftConfig::default();
        assert_eq!(c.target_block_time_ms, 70);
        assert_eq!(c.pipeline_depth, 3);
    }

    #[test]
    fn mode_parse_env() {
        assert_eq!(
            ConsensusMode::parse_env(Some("hyperbft"), false),
            ConsensusMode::HyperBft
        );
        assert_eq!(
            ConsensusMode::parse_env(None, true),
            ConsensusMode::HyperBft
        );
        assert_eq!(
            ConsensusMode::parse_env(None, false),
            ConsensusMode::HotStuff2
        );
    }

    #[test]
    fn pipelined_parent_from_certified_cache() {
        let validators = ValidatorSet::phase1_singleton();
        let sk = validators.dev_bls_secret(0).expect("sk");
        let tip_hh = [0xab; 32];
        let body = VoteSignBody {
            view: 1,
            height: 1,
            header_hash: tip_hh,
        };
        let v = Vote::sign(body, 0, &sk);
        let mut pool = VotePool::new();
        let w = [0u128];
        pool.record(v, &validators, Some(&w));

        let mut pipeline = HyperBftPipeline::default();
        let formed = pool
            .try_form_qc(1, 1, tip_hh, &validators, Some(&w))
            .expect("qc");
        pipeline.note_formed_qc(&formed);

        let high = formed.qc.clone();
        let res = resolve_parent_qc(
            ConsensusMode::HyperBft,
            1,
            1,
            1,
            tip_hh,
            &pool,
            &validators,
            Some(&w),
            &pipeline,
            &high,
            &mut |view, height, hh| pool.try_form_qc(view, height, hh, &validators, Some(&w)),
        )
        .expect("resolve");

        let (qc, _) = parent_qc_bundle(res);
        assert_eq!(qc.block_header_hash, tip_hh);
    }
}
