//! Pure execution state machine: native M3 subtries + canonical `state_root`.
//!
//! Full Merkle Patricia Trie lives in `fractal-storage` later; here `state_root` is
//! `keccak256(borsh(State))` with sorted `BTreeMap` fields for deterministic iteration.

mod address;
mod error;
pub mod merkle;
mod native_gas;
mod native_types;
mod state;
mod tx;

#[cfg(feature = "wallet")]
pub mod wallet_anchor;

pub use address::Address;
pub use error::ExecError;
pub use merkle::{merkle_proof, merkle_root, verify_merkle_proof};
pub use native_gas::{
    intrinsic_gas, is_native_precompile_address, native_opcode_from_precompile_address, PER_BYTE,
    TRANSFER_GAS,
};
pub use native_types::*;
pub use state::{Account, State};
pub use tx::{NativeCall, Transaction, TxBody, VmKind};

use fractal_crypto::hash::commit_borsh;

/// Deterministic state commitment (EVM-style root uses keccak over canonical bytes).
pub fn state_root(state: &State) -> Result<fractal_crypto::Hash256, std::io::Error> {
    commit_borsh(state)
}

/// Apply an ordered list of transactions. Returns total intrinsic gas used.
pub fn apply_block(state: &mut State, txs: &[Transaction]) -> Result<u64, ExecError> {
    let mut sum = 0u64;
    for tx in txs {
        let g = intrinsic_gas(tx)?;
        sum = sum.checked_add(g).ok_or(ExecError::GasOverflow)?;
        state.apply_transaction(tx)?;
    }
    Ok(sum)
}

#[cfg(all(test, feature = "wallet"))]
mod wallet_anchor_tests {
    use fractal_wallet::TaskReceipt;

    use super::wallet_anchor;

    #[test]
    fn task_receipt_commitment_is_deterministic() {
        let tr = TaskReceipt {
            task_id: 1,
            agent_session: [2u8; 32],
            artifact_commitment: [3u8; 32],
            artifact_pointer: "da://x".into(),
            tool_receipt_root: [4u8; 32],
            total_tool_cost: 100,
        };
        let a = wallet_anchor::task_receipt_commitment(&tr).unwrap();
        let b = wallet_anchor::task_receipt_commitment(&tr).unwrap();
        assert_eq!(a, b);
    }
}
