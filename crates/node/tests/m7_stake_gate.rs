//! PRD §12 / M7: optional producer gate on bonded consensus stake (`FRACTAL_MIN_CONSENSUS_STAKE_WEI`).

use std::sync::Arc;

use fractal_consensus::ValidatorSet;
use fractal_core::{NativeCall, Transaction, TxBody, VmKind};
use fractal_node::{
    try_produce_one_tick, NodeInner, ProduceTickOutcome, HARDHAT_DEFAULT_SIGNER_0,
};
use tokio::sync::Mutex;

#[tokio::test]
async fn singleton_waits_for_consensus_stake_when_min_configured() {
    let node = Arc::new(Mutex::new(NodeInner::devnet()));
    {
        let mut n = node.lock().await;
        n.min_consensus_stake_wei = 100;
    }
    assert_eq!(
        try_produce_one_tick(&node).await,
        ProduceTickOutcome::AwaitingConsensusStake
    );

    let fp = {
        let n = node.lock().await;
        n.validators.entry(0).unwrap().fingerprint
    };
    {
        let mut n = node.lock().await;
        let tx = Transaction {
            signer: HARDHAT_DEFAULT_SIGNER_0,
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::DepositConsensusStake {
                validator_fingerprint: fp,
                amount: 100,
            }),
        };
        n.state.apply_transaction(&tx).unwrap();
    }

    match try_produce_one_tick(&node).await {
        ProduceTickOutcome::Produced(1) => {}
        other => panic!("expected Produced(1) after bonding stake; got {other:?}"),
    }
}

#[tokio::test]
async fn min_stake_zero_default_still_produces() {
    let node = Arc::new(Mutex::new(NodeInner::devnet_with_validator_index(
        ValidatorSet::phase1_singleton(),
        0,
    )));
    match try_produce_one_tick(&node).await {
        ProduceTickOutcome::Produced(1) => {}
        other => panic!("expected Produced(1); got {other:?}"),
    }
}
