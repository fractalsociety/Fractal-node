#![no_main]

use borsh::BorshDeserialize;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > 4096 {
        return;
    }

    if let Ok(share) = fractal_consensus::DaShare::try_from_slice(data) {
        let _ = fractal_consensus::da_share_commitment(
            share.namespace,
            share.index,
            share.is_parity,
            &share.data,
        );
    }

    if let Ok(sidecar) = fractal_consensus::DaSidecar::try_from_slice(data) {
        let root = fractal_consensus::da_root(&sidecar);
        let _ =
            fractal_consensus::verify_da_samples(&sidecar, root, sidecar.namespace, 0x5449_4646, 4);
        let _ = fractal_consensus::reconstruct_da_payload(&sidecar);
    }

    let _ = Vec::<fractal_consensus::DaShare>::try_from_slice(data);
});
