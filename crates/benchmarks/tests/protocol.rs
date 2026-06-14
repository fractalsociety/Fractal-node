use fractal_bench::{
    run_da_sampling_bandwidth_bench, run_mixed_proof_slo_bench,
    run_owned_object_certificate_throughput_bench, run_proof_latency_cost_bench,
    run_protocol_bench,
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
}

#[test]
fn proof_latency_cost_bench_verifies_dev_digest_proofs() {
    let report = run_proof_latency_cost_bench(10, 3, 17, 41);

    assert_eq!(report.proof_count, 10);
    assert_eq!(report.covered_blocks_per_proof, 3);
    assert_eq!(report.proof_bytes, 32);
    assert_eq!(report.verified_proofs, 10);
    assert_eq!(report.estimated_cost_per_proof_micro_frac, 51);
    assert_eq!(report.estimated_total_prover_cost_micro_frac, 510);
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
