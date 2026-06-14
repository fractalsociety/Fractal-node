//! Followers must rebuild RPC tx index state (`mined_txs`, `eth_signed_raw`, hash maps) from synced
//! blocks — see `.cursor/scratchpad.md` Wallet infra / M4 polish.

use fractal_consensus::{
    coverage_manifest_digest, coverage_manifest_for_circuit_version, eth_signed_raws_for_txs,
    execute_and_build_block, header_hash, mixed_execution_witness_from_replay,
    mixed_execution_witness_metadata, mixed_intrablock_aggregate_proof_envelope_v1,
    native_recursive_proof_envelope_v1, native_state_transition_statement_v1,
    validity_proof_public_input_digest, BlockValidityProof, CircuitVersion, ExecutionFeatureSetV1,
    ValidityProofSystem, WitnessRetentionPolicyV1,
};
use fractal_core::{NativeCall, Transaction, TxBody, VmKind};
use fractal_crypto::hash::keccak256;
use fractal_node::{NodeInner, SettlementAccessError, SyncApplyError, HARDHAT_DEFAULT_SIGNER_0};
use fractal_storage::ProofFinalityStore;

fn native_transition_witness(
    mut witness: fractal_consensus::MixedExecutionWitnessV1,
) -> fractal_consensus::MixedExecutionWitnessV1 {
    witness.public_inputs.circuit_version = CircuitVersion::NativeStateTransitionV1;
    witness.public_inputs.coverage_manifest_digest = coverage_manifest_digest(
        &coverage_manifest_for_circuit_version(CircuitVersion::NativeStateTransitionV1),
    )
    .unwrap();
    witness
}

fn native_recursive_block_proof(
    block: &fractal_consensus::Block,
    witness: &fractal_consensus::MixedExecutionWitnessV1,
) -> BlockValidityProof {
    let mut proof = BlockValidityProof {
        chain_id: block.header.chain_id,
        height: block.header.height,
        block_hash: header_hash(&block.header).unwrap(),
        timestamp_ms: block.header.timestamp_ms,
        parent_state_root: block.header.parent_state_root,
        state_root: block.header.state_root,
        tx_root: block.header.tx_root,
        receipt_root: block.header.receipt_root,
        native_event_root: block.header.native_event_root,
        evm_log_root: block.header.evm_log_root,
        gas_used: block.header.gas_used,
        zone_namespace: block.header.zone_namespace,
        da_root: block.header.da_root,
        circuit_version: CircuitVersion::NativeStateTransitionV1,
        coverage_manifest_digest: coverage_manifest_digest(&coverage_manifest_for_circuit_version(
            CircuitVersion::NativeStateTransitionV1,
        ))
        .unwrap(),
        feature_set: block.header.feature_set,
        proof_system: ValidityProofSystem::StwoPlonky2,
        proof_bytes: Vec::new(),
    };
    let statement = native_state_transition_statement_v1(witness).unwrap();
    proof.proof_bytes =
        borsh::to_vec(&native_recursive_proof_envelope_v1(statement, &proof, [0x44; 32]).unwrap())
            .unwrap();
    proof
}

fn mixed_transition_witness(
    mut witness: fractal_consensus::MixedExecutionWitnessV1,
) -> fractal_consensus::MixedExecutionWitnessV1 {
    witness.public_inputs.circuit_version = CircuitVersion::MixedStateTransitionV1;
    witness.public_inputs.coverage_manifest_digest = coverage_manifest_digest(
        &coverage_manifest_for_circuit_version(CircuitVersion::MixedStateTransitionV1),
    )
    .unwrap();
    witness
}

fn mixed_aggregate_block_proof(
    block: &fractal_consensus::Block,
    witness: &fractal_consensus::MixedExecutionWitnessV1,
) -> BlockValidityProof {
    let mut proof = BlockValidityProof {
        chain_id: block.header.chain_id,
        height: block.header.height,
        block_hash: header_hash(&block.header).unwrap(),
        timestamp_ms: block.header.timestamp_ms,
        parent_state_root: block.header.parent_state_root,
        state_root: block.header.state_root,
        tx_root: block.header.tx_root,
        receipt_root: block.header.receipt_root,
        native_event_root: block.header.native_event_root,
        evm_log_root: block.header.evm_log_root,
        gas_used: block.header.gas_used,
        zone_namespace: block.header.zone_namespace,
        da_root: block.header.da_root,
        circuit_version: CircuitVersion::MixedStateTransitionV1,
        coverage_manifest_digest: coverage_manifest_digest(&coverage_manifest_for_circuit_version(
            CircuitVersion::MixedStateTransitionV1,
        ))
        .unwrap(),
        feature_set: block.header.feature_set,
        proof_system: ValidityProofSystem::StwoPlonky2,
        proof_bytes: Vec::new(),
    };
    proof.proof_bytes =
        borsh::to_vec(&mixed_intrablock_aggregate_proof_envelope_v1(witness).unwrap()).unwrap();
    proof
}

#[test]
fn apply_synced_block_fills_mined_txs_for_native_noop() {
    let mut n = NodeInner::devnet();
    let tx = Transaction {
        signer: HARDHAT_DEFAULT_SIGNER_0,
        nonce: 0,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::NoOp),
    };
    let mut scratch = n.state.clone();
    let block = execute_and_build_block(
        n.chain_id,
        1,
        n.view,
        n.head_hash,
        n.parent_qc_hash,
        n.validators.expected_proposer(n.view),
        1,
        n.gas_limit,
        &mut scratch,
        vec![tx],
        eth_signed_raws_for_txs(1),
    )
    .expect("block");

    n.apply_synced_block(&block).expect("apply");

    let ih = keccak256(&borsh::to_vec(&block.transactions[0]).expect("borsh"));
    assert!(
        n.mined_txs.contains_key(&ih),
        "follower should index mined tx by internal hash"
    );
    assert_eq!(n.height, 1);
}

#[test]
fn apply_synced_block_rejects_eth_raw_length_mismatch() {
    let mut n = NodeInner::devnet();
    let tx = Transaction {
        signer: HARDHAT_DEFAULT_SIGNER_0,
        nonce: 0,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::NoOp),
    };
    let mut scratch = n.state.clone();
    let mut block = execute_and_build_block(
        n.chain_id,
        1,
        n.view,
        n.head_hash,
        n.parent_qc_hash,
        n.validators.expected_proposer(n.view),
        1,
        n.gas_limit,
        &mut scratch,
        vec![tx],
        eth_signed_raws_for_txs(1),
    )
    .expect("block");
    block.eth_signed_raw.push(None);
    assert!(n.apply_synced_block(&block).is_err());
}

#[test]
fn apply_synced_block_rejects_bad_da_sidecar() {
    let mut n = NodeInner::devnet();
    let tx = Transaction {
        signer: HARDHAT_DEFAULT_SIGNER_0,
        nonce: 0,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::NoOp),
    };
    let mut scratch = n.state.clone();
    let mut block = execute_and_build_block(
        n.chain_id,
        1,
        n.view,
        n.head_hash,
        n.parent_qc_hash,
        n.validators.expected_proposer(n.view),
        1,
        n.gas_limit,
        &mut scratch,
        vec![tx],
        eth_signed_raws_for_txs(1),
    )
    .expect("block");
    block.da_sidecar.shares[0].data[0] ^= 0xff;

    assert!(matches!(
        n.apply_synced_block(&block),
        Err(SyncApplyError::DataAvailability)
    ));
}

#[test]
fn apply_synced_block_rejects_da_payload_mismatch() {
    let mut n = NodeInner::devnet();
    let tx = Transaction {
        signer: HARDHAT_DEFAULT_SIGNER_0,
        nonce: 0,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::NoOp),
    };
    let mut scratch = n.state.clone();
    let mut block = execute_and_build_block(
        n.chain_id,
        1,
        n.view,
        n.head_hash,
        n.parent_qc_hash,
        n.validators.expected_proposer(n.view),
        1,
        n.gas_limit,
        &mut scratch,
        vec![tx],
        eth_signed_raws_for_txs(1),
    )
    .expect("block");
    block.transactions.clear();
    block.eth_signed_raw.clear();

    assert!(matches!(
        n.apply_synced_block(&block),
        Err(SyncApplyError::DataAvailability)
    ));
    assert_eq!(n.da_metrics.reconstruction_failure, 1);
}

#[test]
fn da_share_lookup_and_sample_verification_round_trip() {
    let mut n = NodeInner::devnet();
    let tx = Transaction {
        signer: HARDHAT_DEFAULT_SIGNER_0,
        nonce: 0,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::NoOp),
    };
    let mut scratch = n.state.clone();
    let block = execute_and_build_block(
        n.chain_id,
        1,
        n.view,
        n.head_hash,
        n.parent_qc_hash,
        n.validators.expected_proposer(n.view),
        1,
        n.gas_limit,
        &mut scratch,
        vec![tx],
        eth_signed_raws_for_txs(1),
    )
    .expect("block");
    let block_hash = header_hash(&block.header).unwrap();

    n.apply_synced_block(&block).expect("apply");
    let indexes = NodeInner::da_sample_indexes_for_block(&block, 4, 41);
    let shares = n
        .da_shares_by_block_hash(&block_hash, &indexes)
        .expect("shares");

    NodeInner::verify_da_sampled_shares(&block, &indexes, &shares).expect("samples verify");
}

#[test]
fn da_metrics_track_committed_bytes_fees_and_sampling() {
    let mut n = NodeInner::devnet();
    let tx = Transaction {
        signer: HARDHAT_DEFAULT_SIGNER_0,
        nonce: 0,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::NoOp),
    };
    let mut scratch = n.state.clone();
    let block = execute_and_build_block(
        n.chain_id,
        1,
        n.view,
        n.head_hash,
        n.parent_qc_hash,
        n.validators.expected_proposer(n.view),
        1,
        n.gas_limit,
        &mut scratch,
        vec![tx],
        eth_signed_raws_for_txs(1),
    )
    .expect("block");

    n.apply_synced_block(&block).expect("apply");
    n.record_da_sampling_result(true);
    n.record_da_sampling_result(false);

    assert_eq!(n.da_metrics.committed_blocks, 1);
    assert_eq!(n.da_metrics.committed_original_bytes, block.header.da_bytes);
    assert_eq!(n.da_metrics.committed_da_gas, block.header.da_gas_used);
    assert_eq!(n.da_metrics.da_fee_revenue, block.header.da_fee_paid);
    assert_eq!(n.da_metrics.sampling_success, 1);
    assert_eq!(n.da_metrics.sampling_failure, 1);
    assert_eq!(n.da_metrics.reconstruction_success, 1);
}

#[test]
fn da_sample_verification_rejects_tampered_network_share() {
    let mut n = NodeInner::devnet();
    let tx = Transaction {
        signer: HARDHAT_DEFAULT_SIGNER_0,
        nonce: 0,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::NoOp),
    };
    let mut scratch = n.state.clone();
    let block = execute_and_build_block(
        n.chain_id,
        1,
        n.view,
        n.head_hash,
        n.parent_qc_hash,
        n.validators.expected_proposer(n.view),
        1,
        n.gas_limit,
        &mut scratch,
        vec![tx],
        eth_signed_raws_for_txs(1),
    )
    .expect("block");
    let block_hash = header_hash(&block.header).unwrap();

    n.apply_synced_block(&block).expect("apply");
    let indexes = NodeInner::da_sample_indexes_for_block(&block, 4, 41);
    let mut shares = n
        .da_shares_by_block_hash(&block_hash, &indexes)
        .expect("shares");
    shares[0].data[0] ^= 0xff;

    assert!(NodeInner::verify_da_sampled_shares(&block, &indexes, &shares).is_err());
}

#[test]
fn apply_synced_block_rejects_bad_parent_qc_hash() {
    let mut n = NodeInner::devnet();
    let tx = Transaction {
        signer: HARDHAT_DEFAULT_SIGNER_0,
        nonce: 0,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::NoOp),
    };
    let mut scratch = n.state.clone();
    let block = execute_and_build_block(
        n.chain_id,
        1,
        n.view,
        n.head_hash,
        [0u8; 32],
        n.validators.expected_proposer(n.view),
        1,
        n.gas_limit,
        &mut scratch,
        vec![tx],
        eth_signed_raws_for_txs(1),
    )
    .expect("block");
    assert!(matches!(
        n.apply_synced_block(&block),
        Err(SyncApplyError::ParentQcHash)
    ));
}

#[test]
fn apply_synced_block_rejects_invalid_proposer() {
    let mut n = NodeInner::devnet();
    let tx = Transaction {
        signer: HARDHAT_DEFAULT_SIGNER_0,
        nonce: 0,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::NoOp),
    };
    let mut scratch = n.state.clone();
    let block = execute_and_build_block(
        n.chain_id,
        1,
        n.view,
        n.head_hash,
        n.parent_qc_hash,
        [0xfe; 32],
        1,
        n.gas_limit,
        &mut scratch,
        vec![tx],
        eth_signed_raws_for_txs(1),
    )
    .expect("block");
    assert!(matches!(
        n.apply_synced_block(&block),
        Err(SyncApplyError::InvalidProposer)
    ));
}

#[test]
fn devnet_with_bft7_fixture_has_seven_validators() {
    let n =
        NodeInner::devnet_with_validators(fractal_consensus::ValidatorSet::phase2_bft7_fixture());
    assert_eq!(n.validators.len(), 7);
}

#[test]
fn validity_proof_promotes_committed_block_from_soft_to_proof_final() {
    let mut n = NodeInner::devnet();
    let tx = Transaction {
        signer: HARDHAT_DEFAULT_SIGNER_0,
        nonce: 0,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::NoOp),
    };
    let mut scratch = n.state.clone();
    let block = execute_and_build_block(
        n.chain_id,
        1,
        n.view,
        n.head_hash,
        n.parent_qc_hash,
        n.validators.expected_proposer(n.view),
        1,
        n.gas_limit,
        &mut scratch,
        vec![tx],
        eth_signed_raws_for_txs(1),
    )
    .expect("block");
    let block_hash = header_hash(&block.header).unwrap();

    n.apply_synced_block(&block).expect("apply");
    assert_eq!(
        n.finality_for_block_hash(&block_hash),
        Some(fractal_node::BlockFinality::Soft)
    );

    let mut proof = BlockValidityProof {
        chain_id: block.header.chain_id,
        height: block.header.height,
        block_hash,
        timestamp_ms: block.header.timestamp_ms,
        parent_state_root: block.header.parent_state_root,
        state_root: block.header.state_root,
        tx_root: block.header.tx_root,
        receipt_root: block.header.receipt_root,
        native_event_root: block.header.native_event_root,
        evm_log_root: block.header.evm_log_root,
        gas_used: block.header.gas_used,
        zone_namespace: block.header.zone_namespace,
        da_root: block.header.da_root,
        circuit_version: CircuitVersion::DevMixedV1,
        coverage_manifest_digest: coverage_manifest_digest(&coverage_manifest_for_circuit_version(
            CircuitVersion::DevMixedV1,
        ))
        .unwrap(),
        feature_set: block.header.feature_set,
        proof_system: ValidityProofSystem::DevDigest,
        proof_bytes: Vec::new(),
    };
    proof.proof_bytes = validity_proof_public_input_digest(&proof).unwrap().to_vec();

    n.submit_validity_proof(proof).expect("proof accepted");
    assert_eq!(
        n.finality_for_block_hash(&block_hash),
        Some(fractal_node::BlockFinality::Proof)
    );
    assert_eq!(n.proof_metrics.proofs_accepted, 1);
    assert_eq!(n.proof_metrics.proof_final_height, block.header.height);
    assert!(n.proof_metrics.latest_proof_latency_ms > 0);
}

#[test]
fn soft_final_native_block_becomes_proof_final_after_valid_native_proof() {
    let mut n = NodeInner::devnet();
    n.set_native_transition_proofs_enabled(true);
    n.set_proofs_required_for_settlement(ExecutionFeatureSetV1 {
        bits: fractal_consensus::FEATURE_NATIVE_TX | fractal_consensus::FEATURE_NATIVE_SHARED_STATE,
    });
    let tx = Transaction {
        signer: HARDHAT_DEFAULT_SIGNER_0,
        nonce: 0,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::NoOp),
    };
    let pre_state = n.state.clone();
    let mut scratch = n.state.clone();
    let block = execute_and_build_block(
        n.chain_id,
        1,
        n.view,
        n.head_hash,
        n.parent_qc_hash,
        n.validators.expected_proposer(n.view),
        1,
        n.gas_limit,
        &mut scratch,
        vec![tx],
        eth_signed_raws_for_txs(1),
    )
    .expect("block");
    let block_hash = header_hash(&block.header).unwrap();
    let witness =
        native_transition_witness(mixed_execution_witness_from_replay(&block, &pre_state).unwrap());
    n.record_witness_metadata(
        mixed_execution_witness_metadata(&witness, WitnessRetentionPolicyV1::MetadataOnly).unwrap(),
    );

    n.apply_synced_block(&block).expect("apply");
    assert_eq!(
        n.finality_for_block_hash(&block_hash),
        Some(fractal_node::BlockFinality::Soft)
    );

    let proof = native_recursive_block_proof(&block, &witness);
    n.submit_validity_proof(proof)
        .expect("native proof accepted");
    assert_eq!(
        n.finality_for_block_hash(&block_hash),
        Some(fractal_node::BlockFinality::Proof)
    );
    assert_eq!(
        n.settlement_finality_for_block_hash(&block_hash),
        Ok(fractal_node::BlockFinality::Proof)
    );
    let stored = n.proof_finalized_blocks.get(&block_hash).unwrap();
    assert_eq!(
        stored.circuit_version,
        CircuitVersion::NativeStateTransitionV1
    );
    assert_eq!(
        stored.coverage_manifest_digest,
        coverage_manifest_digest(&coverage_manifest_for_circuit_version(
            CircuitVersion::NativeStateTransitionV1
        ))
        .unwrap()
    );
}

#[test]
fn native_proof_rejects_matching_public_inputs_with_wrong_witness_digest() {
    let mut n = NodeInner::devnet();
    n.set_native_transition_proofs_enabled(true);
    let tx = Transaction {
        signer: HARDHAT_DEFAULT_SIGNER_0,
        nonce: 0,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::NoOp),
    };
    let pre_state = n.state.clone();
    let mut scratch = n.state.clone();
    let block = execute_and_build_block(
        n.chain_id,
        1,
        n.view,
        n.head_hash,
        n.parent_qc_hash,
        n.validators.expected_proposer(n.view),
        1,
        n.gas_limit,
        &mut scratch,
        vec![tx],
        eth_signed_raws_for_txs(1),
    )
    .expect("block");
    let witness =
        native_transition_witness(mixed_execution_witness_from_replay(&block, &pre_state).unwrap());
    let mut metadata =
        mixed_execution_witness_metadata(&witness, WitnessRetentionPolicyV1::MetadataOnly).unwrap();
    metadata.witness_digest[0] ^= 0x99;
    n.record_witness_metadata(metadata);
    n.apply_synced_block(&block).expect("apply");

    let proof = native_recursive_block_proof(&block, &witness);
    let err = n
        .submit_validity_proof(proof)
        .expect_err("witness mismatch rejected");
    assert!(err.to_string().contains("witness digest"));
    assert_eq!(n.proof_metrics.proofs_rejected, 1);
    assert_eq!(
        n.proof_metrics.latest_rejection_reason.as_deref(),
        Some("witness_digest")
    );
}

#[test]
fn mixed_block_accepts_mixed_proof_and_rejects_native_only_coverage() {
    let mut n = NodeInner::devnet();
    let native_tx = Transaction {
        signer: HARDHAT_DEFAULT_SIGNER_0,
        nonce: 0,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::NoOp),
    };
    let evm_signer = [0x42u8; 20];
    n.state.accounts.insert(
        evm_signer,
        fractal_core::Account {
            nonce: 0,
            balance: 1_000_000,
        },
    );
    let evm_tx = Transaction {
        signer: evm_signer,
        nonce: 0,
        vm: VmKind::Evm,
        body: TxBody::Transfer {
            to: [0x43u8; 20],
            amount: 10,
        },
    };
    let pre_state = n.state.clone();
    let mut scratch = n.state.clone();
    let block = execute_and_build_block(
        n.chain_id,
        1,
        n.view,
        n.head_hash,
        n.parent_qc_hash,
        n.validators.expected_proposer(n.view),
        1,
        n.gas_limit,
        &mut scratch,
        vec![native_tx, evm_tx],
        eth_signed_raws_for_txs(2),
    )
    .expect("block");
    let block_hash = header_hash(&block.header).unwrap();
    let witness =
        mixed_transition_witness(mixed_execution_witness_from_replay(&block, &pre_state).unwrap());
    n.record_witness_metadata(
        mixed_execution_witness_metadata(&witness, WitnessRetentionPolicyV1::MetadataOnly).unwrap(),
    );
    n.apply_synced_block(&block).expect("apply");

    let native_only_proof = BlockValidityProof {
        chain_id: block.header.chain_id,
        height: block.header.height,
        block_hash,
        timestamp_ms: block.header.timestamp_ms,
        parent_state_root: block.header.parent_state_root,
        state_root: block.header.state_root,
        tx_root: block.header.tx_root,
        receipt_root: block.header.receipt_root,
        native_event_root: block.header.native_event_root,
        evm_log_root: block.header.evm_log_root,
        gas_used: block.header.gas_used,
        zone_namespace: block.header.zone_namespace,
        da_root: block.header.da_root,
        circuit_version: CircuitVersion::NativeStateTransitionV1,
        coverage_manifest_digest: coverage_manifest_digest(&coverage_manifest_for_circuit_version(
            CircuitVersion::NativeStateTransitionV1,
        ))
        .unwrap(),
        feature_set: block.header.feature_set,
        proof_system: ValidityProofSystem::StwoPlonky2,
        proof_bytes: vec![1],
    };
    assert!(n.submit_validity_proof(native_only_proof).is_err());
    assert_eq!(
        n.finality_for_block_hash(&block_hash),
        Some(fractal_node::BlockFinality::Soft)
    );

    let proof = mixed_aggregate_block_proof(&block, &witness);
    n.submit_validity_proof(proof)
        .expect("mixed proof accepted");
    assert_eq!(
        n.finality_for_block_hash(&block_hash),
        Some(fractal_node::BlockFinality::Proof)
    );
    let stored = n.proof_finalized_blocks.get(&block_hash).unwrap();
    assert_eq!(
        stored.circuit_version,
        CircuitVersion::MixedStateTransitionV1
    );
}

#[test]
fn proof_finality_records_persist_to_store() {
    let mut n = NodeInner::devnet();
    let tx = Transaction {
        signer: HARDHAT_DEFAULT_SIGNER_0,
        nonce: 0,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::NoOp),
    };
    let mut scratch = n.state.clone();
    let block = execute_and_build_block(
        n.chain_id,
        1,
        n.view,
        n.head_hash,
        n.parent_qc_hash,
        n.validators.expected_proposer(n.view),
        1,
        n.gas_limit,
        &mut scratch,
        vec![tx],
        eth_signed_raws_for_txs(1),
    )
    .expect("block");
    let block_hash = header_hash(&block.header).unwrap();

    n.apply_synced_block(&block).expect("apply");
    let path = std::env::temp_dir().join(format!(
        "fractal-proof-finality-{}-{}.borsh",
        std::process::id(),
        block.header.height
    ));
    let _ = std::fs::remove_file(&path);
    n.set_proof_finality_store(ProofFinalityStore::open(&path).expect("store"))
        .expect("attach store");

    let mut proof = BlockValidityProof {
        chain_id: block.header.chain_id,
        height: block.header.height,
        block_hash,
        timestamp_ms: block.header.timestamp_ms,
        parent_state_root: block.header.parent_state_root,
        state_root: block.header.state_root,
        tx_root: block.header.tx_root,
        receipt_root: block.header.receipt_root,
        native_event_root: block.header.native_event_root,
        evm_log_root: block.header.evm_log_root,
        gas_used: block.header.gas_used,
        zone_namespace: block.header.zone_namespace,
        da_root: block.header.da_root,
        circuit_version: CircuitVersion::DevMixedV1,
        coverage_manifest_digest: coverage_manifest_digest(&coverage_manifest_for_circuit_version(
            CircuitVersion::DevMixedV1,
        ))
        .unwrap(),
        feature_set: block.header.feature_set,
        proof_system: ValidityProofSystem::DevDigest,
        proof_bytes: Vec::new(),
    };
    proof.proof_bytes = validity_proof_public_input_digest(&proof).unwrap().to_vec();
    n.submit_validity_proof(proof).expect("proof accepted");

    let mut restored = NodeInner::devnet();
    restored
        .set_proof_finality_store(ProofFinalityStore::open(&path).expect("store reopen"))
        .expect("restore store");
    assert_eq!(
        restored.finality_for_block_hash(&block_hash),
        Some(fractal_node::BlockFinality::Proof)
    );
    assert_eq!(
        restored.proof_metrics.proof_final_height,
        block.header.height
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn validity_proof_rejects_unknown_block() {
    let mut n = NodeInner::devnet();
    let proof = BlockValidityProof {
        chain_id: n.chain_id,
        height: 1,
        block_hash: [9u8; 32],
        timestamp_ms: 0,
        parent_state_root: [0u8; 32],
        state_root: [8u8; 32],
        tx_root: [7u8; 32],
        receipt_root: [0u8; 32],
        native_event_root: [0u8; 32],
        evm_log_root: [0u8; 32],
        gas_used: 0,
        zone_namespace: fractal_consensus::MASTERCHAIN_ZONE_NAMESPACE,
        da_root: [6u8; 32],
        circuit_version: CircuitVersion::DevMixedV1,
        coverage_manifest_digest: coverage_manifest_digest(&coverage_manifest_for_circuit_version(
            CircuitVersion::DevMixedV1,
        ))
        .unwrap(),
        feature_set: ExecutionFeatureSetV1::empty(),
        proof_system: ValidityProofSystem::DevDigest,
        proof_bytes: vec![1],
    };

    assert!(n.submit_validity_proof(proof).is_err());
    assert_eq!(n.proof_metrics.proofs_rejected, 1);
    assert_eq!(
        n.proof_metrics.latest_rejection_reason.as_deref(),
        Some("block_not_found")
    );
    assert_eq!(
        n.proof_metrics
            .rejection_reasons
            .get("block_not_found")
            .copied(),
        Some(1)
    );
}

#[test]
fn proof_required_settlement_config_switches_finality_requirement() {
    let mut n = NodeInner::devnet();
    assert!(!n.settlement_requires_proof());

    n.set_proof_required_settlement(true);
    assert!(n.settlement_requires_proof());

    n.set_proof_required_settlement(false);
    assert!(!n.settlement_requires_proof());
}

#[test]
fn protocol_phase_config_switches_all_rollout_gates() {
    let mut n = NodeInner::devnet();
    let cfg = fractal_core::ProtocolPhaseConfig::mainnet();

    n.set_protocol_phase_config(cfg.clone());

    assert!(n.settlement_requires_proof());
    assert_eq!(n.chain_config.phase_config, cfg);
    n.set_proof_required_settlement(false);
    assert!(!n.settlement_requires_proof());
    assert!(!n.chain_config.phase_config.proof_final_settlement);
}

#[test]
fn proof_required_settlement_rejects_soft_final_block() {
    let mut n = NodeInner::devnet();
    let tx = Transaction {
        signer: HARDHAT_DEFAULT_SIGNER_0,
        nonce: 0,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::NoOp),
    };
    let mut scratch = n.state.clone();
    let block = execute_and_build_block(
        n.chain_id,
        1,
        n.view,
        n.head_hash,
        n.parent_qc_hash,
        n.validators.expected_proposer(n.view),
        1,
        n.gas_limit,
        &mut scratch,
        vec![tx],
        eth_signed_raws_for_txs(1),
    )
    .expect("block");
    let block_hash = header_hash(&block.header).unwrap();
    n.apply_synced_block(&block).expect("apply");

    assert_eq!(
        n.settlement_finality_for_block_hash(&block_hash).unwrap(),
        fractal_node::BlockFinality::Soft
    );
    n.set_proof_required_settlement(true);
    assert_eq!(
        n.settlement_finality_for_block_hash(&block_hash),
        Err(SettlementAccessError::NotProofFinal)
    );
}
