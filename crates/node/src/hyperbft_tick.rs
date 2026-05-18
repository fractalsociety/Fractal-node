//! HyperBFT three-stage tick: commit → vote → propose (`docs/prd.md` §7.9.3).

use crate::{NodeInner, ProduceTickOutcome};
use fractal_consensus::{
    BlockFinalizeContext, ConsensusMode, ParentQcResolution, execute_and_build_block, header_hash,
    parent_qc_bundle, resolve_parent_qc,
};
use fractal_core::Transaction;

impl NodeInner {
    /// One 70 ms tick runs all three pipeline stages (`docs/prd.md` §7.9.3).
    pub fn hyperbft_three_stage_tick(&mut self) -> ProduceTickOutcome {
        if crate::dev_inject_quorum_from_env() {
            self.inject_quorum_votes_for_pipeline_or_tip();
        }
        self.try_advance_view_on_timeout_quorum();
        self.maybe_emit_local_timeout();

        // --- Stage 1: COMMIT ---
        if let Some(block) = self.three_stage.take_commit_block() {
            let expected = self.height + 1;
            if block.header.height != expected {
                eprintln!(
                    "fractal-node: hyperbft skip stale commit slot height={} (chain={})",
                    block.header.height, self.height
                );
                self.three_stage.clear_from_height(block.header.height);
            } else {
                let tip_view = block.header.view;
                let tip_height = block.header.height;
                match self.apply_synced_block(&block) {
                    Ok(()) => {
                        self.optimistic.commit_through(&self.state);
                        self.hyperbft_pipeline.note_block_committed(tip_height);
                        self.maybe_persist_committed_block_to_rocksdb(&block);
                        // Anchor emission runs inside `apply_synced_block` (same as HotStuff path).
                        let hh = self.head_hash;
                        if let Some(formed) = self.try_form_qc(tip_view, tip_height, hh) {
                            if let Ok(h) = fractal_consensus::hash_qc(&formed.qc) {
                                self.parent_qc_hash = h;
                            }
                            self.hyperbft_pipeline.note_formed_qc(&formed);
                            self.maybe_upgrade_high_prepare_qc(&formed.qc);
                            if let Some(ref db) = self.chain_store {
                                let _ = db.persist_consensus_formed_qc_v1(
                                    self.shard_id,
                                    self.shard_topology.shard_count,
                                    &formed,
                                );
                            }
                        }
                        self.maybe_enqueue_proof_checkpoint(&block);
                        return ProduceTickOutcome::Produced(tip_height);
                    }
                    Err(e) => {
                        eprintln!(
                            "fractal-node: hyperbft commit rollback height={} err={e}",
                            block.header.height
                        );
                        self.optimistic.rollback();
                        self.three_stage.clear_from_height(block.header.height);
                        return ProduceTickOutcome::BuildFailed;
                    }
                }
            }
        }

        // --- Stage 2: VOTE ---
        let vote_meta = self.three_stage.vote.as_ref().map(|s| {
            (
                s.block.header.view,
                s.block.header.height,
                s.header_hash,
                s.formed_qc.is_none(),
            )
        });
        if let Some((view, height, hh, needs_qc)) = vote_meta {
            if needs_qc {
                let formed = self.try_form_qc(view, height, hh);
                if let Some(slot) = self.three_stage.vote.as_mut() {
                    slot.formed_qc = formed;
                }
            }
        }
        if self
            .three_stage
            .vote
            .as_ref()
            .and_then(|s| s.formed_qc.as_ref())
            .is_some()
            && self.three_stage.commit.is_none()
        {
            self.three_stage.commit = self.three_stage.vote.take();
        }

        // --- Stage 3: PROPOSE ---
        let view = self.view;
        if !self.is_my_turn(view) {
            return ProduceTickOutcome::NotMyTurn;
        }
        if self.min_consensus_stake_wei > 0 {
            if let Some(entry) = self.validators.entry(self.validator_index) {
                let bonded = self
                    .state
                    .consensus_stake_total_for_fingerprint(&entry.fingerprint);
                if bonded < self.min_consensus_stake_wei {
                    return ProduceTickOutcome::AwaitingConsensusStake;
                }
            }
        }

        let Some((parent_qc, parent_qc_signer_indices)) = self.resolve_parent_qc_for_propose()
        else {
            return ProduceTickOutcome::AwaitingParentQc;
        };

        self.optimistic.prepare_propose(&self.state);

        let base = self.base_fee;
        let pooled = self.mempool.drain_ready_gas_budget(self.gas_limit, base);
        let eth_raws: Vec<Option<Vec<u8>>> =
            pooled.iter().map(|p| p.eth_signed_raw.clone()).collect();
        let txs: Vec<Transaction> = pooled.into_iter().map(|p| p.tx).collect();
        let parent = self.head_hash;
        let height = self.height + 1;
        let ts = crate::now_ms();
        let proposer = self.validators.expected_proposer(view);
        let validator_fingerprints = self.validators.ids();
        let unbonding_ms: u64 = std::env::var("FRACTAL_UNBONDING_PERIOD_MS")
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(self.state.chain_economics.unbonding_period_ms);
        let block_reward_wei: u128 = std::env::var("FRACTAL_BLOCK_REWARD_WEI")
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);
        let evm_gas = self.evm_gas_used_for_txs(&txs);
        let parent_qc_signer_indices_for_finalize = parent_qc_signer_indices.clone();
        let finalize = BlockFinalizeContext {
            block_timestamp_ms: ts,
            unbonding_period_ms: unbonding_ms,
            proposer,
            parent_qc_signer_indices: &parent_qc_signer_indices_for_finalize,
            validator_fingerprints: &validator_fingerprints,
            treasury: fractal_core::DEVNET_FAUCET_TREASURY,
            block_reward_wei,
            base_fee_per_gas: base,
            evm_gas_used: evm_gas,
        };

        let block = match execute_and_build_block(
            self.chain_id,
            self.shard_id,
            height,
            view,
            parent,
            parent_qc,
            parent_qc_signer_indices,
            proposer,
            ts,
            self.gas_limit,
            &mut self.optimistic.scratch,
            txs,
            eth_raws,
            Some(finalize),
        ) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("fractal-node: hyperbft propose build failed: {e}");
                self.optimistic.rollback();
                return ProduceTickOutcome::BuildFailed;
            }
        };

        let hh = match header_hash(&block.header) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("fractal-node: hyperbft header_hash failed: {e}");
                self.optimistic.rollback();
                return ProduceTickOutcome::BuildFailed;
            }
        };

        self.three_stage.enqueue_proposed(block.clone(), hh);
        self.forward_vote_after_commit(&block);
        self.hyperbft_pipeline
            .note_block_produced(block.header.height);
        if self
            .three_stage
            .vote
            .as_ref()
            .map(|s| s.formed_qc.is_none())
            .unwrap_or(false)
        {
            let formed = self.try_form_qc(block.header.view, block.header.height, hh);
            if let Some(slot) = self.three_stage.vote.as_mut() {
                slot.formed_qc = formed;
            }
        }

        ProduceTickOutcome::Pipelined(block.header.height)
    }

    fn resolve_parent_qc_for_propose(
        &mut self,
    ) -> Option<(fractal_consensus::QuorumCertificate, Vec<u32>)> {
        if self.height == 0 {
            return Some(parent_qc_bundle(ParentQcResolution::Genesis));
        }
        let tip_block = self
            .blocks
            .iter()
            .find(|b| b.header.height == self.height)?;
        let tip_height = tip_block.header.height;
        let tip_view = tip_block.header.view;
        let tip_hh = header_hash(&tip_block.header).ok()?;
        let pipeline = self.hyperbft_pipeline.clone();
        let high = self.high_prepare_qc.clone();
        let chain_height = self.height;
        let stake_w = self.consensus_stake_weights();
        let mut try_form = |view: u64, height: u64, hh: fractal_crypto::Hash256| {
            self.try_form_qc(view, height, hh)
        };
        let resolution = resolve_parent_qc(
            ConsensusMode::HyperBft,
            chain_height,
            tip_height,
            tip_view,
            tip_hh,
            &self.vote_pool,
            &self.validators,
            Some(&stake_w),
            &pipeline,
            &high,
            &mut try_form,
        )?;
        if let ParentQcResolution::Formed(ref f) = resolution {
            self.hyperbft_pipeline.note_formed_qc(f);
            if let Some(ref db) = self.chain_store {
                let _ = db.persist_consensus_formed_qc_v1(
                    self.shard_id,
                    self.shard_topology.shard_count,
                    f,
                );
            }
        }
        Some(parent_qc_bundle(resolution))
    }
}
