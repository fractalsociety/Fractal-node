//! Optional binding between `fractal-core` state commitments and wallet `TaskReceipt`
//! (`docs/wallet.md` §9.2). Enable with `--features wallet` on `fractal-core`.

use fractal_crypto::hash::commit_borsh;
use fractal_wallet::TaskReceipt;

/// Canonical `keccak256(borsh(task_receipt))` for native settlement / receipts trie.
pub fn task_receipt_commitment(receipt: &TaskReceipt) -> Result<fractal_crypto::Hash256, std::io::Error> {
    commit_borsh(receipt)
}
