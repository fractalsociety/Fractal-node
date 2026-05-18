//! HyperBFT BFT-7 rotates real validator signers; no synthetic quorum injection.

use fractal_consensus::{header_hash, ConsensusMode, ValidatorSet};
use fractal_node::{NodeInner, ProduceTickOutcome};
use fractal_shard::ShardTopology;

fn make_nodes() -> Vec<NodeInner> {
    let validators = ValidatorSet::phase2_bft7_fixture();
    (0..validators.len())
        .map(|idx| {
            let mut n = NodeInner::devnet_with_validator_index(validators.clone(), idx);
            n.consensus_mode = ConsensusMode::HyperBft;
            n.shard_topology = ShardTopology { shard_count: 2 };
            n.shard_id = 0;
            n.anchor_interval = 0;
            n
        })
        .collect()
}

fn gossip_votes_for_block(nodes: &mut [NodeInner], view: u64, height: u64, hh: [u8; 32]) {
    let votes: Vec<_> = nodes
        .iter()
        .map(|n| {
            n.build_self_vote(view, height, hh)
                .expect("dev validator has BLS key")
        })
        .collect();
    for vote in votes {
        for node in nodes.iter_mut() {
            let _ = node.record_vote(vote.clone());
        }
    }
}

#[test]
fn hyperbft_bft7_rotates_leaders_with_gossiped_votes() {
    let mut nodes = make_nodes();
    let rounds = 4;

    for _ in 0..rounds {
        let view = nodes[0].view;
        let height = nodes[0].height;
        for n in &nodes {
            assert_eq!(
                n.height, height,
                "all validators start round at same height"
            );
            assert_eq!(n.view, view, "all validators start round at same view");
        }

        let leader_idx = nodes[0].validators.leader_index(view);
        let outcome = nodes[leader_idx].hyperbft_three_stage_tick();
        let proposed_height = match outcome {
            ProduceTickOutcome::Pipelined(h) => h,
            other => panic!("leader {leader_idx} should pipeline a proposal, got {other:?}"),
        };
        assert_eq!(proposed_height, height + 1);

        let block = nodes[leader_idx]
            .three_stage
            .vote
            .as_ref()
            .expect("proposal should be in vote stage")
            .block
            .clone();
        assert_eq!(block.header.view, view);
        assert_eq!(
            block.header.proposer,
            nodes[0].validators.expected_proposer(view)
        );
        let hh = header_hash(&block.header).expect("header hash");

        for (idx, node) in nodes.iter_mut().enumerate() {
            if idx == leader_idx {
                continue;
            }
            node.apply_synced_block(&block)
                .expect("non-leader validates and applies leader block");
        }

        gossip_votes_for_block(&mut nodes, block.header.view, block.header.height, hh);

        let mut committed = false;
        for _ in 0..3 {
            if nodes[leader_idx].hyperbft_three_stage_tick()
                == ProduceTickOutcome::Produced(block.header.height)
            {
                committed = true;
                break;
            }
        }
        assert!(
            committed,
            "leader {leader_idx} should commit after vote quorum"
        );

        for n in &nodes {
            assert_eq!(n.height, block.header.height);
            assert_eq!(n.head_hash, hh);
            assert_eq!(n.view, block.header.view + 1);
        }
    }
}
