//! Dedicated masterchain BFT: two shard anchors → one coordination block.

#[cfg(feature = "runtime")]
use std::sync::Arc;
#[cfg(feature = "runtime")]
use std::time::Duration;

use fractal_masterchain::ledger::{
    forced_inclusion_queue_root, prover_reward_wei, AsyncCrossZoneMessageV1,
    ExecutionZoneMetadataV1, MasterchainError, MasterchainLedger, ProofSlashingPolicyV1,
    ProverEconomicsParamsV1, ProverMarketParamsV1, ZoneProofFinalUpdateV1, INVALID_PROOF_BAD_RANGE,
    INVALID_PROOF_MISSING_VERIFIED_STWO, INVALID_PROOF_RANGE_EXCEEDS_ANCHOR,
};
#[cfg(feature = "runtime")]
use fractal_masterchain::{
    bft::{verify_masterchain_qc, verify_masterchain_timeout_cert},
    masterchain_gossip_task,
    node::MasterchainBftNode,
    MasterchainHandle,
};
use fractal_shard::{CrossShardMessageV1, ProofSubmissionV1, ShardAnchor};
#[cfg(feature = "runtime")]
use libp2p::{multiaddr::Protocol, Multiaddr};
#[cfg(feature = "runtime")]
use tokio::sync::Mutex;

fn anchor(shard: u32, height: u64, byte: u8) -> ShardAnchor {
    ShardAnchor {
        shard_id: shard,
        block_height: height,
        state_root: [byte; 32],
        witness_commitment: [byte.wrapping_add(1); 32],
    }
}

#[cfg(feature = "runtime")]
#[tokio::test]
async fn dedicated_masterchain_seals_multi_shard_round() {
    let node: MasterchainHandle = Arc::new(Mutex::new(MasterchainBftNode::devnet_singleton()));
    {
        let mut n = node.lock().await;
        n.ingest_anchor(anchor(0, 4, 1)).expect("shard 0");
        n.ingest_anchor(anchor(1, 8, 2)).expect("shard 1");
    }
    {
        let mut n = node.lock().await;
        let mc = n.try_produce_round().expect("produce").expect("sealed");
        assert_eq!(mc.height, 1);
        assert_eq!(mc.shard_anchors.len(), 2);
        assert_ne!(mc.global_state_root, [0u8; 32]);
    }
}

#[cfg(feature = "runtime")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn bft7_masterchain_gossipsub_votes_form_qc_without_direct_injection() {
    let listen: Multiaddr = "/ip4/127.0.0.1/udp/0/quic-v1".parse().unwrap();
    let mut nodes: Vec<MasterchainHandle> = Vec::new();
    let mut tasks = Vec::new();

    let (vote_tx, vote_rx) = tokio::sync::mpsc::unbounded_channel();
    let (timeout_tx, timeout_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut first = MasterchainBftNode::devnet_bft7(0);
    first.set_vote_sink(Some(vote_tx));
    first.set_timeout_sink(Some(timeout_tx));
    let first: MasterchainHandle = Arc::new(Mutex::new(first));
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let mut first_task = tokio::spawn(masterchain_gossip_task(
        first.clone(),
        listen.clone(),
        Vec::new(),
        Some(ready_tx),
        Some(vote_rx),
        Some(timeout_rx),
    ));
    let (addr, peer) = tokio::select! {
        ready = ready_rx => match ready {
            Ok(v) => v,
            Err(e) => {
                let result = first_task.await;
                panic!("first ready failed: {e:?}; task={result:?}");
            }
        },
        result = &mut first_task => panic!("first gossip task exited before ready: {result:?}"),
    };
    tasks.push(first_task);
    let mut bootstrap = addr.clone();
    bootstrap.push(Protocol::P2p(peer));
    nodes.push(first.clone());

    for idx in 1u32..7 {
        let (vote_tx, vote_rx) = tokio::sync::mpsc::unbounded_channel();
        let (timeout_tx, timeout_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut node = MasterchainBftNode::devnet_bft7(idx);
        node.set_vote_sink(Some(vote_tx));
        node.set_timeout_sink(Some(timeout_tx));
        let node: MasterchainHandle = Arc::new(Mutex::new(node));
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        tasks.push(tokio::spawn(masterchain_gossip_task(
            node.clone(),
            listen.clone(),
            vec![bootstrap.clone()],
            Some(ready_tx),
            Some(vote_rx),
            Some(timeout_rx),
        )));
        let _ = ready_rx.await.expect("validator ready");
        nodes.push(node);
    }

    tokio::time::sleep(Duration::from_millis(700)).await;
    {
        let mut proposer = nodes[0].lock().await;
        proposer.ingest_anchor(anchor(0, 4, 1)).expect("anchor");
        let block = proposer
            .try_produce_round()
            .expect("produce")
            .expect("block");
        assert_eq!(block.height, 1);
    }

    let formed = tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            if let Some(qc) = nodes[0].lock().await.last_formed_qc.clone() {
                if qc.signer_indices.len() >= 5 {
                    break qc;
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("QC over gossiped votes");
    assert!(formed.signer_indices.contains(&0));
    assert!(
        formed.signer_indices.len() >= 5,
        "expected 5-of-7 QC, got {:?}",
        formed.signer_indices
    );

    for task in tasks {
        task.abort();
    }
}

#[test]
fn ingest_rejects_stale_anchor_height() {
    let mut ledger = MasterchainLedger::default();
    ledger.ingest_shard_anchor(anchor(0, 10, 1)).expect("first");
    assert!(ledger.ingest_shard_anchor(anchor(0, 9, 2)).is_err());
}

#[test]
fn masterchain_orders_cross_shard_messages_deterministically() {
    let mut ledger = MasterchainLedger::default();
    ledger
        .ingest_shard_anchor(anchor(0, 10, 1))
        .expect("anchor 0");
    ledger
        .ingest_shard_anchor(anchor(1, 10, 2))
        .expect("anchor 1");

    let msg_b = CrossShardMessageV1 {
        from_shard: 1,
        to_shard: 0,
        payload_hash: [0x22; 32],
        payload: vec![0x22],
    };
    let msg_a = CrossShardMessageV1 {
        from_shard: 0,
        to_shard: 1,
        payload_hash: [0x11; 32],
        payload: vec![0x11],
    };
    ledger.submit_cross_shard_message(msg_b.clone());
    ledger.submit_cross_shard_message(msg_a.clone());
    ledger.submit_cross_shard_message(msg_a.clone());

    let block = ledger.seal_round([0u8; 20]).expect("seal").expect("block");
    assert_eq!(block.cross_shard_messages, vec![msg_a, msg_b]);
}

fn proof(start: u64, end: u64, digest: [u8; 32]) -> ProofSubmissionV1 {
    ProofSubmissionV1 {
        shard_id: 0,
        start_block: start,
        end_block: end,
        prover: [0x44; 20],
        lag_seconds: 0,
        proof_digest: digest,
    }
}

fn lagged_proof(start: u64, end: u64, lag_seconds: u32, digest: [u8; 32]) -> ProofSubmissionV1 {
    ProofSubmissionV1 {
        lag_seconds,
        ..proof(start, end, digest)
    }
}

fn slashing_policy() -> ProofSlashingPolicyV1 {
    ProofSlashingPolicyV1 {
        enabled: true,
        require_verified_stwo: true,
        slash_amount_wei: 123,
    }
}

fn prover_economics() -> ProverEconomicsParamsV1 {
    ProverEconomicsParamsV1 {
        version: ProverEconomicsParamsV1::VERSION,
        enabled: true,
        treasury: [0x99; 20],
        base_reward_per_block_wei: 100,
        lag_half_life_seconds: 10,
    }
}

fn prover_market() -> ProverMarketParamsV1 {
    ProverMarketParamsV1 {
        version: ProverMarketParamsV1::VERSION,
        enabled: true,
        require_registered_identity: true,
        min_identity_bond_wei: 1_000,
        max_pending_submissions_per_prover: 1,
        max_range_blocks: 10,
    }
}

fn zone_metadata(timeout_blocks: u64, namespace: [u8; 8]) -> ExecutionZoneMetadataV1 {
    ExecutionZoneMetadataV1 {
        version: ExecutionZoneMetadataV1::VERSION,
        proof_system: 1,
        da_namespace: namespace,
        sequencer_policy: 1,
        forced_inclusion_timeout_masterchain_blocks: timeout_blocks,
    }
}

fn zone_ns(idx: usize) -> [u8; 8] {
    let mut namespace = *b"zone0000";
    namespace[7] = b'0' + idx as u8;
    namespace
}

#[test]
fn zone_creation_and_proof_final_update() {
    let mut ledger = MasterchainLedger::default();
    let metadata = zone_metadata(3, *b"zone0001");

    let zone = ledger
        .create_execution_zone(100, [0x11; 20], metadata.clone())
        .expect("zone created");

    assert_eq!(zone.zone_id, 100);
    assert_eq!(zone.metadata, metadata);
    assert_eq!(zone.latest_proof_final_height, 0);

    let updated = ledger
        .submit_zone_proof_final_update(ZoneProofFinalUpdateV1 {
            zone_id: 100,
            zone_block_height: 7,
            state_root: [0x22; 32],
            message_root: [0x33; 32],
            forced_inclusion_root: [0u8; 32],
            required_forced_inclusion_root: [0u8; 32],
            proof_digest: [0x44; 32],
            prover: [0x55; 20],
        })
        .expect("proof-final update");

    assert_eq!(updated.latest_proof_final_height, 7);
    assert_eq!(updated.latest_state_root, [0x22; 32]);
    assert_eq!(updated.latest_message_root, [0x33; 32]);
    assert_eq!(
        ledger
            .execution_zone(100)
            .unwrap()
            .latest_proof_final_height,
        7
    );
    let err = ledger
        .submit_zone_proof_final_update(ZoneProofFinalUpdateV1 {
            zone_id: 100,
            zone_block_height: 7,
            state_root: [0x66; 32],
            message_root: [0x77; 32],
            forced_inclusion_root: [0u8; 32],
            required_forced_inclusion_root: [0u8; 32],
            proof_digest: [0x88; 32],
            prover: [0x55; 20],
        })
        .unwrap_err();
    assert!(matches!(
        err,
        MasterchainError::StaleZoneUpdate {
            height: 7,
            current: 7
        }
    ));
}

#[test]
fn async_cross_zone_message_delivery_is_ordered_and_deduped() {
    let mut ledger = MasterchainLedger::default();
    ledger
        .create_execution_zone(1, [0x11; 20], zone_metadata(3, *b"zone0001"))
        .unwrap();
    ledger
        .create_execution_zone(2, [0x22; 20], zone_metadata(3, *b"zone0002"))
        .unwrap();

    let msg_b = AsyncCrossZoneMessageV1 {
        from_zone: 1,
        to_zone: 2,
        nonce: 2,
        payload_hash: [0xBB; 32],
        payload: vec![0xBB],
    };
    let msg_a = AsyncCrossZoneMessageV1 {
        from_zone: 1,
        to_zone: 2,
        nonce: 1,
        payload_hash: [0xAA; 32],
        payload: vec![0xAA],
    };
    ledger.submit_cross_zone_message(msg_b.clone()).unwrap();
    ledger.submit_cross_zone_message(msg_a.clone()).unwrap();
    ledger.submit_cross_zone_message(msg_a.clone()).unwrap();

    let delivered = ledger.drain_cross_zone_messages_for(2).unwrap();

    assert_eq!(delivered, vec![msg_a, msg_b]);
    assert!(ledger.drain_cross_zone_messages_for(2).unwrap().is_empty());
}

#[test]
fn forced_inclusion_materializes_after_sequencer_censorship_sla() {
    let mut ledger = MasterchainLedger::default();
    ledger
        .ingest_shard_anchor(anchor(0, 10, 1))
        .expect("anchor");
    ledger
        .create_execution_zone(9, [0x11; 20], zone_metadata(2, *b"zone0009"))
        .unwrap();

    let request = ledger
        .submit_forced_inclusion(9, [0xAA; 20], [0xCC; 32], vec![1, 2, 3])
        .expect("forced inclusion request");
    let queue_root = forced_inclusion_queue_root(std::slice::from_ref(&request));

    assert_eq!(request.submitted_at_masterchain_height, 0);
    assert_eq!(request.deadline_masterchain_height, 2);
    assert_eq!(ledger.pending_forced_inclusions().len(), 1);

    let block = ledger.seal_round([0u8; 20]).expect("round 1").unwrap();
    assert_eq!(block.forced_inclusion_queue_root, queue_root);
    assert!(ledger.forced_inclusion_events().is_empty());
    assert_eq!(ledger.pending_forced_inclusions().len(), 1);

    let block = ledger.seal_round([0u8; 20]).expect("round 2").unwrap();
    assert_eq!(block.forced_inclusion_queue_root, queue_root);
    assert!(ledger.pending_forced_inclusions().is_empty());
    assert_eq!(ledger.forced_inclusion_events().len(), 1);
    assert_eq!(
        ledger.forced_inclusion_events()[0],
        fractal_masterchain::ledger::ForcedInclusionEventV1 {
            version: fractal_masterchain::ledger::ForcedInclusionEventV1::VERSION,
            request,
            included_at_masterchain_height: 2,
            sequencer_late_by_blocks: 0,
        }
    );
}

#[test]
fn zone_finality_rejects_missing_forced_inclusion_after_timeout() {
    let mut ledger = MasterchainLedger::default();
    ledger
        .create_execution_zone(9, [0x11; 20], zone_metadata(1, *b"zone0009"))
        .unwrap();
    ledger
        .submit_forced_inclusion(9, [0xAA; 20], [0xCC; 32], vec![1, 2, 3])
        .unwrap();
    ledger.ingest_shard_anchor(anchor(0, 10, 1)).unwrap();
    ledger.seal_round([0u8; 20]).unwrap().unwrap();

    let err = ledger
        .submit_zone_proof_final_update(ZoneProofFinalUpdateV1 {
            zone_id: 9,
            zone_block_height: 1,
            state_root: [0x22; 32],
            message_root: [0x33; 32],
            forced_inclusion_root: [0u8; 32],
            required_forced_inclusion_root: [0u8; 32],
            proof_digest: [0x44; 32],
            prover: [0x55; 20],
        })
        .unwrap_err();

    assert!(matches!(err, MasterchainError::ForcedInclusionRootMismatch));
}

#[test]
fn zone_finality_accepts_satisfied_forced_inclusion_after_timeout() {
    let mut ledger = MasterchainLedger::default();
    ledger
        .create_execution_zone(9, [0x11; 20], zone_metadata(1, *b"zone0009"))
        .unwrap();
    let request = ledger
        .submit_forced_inclusion(9, [0xAA; 20], [0xCC; 32], vec![1, 2, 3])
        .unwrap();
    ledger.ingest_shard_anchor(anchor(0, 10, 1)).unwrap();
    ledger.seal_round([0u8; 20]).unwrap().unwrap();

    let required_root = ledger.required_forced_inclusion_root_for_zone(9);
    assert_eq!(
        required_root,
        forced_inclusion_queue_root(std::slice::from_ref(&request))
    );

    let updated = ledger
        .submit_zone_proof_final_update(ZoneProofFinalUpdateV1 {
            zone_id: 9,
            zone_block_height: 1,
            state_root: [0x22; 32],
            message_root: [0x33; 32],
            forced_inclusion_root: required_root,
            required_forced_inclusion_root: required_root,
            proof_digest: [0x44; 32],
            prover: [0x55; 20],
        })
        .unwrap();

    assert_eq!(updated.latest_proof_final_height, 1);
    assert!(ledger
        .proven_forced_inclusion_request_ids
        .contains(&request.request_id));
    assert_eq!(ledger.required_forced_inclusion_root_for_zone(9), [0u8; 32]);
    assert_eq!(ledger.forced_inclusion_queue_root(), [0u8; 32]);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ForcedInclusionStressReport {
    zones: usize,
    requests: usize,
    worst_inclusion_latency_blocks: u64,
    rejected_omitted_proofs: usize,
    accepted_satisfied_proofs: usize,
}

#[test]
fn multi_zone_forced_inclusion_liveness_with_delayed_proofs() {
    let mut ledger = MasterchainLedger::default();
    ledger.ingest_shard_anchor(anchor(0, 10, 1)).unwrap();

    let zone_count = 4usize;
    let requests_per_zone = 2usize;
    let mut requests = Vec::new();
    for idx in 0..zone_count {
        let zone_id = 100 + idx as u64;
        let timeout = 1 + idx as u64;
        ledger
            .create_execution_zone(
                zone_id,
                [0x10 + idx as u8; 20],
                zone_metadata(timeout, zone_ns(idx)),
            )
            .unwrap();
        for req_idx in 0..requests_per_zone {
            let mut tx_hash = [0u8; 32];
            tx_hash[0] = 0xA0 + idx as u8;
            tx_hash[1] = req_idx as u8;
            let request = ledger
                .submit_forced_inclusion(
                    zone_id,
                    [0xB0 + req_idx as u8; 20],
                    tx_hash,
                    vec![idx as u8, req_idx as u8],
                )
                .unwrap();
            requests.push(request);
        }
    }

    let max_deadline = requests
        .iter()
        .map(|request| request.deadline_masterchain_height)
        .max()
        .unwrap();
    let mut committed_queue_roots = Vec::new();
    while ledger.forced_inclusion_events().len() < zone_count.saturating_mul(requests_per_zone) {
        let block = ledger.seal_round([0u8; 20]).unwrap().unwrap();
        committed_queue_roots.push(block.forced_inclusion_queue_root);
        assert!(
            block.height <= max_deadline,
            "forced requests should materialize by their deadline"
        );
    }

    let mut worst_inclusion_latency_blocks = 0u64;
    for event in ledger.forced_inclusion_events() {
        let latency = event
            .included_at_masterchain_height
            .saturating_sub(event.request.deadline_masterchain_height);
        worst_inclusion_latency_blocks = worst_inclusion_latency_blocks.max(latency);
        assert_eq!(
            event.included_at_masterchain_height, event.request.deadline_masterchain_height,
            "forced inclusion must materialize at the first deadline block"
        );
    }
    assert!(committed_queue_roots.iter().all(|root| *root != [0u8; 32]));

    let mut rejected_omitted_proofs = 0usize;
    let mut accepted_satisfied_proofs = 0usize;
    for idx in 0..zone_count {
        let zone_id = 100 + idx as u64;
        let missing = ZoneProofFinalUpdateV1 {
            zone_id,
            zone_block_height: 1,
            state_root: [0x22; 32],
            message_root: [0x33; 32],
            forced_inclusion_root: [0u8; 32],
            required_forced_inclusion_root: [0u8; 32],
            proof_digest: [0x44; 32],
            prover: [0x55; 20],
        };
        assert!(matches!(
            ledger.submit_zone_proof_final_update(missing),
            Err(MasterchainError::ForcedInclusionRootMismatch)
        ));
        rejected_omitted_proofs += 1;

        let due_root = ledger.required_forced_inclusion_root_for_zone(zone_id);
        assert_ne!(due_root, [0u8; 32]);
        let accepted = ledger
            .submit_zone_proof_final_update(ZoneProofFinalUpdateV1 {
                zone_id,
                zone_block_height: 1,
                state_root: [0x22; 32],
                message_root: [0x33; 32],
                forced_inclusion_root: due_root,
                required_forced_inclusion_root: due_root,
                proof_digest: [0x44 + idx as u8; 32],
                prover: [0x55; 20],
            })
            .unwrap();
        assert_eq!(accepted.latest_proof_final_height, 1);
        accepted_satisfied_proofs += 1;
    }

    let report = ForcedInclusionStressReport {
        zones: zone_count,
        requests: requests.len(),
        worst_inclusion_latency_blocks,
        rejected_omitted_proofs,
        accepted_satisfied_proofs,
    };
    assert_eq!(
        report,
        ForcedInclusionStressReport {
            zones: 4,
            requests: 8,
            worst_inclusion_latency_blocks: 0,
            rejected_omitted_proofs: 4,
            accepted_satisfied_proofs: 4,
        }
    );
    assert_eq!(ledger.forced_inclusion_queue_root(), [0u8; 32]);
    assert_eq!(
        ledger.proven_forced_inclusion_request_ids.len(),
        zone_count * requests_per_zone
    );
}

#[test]
fn prover_market_requires_registered_bonded_identity() {
    let mut ledger = MasterchainLedger::default();
    ledger.set_prover_market(prover_market());
    ledger
        .ingest_shard_anchor(anchor(0, 10, 1))
        .expect("anchor");
    let sub = proof(1, 5, [0x31; 32]);

    let err = ledger.submit_validity_proof(sub.clone()).unwrap_err();
    assert!(matches!(err, MasterchainError::ProverIdentityRequired));

    let err = ledger
        .register_prover_identity(sub.prover, 999)
        .unwrap_err();
    assert!(matches!(err, MasterchainError::ProverBondTooLow { .. }));

    let id = ledger
        .register_prover_identity(sub.prover, 1_000)
        .expect("register");
    assert!(id.active);
    assert_eq!(id.prover, sub.prover);
    assert_eq!(ledger.prover_identity(&sub.prover), Some(&id));
    ledger.submit_validity_proof(sub).expect("registered");
}

#[test]
fn prover_market_applies_pending_and_range_anti_spam_limits() {
    let mut ledger = MasterchainLedger::default();
    ledger.set_prover_market(prover_market());
    ledger
        .register_prover_identity([0x44; 20], 1_000)
        .expect("register");
    ledger
        .ingest_shard_anchor(anchor(0, 20, 1))
        .expect("anchor");

    let too_large = proof(1, 11, [0x32; 32]);
    let err = ledger.submit_validity_proof(too_large).unwrap_err();
    assert!(matches!(err, MasterchainError::ProofRangeTooLarge { .. }));

    ledger
        .submit_validity_proof(proof(1, 5, [0x33; 32]))
        .expect("first pending");
    let err = ledger
        .submit_validity_proof(proof(6, 10, [0x34; 32]))
        .unwrap_err();
    assert!(matches!(err, MasterchainError::ProverPendingLimit));
}

#[test]
fn prover_reward_curve_scales_with_range_and_decays_with_lag() {
    let params = prover_economics();
    assert_eq!(
        prover_reward_wei(&params, &lagged_proof(1, 5, 0, [0x11; 32])),
        500
    );
    assert_eq!(
        prover_reward_wei(&params, &lagged_proof(1, 5, 10, [0x11; 32])),
        250
    );
    assert_eq!(
        prover_reward_wei(&params, &lagged_proof(1, 10, 10, [0x11; 32])),
        500
    );
}

#[test]
fn accepted_proofs_credit_prover_from_treasury() {
    let mut ledger = MasterchainLedger::default();
    ledger.set_prover_economics(prover_economics());
    ledger.fund_prover_treasury(1_000);
    ledger
        .ingest_shard_anchor(anchor(0, 10, 1))
        .expect("anchor");
    let sub = lagged_proof(1, 5, 10, [0x55; 32]);
    ledger.submit_validity_proof(sub.clone()).expect("queued");

    let block = ledger.seal_round([0u8; 20]).expect("seal").expect("block");

    assert_eq!(block.validity_proofs, vec![sub.clone()]);
    assert_eq!(ledger.prover_reward_credit(&sub.prover), 250);
    assert_eq!(ledger.treasury_balance_wei, 750);
    let events = ledger.prover_reward_events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].reward_wei, 250);
    assert_eq!(events[0].covered_blocks, 5);
    assert_eq!(events[0].lag_seconds, 10);
}

#[test]
fn prover_reward_is_capped_by_treasury_and_invalid_proofs_do_not_pay() {
    let mut ledger = MasterchainLedger::default();
    ledger.set_prover_economics(prover_economics());
    ledger.fund_prover_treasury(125);
    ledger
        .ingest_shard_anchor(anchor(0, 10, 1))
        .expect("anchor");
    let sub = lagged_proof(1, 5, 0, [0x56; 32]);
    ledger.submit_validity_proof(sub.clone()).expect("queued");
    ledger.seal_round([0u8; 20]).expect("seal").expect("block");
    assert_eq!(ledger.prover_reward_credit(&sub.prover), 125);
    assert_eq!(ledger.treasury_balance_wei, 0);

    let mut invalid = MasterchainLedger::default();
    invalid.set_prover_economics(prover_economics());
    invalid.fund_prover_treasury(1_000);
    let bad = proof(9, 3, [0x99; 32]);
    assert!(invalid.submit_validity_proof(bad.clone()).is_err());
    assert_eq!(invalid.prover_reward_credit(&bad.prover), 0);
    assert!(invalid.prover_reward_events().is_empty());
    assert_eq!(invalid.treasury_balance_wei, 1_000);
}

#[test]
fn invalid_submission_records_slashable_evidence() {
    let mut ledger = MasterchainLedger::default();
    ledger.set_proof_slashing_policy(slashing_policy());
    let bad = proof(9, 3, [0x99; 32]);
    assert!(ledger.submit_validity_proof(bad.clone()).is_err());
    let events = ledger.invalid_proof_slash_events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].reason_code, INVALID_PROOF_BAD_RANGE);
    assert_eq!(events[0].prover, bad.prover);
    assert_eq!(events[0].slash_amount_wei, 123);
    assert_ne!(events[0].evidence_hash, [0u8; 32]);
}

#[test]
fn invalid_submission_burns_registered_prover_bond_once() {
    let mut ledger = MasterchainLedger::default();
    ledger.set_proof_slashing_policy(slashing_policy());
    let bad = proof(9, 3, [0x99; 32]);
    ledger
        .register_prover_identity(bad.prover, 200)
        .expect("register");

    assert!(ledger.submit_validity_proof(bad.clone()).is_err());
    assert!(ledger.submit_validity_proof(bad.clone()).is_err());

    let id = ledger.prover_identity(&bad.prover).expect("identity");
    assert_eq!(id.bond_wei, 77);
    assert!(id.active);
    let events = ledger.invalid_proof_slash_events();
    assert_eq!(events.len(), 1, "duplicate evidence must not double burn");
    assert!(events[0].executed);
    assert_eq!(events[0].burned_bond_wei, 123);
    assert_eq!(events[0].bond_before_wei, 200);
    assert_eq!(events[0].bond_after_wei, 77);
    assert!(events[0].prover_active_after);
}

#[test]
fn invalid_submission_deactivates_prover_below_market_min_bond() {
    let mut ledger = MasterchainLedger::default();
    ledger.set_proof_slashing_policy(slashing_policy());
    ledger.set_prover_market(ProverMarketParamsV1 {
        min_identity_bond_wei: 1_000,
        max_pending_submissions_per_prover: 8,
        max_range_blocks: 10_000,
        ..prover_market()
    });
    let bad = proof(9, 3, [0x98; 32]);
    ledger
        .register_prover_identity(bad.prover, 1_100)
        .expect("register");

    assert!(ledger.submit_validity_proof(bad.clone()).is_err());

    let id = ledger.prover_identity(&bad.prover).expect("identity");
    assert_eq!(id.bond_wei, 977);
    assert!(!id.active);
    let events = ledger.invalid_proof_slash_events();
    assert_eq!(events.len(), 1);
    assert!(events[0].executed);
    assert_eq!(events[0].burned_bond_wei, 123);
    assert_eq!(events[0].bond_before_wei, 1_100);
    assert_eq!(events[0].bond_after_wei, 977);
    assert!(!events[0].prover_active_after);
}

#[test]
fn range_past_anchor_records_slashable_evidence() {
    let mut ledger = MasterchainLedger::default();
    ledger.set_proof_slashing_policy(slashing_policy());
    ledger
        .ingest_shard_anchor(anchor(0, 10, 1))
        .expect("anchor");
    let bad = proof(1, 20, [0x66; 32]);
    assert!(ledger.submit_validity_proof(bad).is_err());
    let events = ledger.invalid_proof_slash_events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].reason_code, INVALID_PROOF_RANGE_EXCEEDS_ANCHOR);
}

#[test]
fn mandatory_verified_stwo_missing_statement_records_slashable_evidence() {
    let mut ledger = MasterchainLedger::default();
    ledger.set_proof_slashing_policy(slashing_policy());
    ledger
        .ingest_shard_anchor(anchor(0, 10, 1))
        .expect("anchor");
    ledger
        .submit_validity_proof(proof(1, 10, [0x77; 32]))
        .expect("queued");
    assert!(ledger.seal_round([0u8; 20]).is_err());
    let events = ledger.invalid_proof_slash_events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].reason_code, INVALID_PROOF_MISSING_VERIFIED_STWO);
}

#[test]
#[cfg(feature = "runtime")]
fn bft7_masterchain_votes_form_qc_on_block() {
    let mut proposer = MasterchainBftNode::devnet_bft7(0);
    proposer.ingest_anchor(anchor(0, 4, 1)).expect("anchor");
    let block = proposer
        .try_produce_round()
        .expect("produce")
        .expect("block");
    assert!(
        proposer.last_formed_qc.is_none(),
        "one vote is below 5-of-7"
    );

    let mut formed = None;
    for idx in 1u32..5 {
        let voter = MasterchainBftNode::devnet_bft7(idx);
        let vote = voter.sign_vote_for_block(&block).expect("vote");
        formed = proposer.ingest_vote_for_block(&block, vote);
    }
    let qc = formed.expect("5 votes form QC");
    assert_eq!(qc.signer_indices, vec![0, 1, 2, 3, 4]);
    assert!(verify_masterchain_qc(
        &qc,
        &block,
        &proposer.validators,
        None
    ));
    assert_eq!(proposer.last_formed_qc, Some(qc));
}

#[test]
#[cfg(feature = "runtime")]
fn bft7_masterchain_timeouts_form_cert_and_advance_view() {
    let mut node = MasterchainBftNode::devnet_bft7(0);
    let high_qc = fractal_consensus::genesis_parent_qc();
    let mut cert = None;
    for idx in 0u32..5 {
        let voter = MasterchainBftNode::devnet_bft7(idx);
        let timeout = voter.sign_timeout(high_qc.clone()).expect("timeout");
        cert = node.ingest_timeout(timeout);
    }
    let cert = cert.expect("5 timeouts form certificate");
    assert_eq!(cert.signer_indices, vec![0, 1, 2, 3, 4]);
    assert!(verify_masterchain_timeout_cert(&cert, &node.validators));
    assert_eq!(node.view, 1);
    assert_eq!(node.last_timeout_cert, Some(cert));
}
