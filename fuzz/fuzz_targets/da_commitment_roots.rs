#![no_main]

use borsh::BorshDeserialize;
use fractal_consensus::{
    build_zone_blob_da_sidecar, da_root, proof_ingestion_header_extra,
    zone_blob_da_commitment_hash, ZoneBlobDaCommitmentV1, ZoneBlobDaV1,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > 8192 {
        return;
    }

    if let Ok(commitment) = ZoneBlobDaCommitmentV1::try_from_slice(data) {
        let _ = zone_blob_da_commitment_hash(&commitment);
        let _ = proof_ingestion_header_extra([0xA5; 32], &commitment);
    }

    if let Ok(blob) = ZoneBlobDaV1::try_from_slice(data) {
        if blob.payload.len() <= 4096 && (1..=4096).contains(&blob.share_size) {
            if let Ok((sidecar, commitment)) = build_zone_blob_da_sidecar(&blob) {
                let _ = da_root(&sidecar);
                let _ = zone_blob_da_commitment_hash(&commitment);
            }
        }
    }
});
