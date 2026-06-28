#![no_main]

use borsh::BorshDeserialize;
use fractal_consensus::{
    certificate_batch_root, certificate_batches_root, proof_update_leaf_hash, proof_updates_root,
    BlockPayload, OwnedObjectCertificateBatchV1, ZoneProofUpdateV1,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > 8192 {
        return;
    }

    if let Ok(payload) = BlockPayload::try_from_slice(data) {
        let _ = payload.payload_root();
    }

    if let Ok(updates) = Vec::<ZoneProofUpdateV1>::try_from_slice(data) {
        let _ = proof_updates_root(&updates);
        if let Some(first) = updates.first() {
            let _ = proof_update_leaf_hash(first);
        }
    }

    if let Ok(batch) = OwnedObjectCertificateBatchV1::try_from_slice(data) {
        let _ = certificate_batch_root(&batch);
    }

    if let Ok(batches) = Vec::<OwnedObjectCertificateBatchV1>::try_from_slice(data) {
        let _ = certificate_batches_root(&batches);
    }
});
