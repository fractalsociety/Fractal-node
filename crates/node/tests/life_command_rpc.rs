use std::sync::Arc;

use fractal_core::{NativeCall, TxBody};
use fractal_node::{try_produce_one_tick, NodeInner, ProduceTickOutcome};
use fractal_rpc::{
    build_module, ChainInteraction, RpcLifeCommandResponse, RpcSupplyResponse, SharedChain,
};
use tokio::sync::Mutex;

#[tokio::test]
async fn life_command_rpc_mines_and_indexes_events() {
    let rpc_ctx: SharedChain = Arc::new(Mutex::new(NodeInner::devnet()));
    let module = build_module(rpc_ctx);

    let submitted: RpcLifeCommandResponse = module
        .call(
            "fractal_submitLifeCommand",
            [serde_json::json!({
                "kind": "birth_grant",
                "soulId": "soul-life-1",
                "epoch": 1,
                "amountMicroCredits": 12345,
                "payload": { "class": "npc", "ownerAccountId": "owner-1" }
            })],
        )
        .await
        .expect("submit life command");
    assert!(submitted.transaction_hash.starts_with("0x"));
    assert_eq!(submitted.command.kind, "birth_grant");

    let node = Arc::new(Mutex::new(NodeInner::devnet()));
    let direct = {
        let mut guard = node.lock().await;
        guard
            .submit_life_command(fractal_core::LifeCommandV1 {
                command_id: decode_hash32(&submitted.command.command_id),
                kind: fractal_core::LifeCommandKind::BirthGrant,
                soul_id_hash: decode_hash32(&submitted.command.soul_id_hash),
                counterparty_hash: None,
                epoch: 1,
                amount_micro_credits: 12345,
                payload_hash: decode_hash32(&submitted.command.payload_hash),
            })
            .expect("direct submit")
    };
    assert!(matches!(
        try_produce_one_tick(&node).await,
        ProduceTickOutcome::Produced(1)
    ));

    let tx_hash = decode_hash32(&direct.transaction_hash);
    let guard = node.lock().await;
    let tx = guard.tx_by_hash(&tx_hash).expect("life tx mined");
    assert!(matches!(
        tx.body,
        TxBody::Native(NativeCall::LifeCommandV1(_))
    ));
    let fetched = guard
        .life_command_by_id(decode_hash32(&submitted.command.command_id))
        .expect("indexed life command");
    assert_eq!(fetched.kind, "birth_grant");
    let events = guard.list_life_events(Some("birth_grant"), None, 5);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].command_id, submitted.command.command_id);
}

#[tokio::test]
async fn life_command_rpc_accepts_extended_economic_command_kinds() {
    let rpc_ctx: SharedChain = Arc::new(Mutex::new(NodeInner::devnet()));
    let module = build_module(rpc_ctx);

    for kind in [
        "intelligence_payout",
        "provenance_bond",
        "feedback_artifact",
        "sealed_sale",
    ] {
        let submitted: RpcLifeCommandResponse = module
            .call(
                "fractal_submitLifeCommand",
                [serde_json::json!({
                    "kind": kind,
                    "soulId": format!("soul-{kind}"),
                    "epoch": 7,
                    "amountMicroCredits": 99,
                    "payload": { "source": "emission-task-11-20" }
                })],
            )
            .await
            .expect("submit extended life command");

        assert_eq!(submitted.command.kind, kind);
        assert!(submitted.command.command_id.starts_with("0x"));
        assert_eq!(submitted.command.amount_micro_credits, "99");
    }
}

#[tokio::test]
async fn supply_rpc_reports_protocol_cap_and_pools() {
    let rpc_ctx: SharedChain = Arc::new(Mutex::new(NodeInner::devnet()));
    let module = build_module(rpc_ctx);

    let supply: RpcSupplyResponse = module
        .call("fractal_getSupply", [serde_json::json!({})])
        .await
        .expect("get supply");

    assert_eq!(supply.block_number, "0x0");
    assert_eq!(
        supply.max_supply_wei,
        format!("0x{:x}", fractal_core::MAX_SUPPLY_WEI)
    );
    assert_eq!(supply.protocol_minted_wei, "0x0");
    assert_eq!(supply.protocol_burned_wei, "0x0");
    assert_eq!(supply.circulating_supply_wei, "0x0");
    assert_eq!(supply.provider_pool_wei, "0x0");
    assert_eq!(supply.consensus_pool_wei, "0x0");
    assert_eq!(supply.intelligence_pool_wei, "0x0");
}

fn decode_hash32(raw: &str) -> [u8; 32] {
    hex::decode(raw.trim_start_matches("0x"))
        .unwrap()
        .try_into()
        .unwrap()
}
