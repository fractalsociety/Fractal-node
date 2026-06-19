#![no_main]

use borsh::BorshDeserialize;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > 4096 {
        return;
    }

    let _ = fractal_network::SyncRequest::try_from_slice(data);
    let _ = fractal_network::SyncResponse::try_from_slice(data);
    let _ = fractal_network::DaProviderAnnouncement::try_from_slice(data);
    let _ = fractal_consensus::Vote::try_from_slice(data);
});
