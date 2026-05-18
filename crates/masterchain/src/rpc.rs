//! `MasterchainRpc` trait implementation for the dedicated BFT node.

use fractal_proof_aggregator::Plonky2ProofBundleV1;
use fractal_rpc::{InvalidProofSlashEventJson, MasterchainRpc, ProverIdentityJson};
use fractal_shard::{MasterchainBlockV1, ProofSubmissionV1, ShardAnchor};

use crate::node::MasterchainBftNode;

impl MasterchainRpc for MasterchainBftNode {
    fn masterchain_height(&self) -> u64 {
        self.ledger.masterchain_height
    }

    fn submit_shard_anchor(&mut self, anchor: ShardAnchor) -> Result<(), String> {
        self.ingest_anchor(anchor).map_err(|e| e.to_string())
    }

    fn submit_validity_proof(&mut self, sub: ProofSubmissionV1) -> Result<(), String> {
        self.submit_validity_proof(sub).map_err(|e| e.to_string())
    }

    fn register_prover(&mut self, prover: [u8; 20], bond_wei: u128) -> Result<(), String> {
        self.ledger
            .register_prover_identity(prover, bond_wei)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    fn get_prover_identity(&self, prover: [u8; 20]) -> Option<ProverIdentityJson> {
        self.ledger
            .prover_identity(&prover)
            .map(|id| ProverIdentityJson {
                prover: id.prover,
                bond_wei: id.bond_wei,
                registered_at_masterchain_height: id.registered_at_masterchain_height,
                active: id.active,
            })
    }

    fn get_invalid_proof_slash_events(&self) -> Vec<InvalidProofSlashEventJson> {
        self.ledger
            .invalid_proof_slash_events()
            .iter()
            .map(|e| InvalidProofSlashEventJson {
                masterchain_height: e.masterchain_height,
                prover: e.prover,
                shard_id: e.shard_id,
                start_block: e.start_block,
                end_block: e.end_block,
                proof_digest: e.proof_digest,
                reason_code: e.reason_code,
                evidence_hash: e.evidence_hash,
                slash_amount_wei: e.slash_amount_wei,
                executed: e.executed,
                burned_bond_wei: e.burned_bond_wei,
                bond_before_wei: e.bond_before_wei,
                bond_after_wei: e.bond_after_wei,
                prover_active_after: e.prover_active_after,
            })
            .collect()
    }

    fn get_masterchain_head(&self) -> Option<MasterchainBlockV1> {
        self.ledger.head().cloned()
    }

    fn get_global_zk_root(&self) -> Option<[u8; 32]> {
        self.ledger.global_zk_root()
    }

    fn get_global_zk_proof(&self) -> Option<Plonky2ProofBundleV1> {
        self.ledger.plonky2_bundle().cloned()
    }

    fn get_shard_anchor(&self, shard_id: u32, block_height: Option<u64>) -> Option<ShardAnchor> {
        if let Some(h) = block_height {
            self.ledger
                .anchor_for_shard(shard_id)
                .filter(|a| a.block_height == h)
                .cloned()
        } else {
            self.ledger.anchor_for_shard(shard_id).cloned()
        }
    }
}
