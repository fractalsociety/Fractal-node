use std::sync::Arc;

use borsh::BorshDeserialize;
use fractal_consensus::BlockPayload;
use fractal_core::{
    NativeCall, OwnedObjectCertificate, OwnedObjectVersion, Transaction, TxBody, VmKind,
    HARDHAT_DEFAULT_SIGNER_0,
};
use fractal_node::{try_produce_one_tick, BlockPayloadMode, NodeInner, ProduceTickOutcome};
use fractal_rpc::{
    build_module, ChainInteraction, RpcOwnedObjectCertificate, RpcOwnedObjectCountersignature,
    RpcOwnedObjectPrecheck, SharedChain,
};
use tokio::sync::Mutex;

fn hex_bytes(s: &str) -> Vec<u8> {
    hex::decode(s.strip_prefix("0x").unwrap_or(s)).expect("hex")
}

#[tokio::test]
async fn owned_object_certificate_rpc_round_trip() {
    let ctx: SharedChain = Arc::new(Mutex::new(NodeInner::devnet()));
    let module = build_module(ctx);

    let tx = Transaction {
        signer: HARDHAT_DEFAULT_SIGNER_0,
        nonce: 0,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::NoOp),
    };
    let raw = borsh::to_vec(&tx).expect("tx borsh");
    let raw_hex = format!("0x{}", hex::encode(&raw));

    let precheck: RpcOwnedObjectPrecheck = module
        .call("fractal_ownedObjectPrecheck", (raw_hex.clone(), "0x1"))
        .await
        .expect("precheck");
    assert_eq!(
        precheck.owner,
        format!("0x{}", hex::encode(HARDHAT_DEFAULT_SIGNER_0))
    );
    assert!(!precheck.object_versions.is_empty());

    let countersig: RpcOwnedObjectCountersignature = module
        .call("fractal_countersignOwnedObjectTx", (raw_hex.clone(), "0x1"))
        .await
        .expect("countersign");
    assert_eq!(countersig.validator_index, "0x0");

    let cert_response: RpcOwnedObjectCertificate = module
        .call(
            "fractal_aggregateOwnedObjectCertificate",
            (
                raw_hex,
                precheck.object_versions_borsh,
                vec![countersig.signature_borsh],
            ),
        )
        .await
        .expect("aggregate");
    assert_eq!(cert_response.signer_indices, vec!["0x0"]);

    let cert = OwnedObjectCertificate::try_from_slice(&hex_bytes(&cert_response.certificate_borsh))
        .expect("certificate borsh");
    assert_eq!(cert.signer_indices, vec![0]);
    assert_eq!(
        cert_response.certificate_hash,
        format!("0x{}", hex::encode(cert.certificate_hash().unwrap()))
    );
}

#[test]
fn owned_object_certificate_pool_gives_direct_finality_and_batch_root_hook() {
    let mut node = NodeInner::devnet();
    let tx = Transaction {
        signer: HARDHAT_DEFAULT_SIGNER_0,
        nonce: 0,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::NoOp),
    };
    let raw = borsh::to_vec(&tx).expect("tx borsh");
    let precheck = node
        .owned_object_precheck(&raw, 1)
        .expect("precheck for cert");
    let countersig = node
        .countersign_owned_object_tx(&raw, 1)
        .expect("countersign");
    let cert_response = node
        .aggregate_owned_object_certificate(
            &raw,
            &hex_bytes(&precheck.object_versions_borsh),
            vec![hex_bytes(&countersig.signature_borsh)],
        )
        .expect("aggregate");
    let cert = OwnedObjectCertificate::try_from_slice(&hex_bytes(&cert_response.certificate_borsh))
        .expect("certificate borsh");
    let object_versions =
        Vec::<OwnedObjectVersion>::try_from_slice(&hex_bytes(&precheck.object_versions_borsh))
            .expect("object versions");
    let empty_root = node
        .certificate_batch_payload_root_hook()
        .expect("empty root");

    let cert_hash = node
        .submit_owned_object_certificate(cert)
        .expect("certificate accepted");

    let finality = node
        .owned_object_certificate_finality(&object_versions[0])
        .expect("object final");
    assert_eq!(finality.certificate_hash, cert_hash);
    assert_ne!(
        node.certificate_batch_payload_root_hook()
            .expect("batch root"),
        empty_root
    );
    assert_eq!(
        node.owned_object_finality(&object_versions[0])
            .expect("rpc finality")
            .0,
        format!("0x{}", hex::encode(cert_hash))
    );
}

#[tokio::test]
async fn proof_ingestion_block_commits_certificate_batch_without_replay() {
    let mut node = NodeInner::devnet();
    let tx = Transaction {
        signer: HARDHAT_DEFAULT_SIGNER_0,
        nonce: 0,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::NoOp),
    };
    let raw = borsh::to_vec(&tx).expect("tx borsh");
    let precheck = node
        .owned_object_precheck(&raw, 1)
        .expect("precheck for cert");
    let countersig = node
        .countersign_owned_object_tx(&raw, 1)
        .expect("countersign");
    let cert_response = node
        .aggregate_owned_object_certificate(
            &raw,
            &hex_bytes(&precheck.object_versions_borsh),
            vec![hex_bytes(&countersig.signature_borsh)],
        )
        .expect("aggregate");
    let cert = OwnedObjectCertificate::try_from_slice(&hex_bytes(&cert_response.certificate_borsh))
        .expect("certificate borsh");
    node.submit_owned_object_certificate(cert.clone())
        .expect("certificate accepted");
    node.set_block_payload_mode(BlockPayloadMode::ProofIngestion);
    let handle = Arc::new(Mutex::new(node));

    assert_eq!(
        try_produce_one_tick(&handle).await,
        ProduceTickOutcome::Produced(1)
    );

    let node = handle.lock().await;
    let block = node
        .block_by_hash(&node.block_hash_by_number(1).unwrap())
        .unwrap();
    assert!(block.transactions.is_empty());
    assert_eq!(block.header.gas_used, 0);
    fractal_consensus::verify_da_sidecar(&block.header, &block.da_sidecar).unwrap();
    let payload_bytes = fractal_consensus::reconstruct_da_payload(&block.da_sidecar).unwrap();
    assert_eq!(
        BlockPayload::try_from_slice(&payload_bytes).unwrap(),
        BlockPayload::CertificateBatches(vec![fractal_consensus::OwnedObjectCertificateBatchV1 {
            certificates: vec![cert],
        }])
    );
}
