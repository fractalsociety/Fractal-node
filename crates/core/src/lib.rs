//! Pure execution state machine (M1): mocked native calls + canonical `state_root`.
//!
//! Full Merkle Patricia Trie lives in `fractal-storage` later; here `state_root` is
//! `keccak256(borsh(State))` with sorted `BTreeMap` fields for deterministic iteration.

mod error;
mod state;
mod tx;

#[cfg(feature = "wallet")]
pub mod wallet_anchor;

pub use error::ExecError;
pub use state::{Account, Address, State};
pub use tx::{NativeCall, Transaction, TxBody, VmKind};

use fractal_crypto::hash::commit_borsh;

/// Deterministic state commitment (EVM-style root uses keccak over canonical bytes).
pub fn state_root(state: &State) -> Result<fractal_crypto::Hash256, std::io::Error> {
    commit_borsh(state)
}

/// Apply an ordered list of transactions. Stops at the first invalid tx.
pub fn apply_block(state: &mut State, txs: &[Transaction]) -> Result<(), ExecError> {
    for tx in txs {
        state.apply_transaction(tx)?;
    }
    Ok(())
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
