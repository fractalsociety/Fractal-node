#![no_main]

use borsh::BorshDeserialize;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > 4096 {
        return;
    }

    let _ = fractal_consensus::BlockValidityProof::try_from_slice(data);
    let _ = fractal_consensus::StwoPlonky2ProofEnvelope::try_from_slice(data);
    let _ = fractal_consensus::MixedExecutionWitnessV1::try_from_slice(data);
    let _ = fractal_consensus::MixedExecutionPublicInputsV1::try_from_slice(data);
});
