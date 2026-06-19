#![no_main]

use borsh::BorshDeserialize;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > 4096 {
        return;
    }

    if let Ok(tx) = fractal_core::Transaction::try_from_slice(data) {
        let _ = fractal_core::intrinsic_gas(&tx);
        let _ = tx.execution_scope();
    }

    let _ = Vec::<fractal_core::Transaction>::try_from_slice(data);
});
