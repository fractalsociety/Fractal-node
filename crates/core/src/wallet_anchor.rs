//! Optional binding between `fractal-core` state commitments and wallet `TaskReceipt`
//! (`docs/wallet.md` §9.2). Enable with `--features wallet` on `fractal-core`.
//!
//! **W6-d:** on-chain anchor via [`NativeCall::WalletTaskReceiptAnchorV1`](crate::tx::NativeCall)
//! (`OP_WALLET_TASK_RECEIPT_ANCHOR_V1` = `0x0e`) stores `task_receipt_commitment` in
//! [`State::wallet_task_receipt_anchors`](crate::state::State). Non-empty `receipt_witness`
//! is verified only when this feature is enabled.

use fractal_crypto::hash::commit_borsh;
use fractal_wallet::TaskReceipt;

/// Canonical `keccak256(borsh(task_receipt))` for native settlement / receipts trie.
pub fn task_receipt_commitment(
    receipt: &TaskReceipt,
) -> Result<fractal_crypto::Hash256, std::io::Error> {
    commit_borsh(receipt)
}
