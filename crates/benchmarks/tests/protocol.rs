use fractal_bench::{
    run_baseline_bench, run_da_sampling_bandwidth_bench, run_mixed_proof_slo_bench,
    run_owned_object_certificate_throughput_bench, run_proof_ingestion_bench,
    run_proof_latency_cost_bench, run_protocol_bench, BaselineBenchConfig, BaselineScenarioKind,
};

#[test]
fn owned_object_certificate_bench_counts_verified_certificates() {
    let report = run_owned_object_certificate_throughput_bench(12, 7, 5);

    assert_eq!(report.certificate_count, 12);
    assert_eq!(report.validator_count, 7);
    assert_eq!(report.quorum_threshold, 5);
    assert_eq!(report.total_signatures, 60);
    assert_eq!(report.verified_certificates, 12);
    assert!(report.certificates_per_second.is_finite());
    assert!(report.certificates_per_second > 0.0);
}

#[test]
fn da_sampling_bandwidth_bench_accounts_sampled_bytes() {
    let report = run_da_sampling_bandwidth_bench(8 * 1024, 512, 8, 10, 41);

    assert_eq!(report.payload_bytes, 8 * 1024);
    assert_eq!(report.share_size, 512);
    assert_eq!(report.sample_count_per_round, 8);
    assert_eq!(report.rounds, 10);
    assert_eq!(report.sampled_bytes, 8 * 10 * 512);
    assert!(report.encoded_bytes >= report.payload_bytes as u64);
    assert!(report.sampled_bytes_per_second.is_finite());
    assert!(report.sampled_bytes_per_second > 0.0);
}

#[test]
fn protocol_bench_runs_both_protocol_reports() {
    let report = run_protocol_bench(4, 7, 5, 4096, 512, 4, 3, 6, 2, 11, 99);

    assert_eq!(report.owned_object_certificates.verified_certificates, 4);
    assert_eq!(
        report.fee_policy.cost_categories,
        vec![
            "da_bytes".to_owned(),
            "proof_verify".to_owned(),
            "shared_state_execution".to_owned()
        ]
    );
    assert_eq!(report.fee_policy.da_fee_per_byte, 1);
    assert_eq!(report.da_sampling.sampled_bytes, 4 * 3 * 512);
    assert_eq!(report.proof_latency_cost.verified_proofs, 6);
    assert_eq!(report.mixed_proof_slo.verified_proofs, 6);
    assert_eq!(report.mixed_proof_slo.tx_count, 2);
    assert_eq!(
        report
            .proof_latency_cost
            .estimated_total_prover_cost_micro_frac,
        6 * 2 * 11
    );
    assert_eq!(
        report.proof_latency_cost.estimated_total_proof_verify_fee,
        6 * report.proof_latency_cost.proof_verify_fee_per_proof
    );
}

#[test]
fn proof_latency_cost_bench_verifies_production_fixture_proofs() {
    let report = run_proof_latency_cost_bench(10, 3, 17, 41);

    assert_eq!(report.proof_count, 10);
    assert_eq!(report.covered_blocks_per_proof, 3);
    assert!(report.proof_bytes > 32);
    assert_eq!(report.verified_proofs, 10);
    assert_eq!(report.estimated_cost_per_proof_micro_frac, 51);
    assert_eq!(report.estimated_total_prover_cost_micro_frac, 510);
    assert_eq!(
        report.proof_verify_fee_per_proof,
        10_000 + report.proof_bytes as u128
    );
    assert_eq!(
        report.estimated_total_proof_verify_fee,
        10 * report.proof_verify_fee_per_proof
    );
    assert!(report.avg_verify_latency_micros.is_finite());
    assert!(report.proofs_per_second > 0.0);
}

#[test]
fn mixed_proof_slo_bench_measures_all_phase_g_steps() {
    let report = run_mixed_proof_slo_bench(5, 41);

    assert_eq!(report.iterations, 5);
    assert_eq!(report.tx_count, 2);
    assert_eq!(report.verified_proofs, 5);
    assert!(report.proof_bytes > 32);
    assert!(report.witness_gen_latency_nanos > 0);
    assert!(report.native_component_latency_nanos > 0);
    assert!(report.evm_zkvm_fixture_latency_nanos > 0);
    assert!(report.aggregation_latency_nanos > 0);
    assert!(report.verification_latency_nanos > 0);
    assert!(report.avg_total_latency_micros.is_finite());
    assert!(report.avg_total_latency_micros > 0.0);
}

#[test]
fn baseline_bench_runs_all_h1_scenarios_with_stable_metrics() {
    let report = run_baseline_bench(BaselineBenchConfig {
        blocks_per_scenario: 2,
        txs_per_block: 4,
        chain_id: 41,
        gas_limit: 60_000_000,
        seed: 7,
    });

    assert_eq!(report.schema_version, 1);
    assert_eq!(report.run_kind, "baseline");
    assert_eq!(report.scenarios.len(), 5);
    assert_eq!(
        report.scenarios.iter().map(|s| s.kind).collect::<Vec<_>>(),
        vec![
            BaselineScenarioKind::NativeNoOp,
            BaselineScenarioKind::OwnedObjectTx,
            BaselineScenarioKind::ProofCommitment,
            BaselineScenarioKind::MixedEvmNative,
            BaselineScenarioKind::Bft7ValidatorLab,
        ]
    );
    for scenario in &report.scenarios {
        assert_eq!(scenario.blocks, 2);
        assert_eq!(scenario.submitted_txs, 8);
        assert_eq!(scenario.committed_txs, 8);
        assert!(scenario.submitted_tx_per_second.is_finite());
        assert!(scenario.submitted_tx_per_second > 0.0);
        assert!(scenario.committed_tx_per_second.is_finite());
        assert!(scenario.committed_tx_per_second > 0.0);
        assert!(scenario.block_p50_latency_nanos > 0);
        assert!(scenario.block_p95_latency_nanos >= scenario.block_p50_latency_nanos);
        assert!(scenario.cpu_nanos > 0);
        assert!(scenario.peak_working_set_bytes > 0);
        assert!(scenario.total_block_bytes > 0);
        assert!(scenario.avg_block_bytes > 0.0);
        assert!(scenario.total_da_bytes > 0);
        assert!(scenario.avg_da_bytes > 0.0);
        assert!(scenario.replay_time_nanos > 0);
        assert!(scenario.replay_tx_per_second > 0.0);
        assert_eq!(scenario.accepted_proof_updates, 0);
        assert_eq!(scenario.accepted_certificate_updates, 0);
        assert_eq!(scenario.accepted_proof_updates_per_second, 0.0);
        assert_eq!(scenario.accepted_certificate_updates_per_second, 0.0);
        assert_eq!(scenario.proof_verify_time_nanos, 0);
        assert_eq!(scenario.da_sampling_time_nanos, 0);
        assert_eq!(scenario.total_payload_bytes, scenario.total_block_bytes);
        assert!(scenario.avg_payload_bytes > 0.0);
    }
    let bft = report
        .scenarios
        .iter()
        .find(|scenario| scenario.kind == BaselineScenarioKind::Bft7ValidatorLab)
        .unwrap();
    assert_eq!(bft.bft.validator_count, 7);
    assert_eq!(bft.bft.quorum_threshold, 5);
    assert_eq!(bft.bft.formed_qcs, 2);
    assert_eq!(bft.bft.votes_recorded, 10);
}

#[test]
fn proof_ingestion_bench_matches_h1_schema_and_records_ingestion_metrics() {
    let report = run_proof_ingestion_bench(BaselineBenchConfig {
        blocks_per_scenario: 2,
        txs_per_block: 4,
        chain_id: 41,
        gas_limit: 60_000_000,
        seed: 9,
    });

    assert_eq!(report.schema_version, 1);
    assert_eq!(report.run_kind, "proof_ingestion");
    assert_eq!(report.scenarios.len(), 5);
    assert_eq!(
        report.scenarios.iter().map(|s| s.kind).collect::<Vec<_>>(),
        vec![
            BaselineScenarioKind::ProofUpdates,
            BaselineScenarioKind::CertificateUpdates,
            BaselineScenarioKind::MixedProofSharedState,
            BaselineScenarioKind::DaSamplingProofUpdates,
            BaselineScenarioKind::Bft7ProofIngestion,
        ]
    );
    for scenario in &report.scenarios {
        assert_eq!(scenario.blocks, 2);
        assert_eq!(scenario.submitted_txs, 8);
        assert_eq!(scenario.committed_txs, 8);
        assert!(scenario.submitted_tx_per_second.is_finite());
        assert!(scenario.committed_tx_per_second.is_finite());
        assert!(scenario.block_p50_latency_nanos > 0);
        assert!(scenario.block_p95_latency_nanos >= scenario.block_p50_latency_nanos);
        assert!(scenario.cpu_nanos > 0);
        assert!(scenario.peak_working_set_bytes > 0);
        assert!(scenario.total_payload_bytes > 0);
        assert!(scenario.avg_payload_bytes > 0.0);
        assert!(scenario.total_block_bytes > 0);
        assert!(scenario.total_da_bytes > 0);
        assert!(scenario.replay_time_nanos > 0);
    }
    let proof = &report.scenarios[0];
    assert_eq!(proof.accepted_proof_updates, 8);
    assert!(proof.accepted_proof_updates_per_second > 0.0);
    assert!(proof.proof_verify_time_nanos > 0);

    let certs = &report.scenarios[1];
    assert_eq!(certs.accepted_certificate_updates, 8);
    assert!(certs.accepted_certificate_updates_per_second > 0.0);

    let da = &report.scenarios[3];
    assert_eq!(da.accepted_proof_updates, 8);
    assert!(da.da_sampling_time_nanos > 0);
    assert!(da.total_da_bytes >= da.total_payload_bytes);

    let bft = &report.scenarios[4];
    assert_eq!(bft.bft.validator_count, 7);
    assert_eq!(bft.bft.quorum_threshold, 5);
    assert_eq!(bft.bft.formed_qcs, 2);
    assert_eq!(bft.bft.votes_recorded, 10);
}
