//! HyperBFT BFT-7 torture slice: minority partition + view-change under native tx load.

use fractal_consensus::{ConsensusMode, Timeout, TimeoutSignBody, ValidatorSet, header_hash};
use fractal_core::{NativeCall, Transaction, TxBody, VmKind};
use fractal_mempool::PooledTx;
use fractal_node::{HARDHAT_DEFAULT_SIGNER_0, NodeInner, ProduceTickOutcome};
use fractal_shard::ShardTopology;

const LIVE: [usize; 5] = [0, 1, 2, 3, 4];
const TXS_PER_BLOCK: u64 = 4;
const TARGET_COMMITS: usize = 12;
const TARGET_BLOCK_MS: u64 = 70;
const P99_FINALITY_BUDGET_MS: u64 = 900;

fn make_nodes() -> Vec<NodeInner> {
    let validators = ValidatorSet::phase2_bft7_fixture();
    (0..validators.len())
        .map(|idx| {
            let mut n = NodeInner::devnet_with_validator_index(validators.clone(), idx);
            n.consensus_mode = ConsensusMode::HyperBft;
            n.hyperbft_config.target_block_time_ms = TARGET_BLOCK_MS;
            n.hyperbft_config.pacemaker_base_ms = TARGET_BLOCK_MS;
            n.shard_topology = ShardTopology { shard_count: 2 };
            n.shard_id = 0;
            n.anchor_interval = 0;
            n
        })
        .collect()
}

fn insert_native_load(node: &mut NodeInner) {
    let start_nonce = node
        .state
        .accounts
        .get(&HARDHAT_DEFAULT_SIGNER_0)
        .map(|a| a.nonce)
        .unwrap_or(0);
    for offset in 0..TXS_PER_BLOCK {
        node.mempool.insert(PooledTx {
            tx: Transaction {
                signer: HARDHAT_DEFAULT_SIGNER_0,
                nonce: start_nonce + offset,
                vm: VmKind::Native,
                body: TxBody::Native(NativeCall::NoOp),
            },
            max_priority_fee_per_gas: u128::from(TXS_PER_BLOCK - offset),
            max_fee_per_gas: u128::MAX,
            eth_signed_raw: None,
        });
    }
}

fn gossip_votes(nodes: &mut [NodeInner], view: u64, height: u64, hh: [u8; 32]) {
    let votes: Vec<_> = LIVE
        .iter()
        .map(|idx| {
            nodes[*idx]
                .build_self_vote(view, height, hh)
                .expect("live validator has BLS key")
        })
        .collect();
    for vote in votes {
        for idx in LIVE {
            let _ = nodes[idx].record_vote(vote.clone());
        }
    }

    let formed = nodes[LIVE[0]]
        .try_form_qc(view, height, hh)
        .expect("five live validators satisfy BFT-7 quorum");
    for idx in LIVE {
        nodes[idx].high_prepare_qc = formed.qc.clone();
        nodes[idx].hyperbft_pipeline.note_formed_qc(&formed);
    }
}

fn gossip_timeout_for_view(nodes: &mut [NodeInner], view: u64) {
    let high_qc = nodes[LIVE[0]].high_prepare_qc.clone();
    let timeouts: Vec<_> = LIVE
        .iter()
        .map(|idx| {
            let sk = nodes[*idx]
                .validators
                .dev_bls_secret(*idx)
                .expect("live validator has dev BLS key");
            Timeout::sign(
                TimeoutSignBody {
                    view,
                    high_qc: high_qc.clone(),
                },
                *idx as u32,
                &sk,
            )
        })
        .collect();

    for timeout in timeouts {
        for idx in LIVE {
            let _ = nodes[idx].record_timeout(timeout.clone());
        }
    }
}

fn percentile_ms(mut values: Vec<u64>, pct: u64) -> u64 {
    values.sort_unstable();
    let len = values.len();
    let idx = ((len * pct as usize).saturating_add(99) / 100).saturating_sub(1);
    values[idx.min(len.saturating_sub(1))]
}

#[test]
fn hyperbft_bft7_minority_partition_view_change_under_load_keeps_p99_finality_budget() {
    let mut nodes = make_nodes();
    let mut skipped_views = Vec::new();
    let mut finality_ms = Vec::new();
    let mut ticks_since_last_commit = 0u64;

    for _ in 0..200 {
        if finality_ms.len() >= TARGET_COMMITS {
            break;
        }

        let view = nodes[LIVE[0]].view;
        let height = nodes[LIVE[0]].height;
        let head = nodes[LIVE[0]].head_hash;
        for idx in LIVE {
            assert_eq!(nodes[idx].view, view, "live validator {idx} view");
            assert_eq!(nodes[idx].height, height, "live validator {idx} height");
            assert_eq!(nodes[idx].head_hash, head, "live validator {idx} head");
        }

        let leader_idx = nodes[LIVE[0]].validators.leader_index(view);
        if !LIVE.contains(&leader_idx) {
            gossip_timeout_for_view(&mut nodes, view);
            for idx in LIVE {
                nodes[idx].try_advance_view_on_timeout_quorum();
                assert_eq!(nodes[idx].view, view + 1, "validator {idx} advanced view");
            }
            skipped_views.push(view);
            ticks_since_last_commit += 1;
            continue;
        }

        insert_native_load(&mut nodes[leader_idx]);
        ticks_since_last_commit += 1;
        let proposed_height = match nodes[leader_idx].hyperbft_three_stage_tick() {
            ProduceTickOutcome::Pipelined(h) => h,
            other => {
                panic!("live leader {leader_idx} should propose at view {view}, got {other:?}")
            }
        };
        assert_eq!(proposed_height, height + 1);

        let block = nodes[leader_idx]
            .three_stage
            .vote
            .as_ref()
            .expect("proposal should enter vote stage")
            .block
            .clone();
        assert_eq!(block.transactions.len(), TXS_PER_BLOCK as usize);
        assert_eq!(block.header.view, view);
        let hh = header_hash(&block.header).expect("header hash");

        for idx in LIVE {
            if idx == leader_idx {
                continue;
            }
            nodes[idx]
                .apply_synced_block(&block)
                .expect("live validator applies leader proposal");
        }

        gossip_votes(&mut nodes, block.header.view, block.header.height, hh);

        let mut committed = false;
        for _ in 0..8 {
            ticks_since_last_commit += 1;
            match nodes[leader_idx].hyperbft_three_stage_tick() {
                ProduceTickOutcome::Produced(h) if h == block.header.height => {
                    committed = true;
                    break;
                }
                ProduceTickOutcome::Pipelined(_)
                | ProduceTickOutcome::NotMyTurn
                | ProduceTickOutcome::AwaitingParentQc => {}
                other => panic!("unexpected leader tick while committing: {other:?}"),
            }
        }
        assert!(
            committed,
            "leader {leader_idx} should commit after quorum votes"
        );

        let committed_ms = ticks_since_last_commit * TARGET_BLOCK_MS;
        finality_ms.push(committed_ms);
        ticks_since_last_commit = 0;

        for idx in LIVE {
            assert_eq!(
                nodes[idx].height, block.header.height,
                "validator {idx} height"
            );
            assert_eq!(nodes[idx].head_hash, hh, "validator {idx} head");
            assert_eq!(
                nodes[idx].view,
                block.header.view + 1,
                "validator {idx} view"
            );
        }
    }

    assert!(
        skipped_views.iter().any(|v| *v % 7 == 5) && skipped_views.iter().any(|v| *v % 7 == 6),
        "expected partitioned validators 5 and 6 to force view changes, got {skipped_views:?}"
    );
    assert_eq!(finality_ms.len(), TARGET_COMMITS);

    let p99 = percentile_ms(finality_ms.clone(), 99);
    assert!(
        p99 <= P99_FINALITY_BUDGET_MS,
        "synthetic p99 finality {p99} ms exceeds {P99_FINALITY_BUDGET_MS} ms; samples={finality_ms:?}"
    );

    let committed_txs: usize = nodes[LIVE[0]]
        .blocks
        .iter()
        .map(|b| b.transactions.len())
        .sum();
    assert_eq!(committed_txs, TARGET_COMMITS * TXS_PER_BLOCK as usize);
}
