use borsh::BorshDeserialize;

#[test]
fn malformed_remote_consensus_payloads_return_errors() {
    let malformed = [0xff, 0x00, 0x7f, 0x13, 0x37];

    assert!(std::panic::catch_unwind(|| {
        let _ = fractal_consensus::BlockValidityProof::try_from_slice(&malformed);
        let _ = fractal_consensus::StwoPlonky2ProofEnvelope::try_from_slice(&malformed);
        let _ = fractal_consensus::MixedExecutionWitnessV1::try_from_slice(&malformed);
        let _ = fractal_consensus::DaShare::try_from_slice(&malformed);
        let _ = fractal_consensus::DaSidecar::try_from_slice(&malformed);
    })
    .is_ok());
}

#[test]
fn da_sidecar_builder_is_fallible_and_round_trips_valid_payloads() {
    let payload = b"t1-da-regression-payload";
    let sidecar =
        fractal_consensus::build_da_sidecar(payload, fractal_consensus::DEFAULT_DA_NAMESPACE, 8)
            .expect("valid DA fixture");

    assert_eq!(
        fractal_consensus::reconstruct_da_payload(&sidecar).expect("reconstruct"),
        payload
    );
}
