//! Light-client verification tests (in-process, no RPC).

use fractal_light_client::{
    LightClientError, LightClientHeadV1, parse_light_client_head_json, verify_light_client_head,
    verify_masterchain_block,
};
use fractal_proof_aggregator::{GlobalZkStatementV1, Plonky2ProofBundleV1, prove_and_aggregate};
use fractal_shard::{ProofSubmissionV1, ShardAnchor, global_state_root_from_anchors};

fn anchor(shard: u32, h: u64, byte: u8) -> ShardAnchor {
    ShardAnchor {
        shard_id: shard,
        block_height: h,
        state_root: [byte; 32],
        witness_commitment: [byte.wrapping_add(1); 32],
    }
}

fn sub(shard: u32, start: u64, end: u64, digest: u8) -> ProofSubmissionV1 {
    ProofSubmissionV1 {
        shard_id: shard,
        start_block: start,
        end_block: end,
        prover: [0xab; 20],
        lag_seconds: 1,
        proof_digest: [digest; 32],
    }
}

#[test]
fn verify_masterchain_with_plonky2_bundle() {
    let anchors = vec![anchor(0, 100, 0x10), anchor(1, 200, 0x20)];
    let proofs = vec![sub(0, 1, 100, 1), sub(1, 1, 200, 2)];
    let gsr = global_state_root_from_anchors(&anchors);
    let aggregated = prove_and_aggregate(5, &gsr, &proofs).expect("aggregate");
    let bundle = Plonky2ProofBundleV1::from_aggregated(
        5,
        GlobalZkStatementV1 {
            global_state_root: gsr,
            global_zk_root: aggregated.global_zk_root,
            validity_proofs: proofs.clone(),
            verified_stwo_statements: vec![],
        },
        &aggregated,
    );
    let block = fractal_shard::masterchain_block_from_anchors_and_messages(
        5,
        anchors.clone(),
        proofs,
        aggregated.global_zk_root,
        vec![],
    );
    let verified = verify_masterchain_block(&block, Some(&bundle)).expect("verify");
    assert_eq!(verified.masterchain_height, 5);
    assert_eq!(verified.global_state_root, gsr);
    assert_eq!(verified.shard_state_root(0), Some(anchors[0].state_root));
}

#[test]
fn verify_rejects_global_state_root_mismatch() {
    let anchors = vec![anchor(0, 10, 0x01)];
    let proofs = vec![sub(0, 1, 10, 0xaa)];
    let mut block = fractal_shard::masterchain_block_from_anchors(3, anchors, proofs, [0u8; 32]);
    block.global_state_root = [0xff; 32];
    let err = verify_masterchain_block(&block, None).unwrap_err();
    assert_eq!(err, LightClientError::GlobalStateRootMismatch);
}

#[test]
fn parse_and_verify_rpc_json_shape() {
    let anchors = vec![anchor(0, 50, 0x33)];
    let proofs = vec![sub(0, 1, 50, 0x44)];
    let gsr = global_state_root_from_anchors(&anchors);
    let aggregated = prove_and_aggregate(2, &gsr, &proofs).expect("aggregate");
    let bundle = Plonky2ProofBundleV1::from_aggregated(
        2,
        GlobalZkStatementV1 {
            global_state_root: gsr,
            global_zk_root: aggregated.global_zk_root,
            validity_proofs: proofs.clone(),
            verified_stwo_statements: vec![],
        },
        &aggregated,
    );
    let block = fractal_shard::masterchain_block_from_anchors(
        2,
        anchors,
        proofs,
        aggregated.global_zk_root,
    );
    let json = serde_json::json!({
        "height": format!("0x{:x}", block.height),
        "shardAnchors": [{
            "shardId": "0x0",
            "blockHeight": "0x32",
            "stateRoot": format!("0x{}", hex::encode(block.shard_anchors[0].state_root)),
            "witnessCommitment": format!("0x{}", hex::encode(block.shard_anchors[0].witness_commitment)),
        }],
        "validityProofs": [{
            "shardId": "0x0",
            "startBlock": "0x1",
            "endBlock": "0x32",
            "prover": format!("0x{}", hex::encode(block.validity_proofs[0].prover)),
            "lagSeconds": 1,
            "proofDigest": format!("0x{}", hex::encode(block.validity_proofs[0].proof_digest)),
        }],
        "crossShardMessages": [],
        "globalStateRoot": format!("0x{}", hex::encode(block.global_state_root)),
        "globalZkRoot": format!("0x{}", hex::encode(block.global_zk_root)),
        "plonky2": {
            "version": bundle.version,
            "masterchainHeight": "0x2",
            "globalStateRoot": format!("0x{}", hex::encode(bundle.statement.global_state_root)),
            "globalZkRoot": format!("0x{}", hex::encode(bundle.statement.global_zk_root)),
            "validityProofs": [{
                "shardId": "0x0",
                "startBlock": "0x1",
                "endBlock": "0x32",
                "prover": format!("0x{}", hex::encode(bundle.statement.validity_proofs[0].prover)),
                "lagSeconds": 1,
                "proofDigest": format!("0x{}", hex::encode(bundle.statement.validity_proofs[0].proof_digest)),
            }],
            "snarkBytes": format!("0x{}", hex::encode(&bundle.snark_bytes)),
        },
    });
    let head = parse_light_client_head_json(&json).expect("parse");
    let verified = verify_light_client_head(&head).expect("verify");
    assert_eq!(verified.global_zk_root, block.global_zk_root);
    assert!(matches!(
        head,
        LightClientHeadV1 {
            plonky2: Some(_),
            ..
        }
    ));
}
