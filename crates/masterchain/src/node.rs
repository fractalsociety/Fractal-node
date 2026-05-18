//! Dedicated masterchain validator (BFT over coordination blocks only).

use std::sync::Arc;

use fractal_consensus::{
    FormedTimeoutCert, QuorumCertificate, Timeout, TimeoutPool, ValidatorSet, Vote,
    genesis_parent_qc, hash_qc,
};
use fractal_crypto::{BlsSecretKey, Hash256};
use fractal_shard::{MasterchainBlockV1, ProofSubmissionV1, ShardAnchor};
use tokio::sync::Mutex;
use tokio::sync::mpsc::UnboundedSender;

use crate::bft::{
    MasterchainTimeoutGossipV1, MasterchainVoteGossipV1, record_masterchain_timeout,
    record_masterchain_vote_message, sign_masterchain_timeout, sign_masterchain_vote,
};
use crate::ledger::{
    MasterchainError, MasterchainLedger, ProofSlashingPolicyV1, ProverEconomicsParamsV1,
    ProverMarketParamsV1,
};

pub struct MasterchainBftNode {
    pub ledger: MasterchainLedger,
    pub validators: ValidatorSet,
    pub validator_index: u32,
    pub validator_secret: Option<BlsSecretKey>,
    pub view: u64,
    pub vote_pool: fractal_consensus::VotePool,
    pub timeout_pool: TimeoutPool,
    pub parent_qc_hash: Hash256,
    pub last_formed_qc: Option<fractal_consensus::FormedQc>,
    pub last_timeout_cert: Option<FormedTimeoutCert>,
    pub chain_store: Option<fractal_storage::FractalRocksDb>,
    pub shard_count: u32,
    pub vote_sink: Option<UnboundedSender<Vec<u8>>>,
    pub timeout_sink: Option<UnboundedSender<Vec<u8>>>,
}

impl MasterchainBftNode {
    pub fn devnet_singleton() -> Self {
        let validators = ValidatorSet::phase1_singleton();
        let secret = validators.dev_bls_secret(0);
        Self {
            ledger: MasterchainLedger::default(),
            validators: validators.clone(),
            validator_index: 0,
            validator_secret: secret,
            view: 0,
            vote_pool: fractal_consensus::VotePool::default(),
            timeout_pool: TimeoutPool::default(),
            parent_qc_hash: hash_qc(&genesis_parent_qc()).unwrap_or([0u8; 32]),
            last_formed_qc: None,
            last_timeout_cert: None,
            chain_store: None,
            shard_count: 2,
            vote_sink: None,
            timeout_sink: None,
        }
    }

    pub fn devnet_bft7(validator_index: u32) -> Self {
        let validators = ValidatorSet::phase2_bft7_fixture();
        let secret = validators.dev_bls_secret(validator_index as usize);
        Self {
            ledger: MasterchainLedger::default(),
            validators,
            validator_index,
            validator_secret: secret,
            view: 0,
            vote_pool: fractal_consensus::VotePool::default(),
            timeout_pool: TimeoutPool::default(),
            parent_qc_hash: hash_qc(&genesis_parent_qc()).unwrap_or([0u8; 32]),
            last_formed_qc: None,
            last_timeout_cert: None,
            chain_store: None,
            shard_count: 2,
            vote_sink: None,
            timeout_sink: None,
        }
    }

    pub fn devnet_from_env() -> Self {
        let validators = masterchain_validator_set_from_env();
        let validator_index = masterchain_validator_index_from_env(&validators) as u32;
        let secret = validators.dev_bls_secret(validator_index as usize);
        Self {
            ledger: masterchain_ledger_from_env(),
            validators,
            validator_index,
            validator_secret: secret,
            view: 0,
            vote_pool: fractal_consensus::VotePool::default(),
            timeout_pool: TimeoutPool::default(),
            parent_qc_hash: hash_qc(&genesis_parent_qc()).unwrap_or([0u8; 32]),
            last_formed_qc: None,
            last_timeout_cert: None,
            chain_store: None,
            shard_count: 2,
            vote_sink: None,
            timeout_sink: None,
        }
    }

    pub fn set_vote_sink(&mut self, sink: Option<UnboundedSender<Vec<u8>>>) {
        self.vote_sink = sink;
    }

    pub fn set_timeout_sink(&mut self, sink: Option<UnboundedSender<Vec<u8>>>) {
        self.timeout_sink = sink;
    }

    pub fn is_my_turn(&self, view: u64) -> bool {
        self.validators
            .is_proposer_for_view(view, self.validator_index as usize)
    }

    pub fn try_produce_round(&mut self) -> Result<Option<MasterchainBlockV1>, MasterchainError> {
        if !self.is_my_turn(self.view) {
            return Ok(None);
        }
        let prover = prover_address_from_env();
        let Some(mc) = self.ledger.seal_round(prover)? else {
            return Ok(None);
        };
        self.persist_sealed_block(&mc);
        if let Some(ref secret) = self.validator_secret {
            let vote = sign_masterchain_vote(self.view, &mc, self.validator_index, secret);
            if let Some(formed) = record_masterchain_vote_message(
                &mut self.vote_pool,
                &self.validators,
                None,
                &mc,
                vote.clone(),
            ) {
                if let Ok(h) = hash_qc(&formed.qc) {
                    self.parent_qc_hash = h;
                }
                self.last_formed_qc = Some(formed);
                self.vote_pool
                    .prune_below_height(mc.height.saturating_sub(1));
            }
            self.publish_vote_gossip(&mc, vote);
        }
        eprintln!(
            "fractal-masterchain: sealed height={} shards={} global_zk_root=0x{}",
            mc.height,
            mc.shard_anchors.len(),
            hex::encode(mc.global_zk_root)
        );
        self.view = self.view.saturating_add(1);
        Ok(Some(mc))
    }

    pub fn sign_vote_for_block(&self, block: &MasterchainBlockV1) -> Option<Vote> {
        self.validator_secret
            .as_ref()
            .map(|secret| sign_masterchain_vote(self.view, block, self.validator_index, secret))
    }

    pub fn ingest_vote_for_block(
        &mut self,
        block: &MasterchainBlockV1,
        vote: Vote,
    ) -> Option<fractal_consensus::FormedQc> {
        let formed = record_masterchain_vote_message(
            &mut self.vote_pool,
            &self.validators,
            None,
            block,
            vote,
        )?;
        if let Ok(h) = hash_qc(&formed.qc) {
            self.parent_qc_hash = h;
        }
        self.last_formed_qc = Some(formed.clone());
        eprintln!(
            "fractal-masterchain: formed QC height={} view={} signers={:?}",
            formed.qc.block_height, formed.qc.view, formed.signer_indices
        );
        Some(formed)
    }

    pub fn ingest_vote_gossip(
        &mut self,
        msg: MasterchainVoteGossipV1,
    ) -> Option<fractal_consensus::FormedQc> {
        let block_hash = crate::bft::masterchain_block_hash(&msg.block);
        let already_voted = self
            .vote_pool
            .signer_indices(msg.vote.view, block_hash)
            .contains(&self.validator_index);
        let formed = self.ingest_vote_for_block(&msg.block, msg.vote.clone());
        if !already_voted
            && msg.vote.validator_index != self.validator_index
            && msg.block.height >= self.ledger.masterchain_height
        {
            if let Some(ref secret) = self.validator_secret {
                let vote =
                    sign_masterchain_vote(msg.vote.view, &msg.block, self.validator_index, secret);
                let _ = record_masterchain_vote_message(
                    &mut self.vote_pool,
                    &self.validators,
                    None,
                    &msg.block,
                    vote.clone(),
                );
                self.publish_vote_gossip(&msg.block, vote);
            }
        }
        formed
    }

    pub fn sign_timeout(&self, high_qc: QuorumCertificate) -> Option<Timeout> {
        self.validator_secret.as_ref().map(|secret| {
            sign_masterchain_timeout(self.view, high_qc, self.validator_index, secret)
        })
    }

    pub fn ingest_timeout(&mut self, timeout: Timeout) -> Option<FormedTimeoutCert> {
        let (_outcome, cert) =
            record_masterchain_timeout(&mut self.timeout_pool, &self.validators, timeout);
        if let Some(cert) = cert {
            self.view = self.view.max(cert.view.saturating_add(1));
            self.last_timeout_cert = Some(cert.clone());
            self.timeout_pool
                .prune_views_before(self.view.saturating_sub(1));
            Some(cert)
        } else {
            None
        }
    }

    pub fn ingest_timeout_gossip(
        &mut self,
        msg: MasterchainTimeoutGossipV1,
    ) -> Option<FormedTimeoutCert> {
        self.ingest_timeout(msg.timeout)
    }

    pub fn ingest_anchor(&mut self, anchor: ShardAnchor) -> Result<(), MasterchainError> {
        self.ledger.ingest_shard_anchor(anchor)
    }

    pub fn submit_validity_proof(
        &mut self,
        sub: ProofSubmissionV1,
    ) -> Result<(), MasterchainError> {
        self.ledger.submit_validity_proof(sub)
    }

    fn persist_sealed_block(&self, mc: &MasterchainBlockV1) {
        let Some(ref db) = self.chain_store else {
            return;
        };
        for a in &mc.shard_anchors {
            if let Err(e) = db.persist_shard_anchor_v1(a) {
                eprintln!(
                    "fractal-masterchain: persist anchor shard={} err={e}",
                    a.shard_id
                );
            }
        }
        if let Err(e) = db.persist_masterchain_block_v1(mc) {
            eprintln!(
                "fractal-masterchain: persist block height={} err={e}",
                mc.height
            );
        }
    }

    fn publish_vote_gossip(&self, block: &MasterchainBlockV1, vote: Vote) {
        let Some(ref tx) = self.vote_sink else {
            return;
        };
        match borsh::to_vec(&MasterchainVoteGossipV1 {
            block: block.clone(),
            vote,
        }) {
            Ok(bytes) => {
                let _ = tx.send(bytes);
            }
            Err(e) => eprintln!("fractal-masterchain: encode vote gossip failed: {e}"),
        }
    }
}

pub type MasterchainHandle = Arc<Mutex<MasterchainBftNode>>;

pub fn prover_address_from_env() -> [u8; 20] {
    address_from_env("FRACTAL_PROVER_ADDRESS")
}

pub fn prover_reward_treasury_from_env() -> [u8; 20] {
    address_from_env("FRACTAL_PROVER_TREASURY_ADDRESS")
}

pub fn masterchain_ledger_from_env() -> MasterchainLedger {
    let mut ledger = MasterchainLedger::default();
    let min_prover_bond = std::env::var("FRACTAL_PROVER_MARKET_MIN_BOND_WEI")
        .ok()
        .and_then(|s| s.trim().parse::<u128>().ok())
        .unwrap_or(0);
    if min_prover_bond > 0 {
        let max_pending = std::env::var("FRACTAL_PROVER_MARKET_MAX_PENDING")
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok())
            .filter(|&n| n > 0)
            .unwrap_or(8);
        let max_range = std::env::var("FRACTAL_PROVER_MARKET_MAX_RANGE_BLOCKS")
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .filter(|&n| n > 0)
            .unwrap_or(10_000);
        ledger.set_prover_market(ProverMarketParamsV1 {
            version: ProverMarketParamsV1::VERSION,
            enabled: true,
            require_registered_identity: true,
            min_identity_bond_wei: min_prover_bond,
            max_pending_submissions_per_prover: max_pending,
            max_range_blocks: max_range,
        });
        let prover = prover_address_from_env();
        let bond = std::env::var("FRACTAL_PROVER_BOND_WEI")
            .ok()
            .and_then(|s| s.trim().parse::<u128>().ok())
            .unwrap_or(0);
        if prover != [0u8; 20] && bond >= min_prover_bond {
            let _ = ledger.register_prover_identity(prover, bond);
        }
    }
    let base_reward = std::env::var("FRACTAL_PROVER_REWARD_PER_BLOCK_WEI")
        .ok()
        .and_then(|s| s.trim().parse::<u128>().ok())
        .unwrap_or(0);
    if base_reward > 0 {
        ledger.set_prover_economics(ProverEconomicsParamsV1 {
            version: ProverEconomicsParamsV1::VERSION,
            enabled: true,
            treasury: prover_reward_treasury_from_env(),
            base_reward_per_block_wei: base_reward,
            lag_half_life_seconds: std::env::var("FRACTAL_PROVER_REWARD_LAG_HALF_LIFE_SECONDS")
                .ok()
                .and_then(|s| s.trim().parse::<u32>().ok())
                .filter(|&n| n > 0)
                .unwrap_or(60),
        });
        ledger.fund_prover_treasury(
            std::env::var("FRACTAL_PROVER_REWARD_TREASURY_WEI")
                .ok()
                .and_then(|s| s.trim().parse::<u128>().ok())
                .unwrap_or(0),
        );
    }
    configure_proof_slashing_from_env(&mut ledger);
    ledger
}

fn env_flag(name: &str) -> bool {
    std::env::var(name)
        .map(|v| {
            let v = v.trim();
            v == "1"
                || v.eq_ignore_ascii_case("true")
                || v.eq_ignore_ascii_case("yes")
                || v.eq_ignore_ascii_case("on")
        })
        .unwrap_or(false)
}

pub fn configure_proof_slashing_from_env(ledger: &mut MasterchainLedger) {
    if !env_flag("FRACTAL_PROOF_SLASHING_ENABLED") {
        return;
    }
    ledger.set_proof_slashing_policy(ProofSlashingPolicyV1 {
        enabled: true,
        require_verified_stwo: env_flag("FRACTAL_PROOF_REQUIRE_VERIFIED_STWO"),
        slash_amount_wei: std::env::var("FRACTAL_PROOF_SLASH_AMOUNT_WEI")
            .ok()
            .and_then(|s| s.trim().parse::<u128>().ok())
            .unwrap_or(0),
    });
}

fn address_from_env(name: &str) -> [u8; 20] {
    let raw = std::env::var(name).ok();
    let Some(s) = raw.filter(|s| !s.trim().is_empty()) else {
        return [0u8; 20];
    };
    let s = s.trim().strip_prefix("0x").unwrap_or(s.trim());
    let bytes = hex::decode(s).unwrap_or_default();
    if bytes.len() != 20 {
        return [0u8; 20];
    }
    let mut out = [0u8; 20];
    out.copy_from_slice(&bytes);
    out
}

pub async fn masterchain_bft_producer_loop(node: MasterchainHandle) {
    let ms = masterchain_block_time_ms_from_env();
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
        let mut n = node.lock().await;
        if let Err(e) = n.try_produce_round() {
            eprintln!("fractal-masterchain: produce round err={e}");
        }
    }
}

pub fn masterchain_block_time_ms_from_env() -> u64 {
    std::env::var("FRACTAL_MASTERCHAIN_BLOCK_MS")
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .filter(|&n| n >= 100)
        .unwrap_or(1000)
}

pub fn masterchain_validator_set_from_env() -> ValidatorSet {
    match std::env::var("FRACTAL_VALIDATOR_SET")
        .map(|s| s.to_ascii_lowercase())
        .as_deref()
    {
        Ok("7") | Ok("bft7") => ValidatorSet::phase2_bft7_fixture(),
        Ok("21") | Ok("bft21") => ValidatorSet::phase3_bft21_fixture(),
        _ => ValidatorSet::phase1_singleton(),
    }
}

pub fn masterchain_validator_index_from_env(validators: &ValidatorSet) -> usize {
    let raw = std::env::var("FRACTAL_VALIDATOR_INDEX").unwrap_or_default();
    let parsed: usize = raw.trim().parse().unwrap_or(0);
    let n = validators.len().max(1);
    if parsed >= n {
        eprintln!(
            "fractal-masterchain: FRACTAL_VALIDATOR_INDEX={raw} >= validator_set_size={n}; clamping to 0"
        );
        0
    } else {
        parsed
    }
}
