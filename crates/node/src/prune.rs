//! Prune historical execution data after masterchain accepts validity proofs (PRD §6.2.3, M11).

use fractal_consensus::Block;
use fractal_shard::MasterchainBlockV1;

/// Minimum execution blocks kept below tip after a prune (sync / rollback buffer).
pub const MIN_EXECUTION_RETAIN: u64 = 32;

/// When true, drop proved checkpoint blobs and old in-memory blocks after seal.
pub fn prune_after_validity_proof_from_env() -> bool {
    match std::env::var("FRACTAL_PRUNE_AFTER_VALIDITY_PROOF")
        .unwrap_or_else(|_| "1".into())
        .to_ascii_lowercase()
        .as_str()
    {
        "0" | "false" | "off" | "no" => false,
        _ => true,
    }
}

/// Upper inclusive height covered by tier-1 proofs on this masterchain block.
#[must_use]
pub fn max_proved_end_block(mc: &MasterchainBlockV1) -> Option<u64> {
    mc.validity_proofs
        .iter()
        .map(|p| p.end_block)
        .max()
        .filter(|&h| h > 0)
}

/// Drop in-memory execution blocks plus RocksDB execution rows and checkpoint proofs at or below
/// `prune_below`.
pub fn prune_execution_history(
    blocks: &mut Vec<Block>,
    chain_store: &Option<fractal_storage::FractalRocksDb>,
    shard_id: u32,
    shard_count: u32,
    tip_height: u64,
    prune_below: u64,
) -> (usize, usize, usize) {
    if prune_below == 0 {
        return (0, 0, 0);
    }
    let safe_keep = tip_height.saturating_sub(MIN_EXECUTION_RETAIN);
    let cap = prune_below.min(safe_keep);
    if cap == 0 {
        return (0, 0, 0);
    }

    let before = blocks.len();
    blocks.retain(|b| b.header.height == 0 || b.header.height > cap);
    let blocks_dropped = before.saturating_sub(blocks.len());

    let mut proofs_dropped = 0usize;
    let mut rocks_execution_rows_dropped = 0usize;
    if let Some(db) = chain_store {
        for h in 1..=cap {
            match db.prune_execution_height_v1(shard_id, shard_count, h) {
                Ok(rows) => {
                    rocks_execution_rows_dropped =
                        rocks_execution_rows_dropped.saturating_add(rows.blocks);
                    rocks_execution_rows_dropped =
                        rocks_execution_rows_dropped.saturating_add(rows.block_hash_indexes);
                    rocks_execution_rows_dropped =
                        rocks_execution_rows_dropped.saturating_add(rows.tx_indexes);
                    rocks_execution_rows_dropped =
                        rocks_execution_rows_dropped.saturating_add(rows.receipts);
                    rocks_execution_rows_dropped =
                        rocks_execution_rows_dropped.saturating_add(rows.native_events);
                    rocks_execution_rows_dropped =
                        rocks_execution_rows_dropped.saturating_add(rows.state_rows);
                }
                Err(e) => eprintln!("fractal-node: prune execution RocksDB height={h} err={e}"),
            }
            match db.delete_proof_blob(shard_id, shard_count, h) {
                Ok(true) => proofs_dropped += 1,
                Ok(false) => {}
                Err(e) => eprintln!("fractal-node: prune checkpoint height={h} err={e}"),
            }
        }
    }

    (blocks_dropped, proofs_dropped, rocks_execution_rows_dropped)
}

/// Run after a masterchain seal that includes a non-zero `globalZkRoot`.
pub fn maybe_prune_after_masterchain_seal(
    mc: &MasterchainBlockV1,
    blocks: &mut Vec<Block>,
    chain_store: &Option<fractal_storage::FractalRocksDb>,
    shard_id: u32,
    shard_count: u32,
    tip_height: u64,
) {
    if !prune_after_validity_proof_from_env() {
        return;
    }
    if mc.global_zk_root == [0u8; 32] || mc.validity_proofs.is_empty() {
        return;
    }
    let Some(proved_end) = max_proved_end_block(mc) else {
        return;
    };
    let (blocks_dropped, proofs_dropped, rocks_execution_rows_dropped) = prune_execution_history(
        blocks,
        chain_store,
        shard_id,
        shard_count,
        tip_height,
        proved_end,
    );
    if blocks_dropped > 0 || proofs_dropped > 0 || rocks_execution_rows_dropped > 0 {
        eprintln!(
            "fractal-node: pruned after validity proof proved_end={proved_end} blocks_dropped={blocks_dropped} rocks_execution_rows_dropped={rocks_execution_rows_dropped} checkpoint_proofs_dropped={proofs_dropped} tip={tip_height}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fractal_consensus::{Block, BlockHeader, genesis_parent_qc};
    use fractal_core::State;
    use fractal_core::state_root;

    fn stub_block(height: u64) -> Block {
        let state = State::default();
        let sr = state_root(&state).expect("sr");
        Block {
            header: BlockHeader {
                version: 1,
                chain_id: 1,
                height,
                view: height,
                parent_hash: [0u8; 32],
                parent_qc_hash: [0u8; 32],
                proposer: [0u8; 32],
                timestamp_ms: 1,
                state_root: sr,
                tx_root: [0u8; 32],
                gas_used: 0,
                gas_limit: 30_000_000,
                shard_id: 0,
                extra: [0u8; 32],
            },
            transactions: vec![],
            parent_qc: genesis_parent_qc(),
            parent_qc_signer_indices: vec![],
            eth_signed_raw: vec![],
        }
    }

    #[test]
    fn prune_drops_old_blocks_keeps_genesis_and_tip_window() {
        let mut blocks: Vec<Block> = (0..=40).map(stub_block).collect();
        let (bd, _, _) = prune_execution_history(&mut blocks, &None, 0, 1, 40, 10);
        assert!(bd > 0);
        assert!(blocks.iter().any(|b| b.header.height == 0));
        assert!(blocks.iter().any(|b| b.header.height == 40));
        assert!(!blocks.iter().any(|b| b.header.height == 5));
    }
}
