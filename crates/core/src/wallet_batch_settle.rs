//! On-chain wallet §16.3 `WalletBatchSettleV1` (`docs/wallet.md`; distinct from M3 `SettleBatch`).

use crate::address::Address;
use crate::error::ExecError;
use crate::native_types::{StoredWalletToolBatch, WalletToolBatchSettlePayload};
use crate::state::State;

#[cfg(feature = "wallet")]
pub fn apply_wallet_batch_settle_v1(
    state: &mut State,
    signer: Address,
    p: &WalletToolBatchSettlePayload,
    bump_nonce: bool,
) -> Result<(), ExecError> {
    use fractal_wallet::{
        WalletBatchSettleSigError, WalletBatchSettleVerifyError, verify_wallet_batch_receipts,
        verify_wallet_tool_batch_sig,
    };

    if state.wallet_tool_batches.contains_key(&p.batch_id) {
        return Err(ExecError::WalletToolBatchDuplicate);
    }
    if p.receipts_borsh.is_empty() {
        return Err(ExecError::WalletToolBatchInvalid);
    }
    let receipt_count =
        u32::try_from(p.receipts_borsh.len()).map_err(|_| ExecError::WalletToolBatchInvalid)?;
    if receipt_count == 0 {
        return Err(ExecError::WalletToolBatchInvalid);
    }
    verify_wallet_tool_batch_sig(
        p.provider_id,
        &p.provider_public_key,
        &p.batch_id,
        &p.receipt_root,
        p.total_cost,
        receipt_count,
        &p.payout_to,
        &p.provider_batch_sig,
    )
    .map_err(|e| match e {
        WalletBatchSettleSigError::ProviderIdMismatch | WalletBatchSettleSigError::BadKey => {
            ExecError::WalletToolBatchInvalid
        }
        WalletBatchSettleSigError::BadSig => ExecError::BadSignature,
    })?;

    let receipts = verify_wallet_batch_receipts(
        &p.receipts_borsh,
        p.provider_id,
        &p.provider_public_key,
        p.tool_class,
        &p.receipt_root,
        p.total_cost,
    )
    .map_err(|e| match e {
        WalletBatchSettleVerifyError::Empty
        | WalletBatchSettleVerifyError::InvalidReceipt
        | WalletBatchSettleVerifyError::ProviderMismatch
        | WalletBatchSettleVerifyError::ToolClassMismatch
        | WalletBatchSettleVerifyError::RootMismatch
        | WalletBatchSettleVerifyError::CostMismatch => ExecError::WalletToolBatchInvalid,
        WalletBatchSettleVerifyError::ReceiptProviderSig
        | WalletBatchSettleVerifyError::ReceiptAgentAck => ExecError::BadSignature,
    })?;

    for r in &receipts {
        if state
            .wallet_settled_tool_receipt_ids
            .contains_key(&r.receipt_id)
        {
            return Err(ExecError::WalletToolReceiptAlreadySettled);
        }
    }

    {
        let payer = state
            .accounts
            .get_mut(&signer)
            .ok_or(ExecError::UnknownSigner)?;
        if payer.balance < p.total_cost {
            return Err(ExecError::InsufficientBalance);
        }
        payer.balance -= p.total_cost;
    }
    {
        let payee = state.accounts.entry(p.payout_to).or_insert(crate::Account {
            nonce: 0,
            balance: 0,
        });
        payee.balance = payee.balance.saturating_add(p.total_cost);
    }

    for r in &receipts {
        state
            .wallet_settled_tool_receipt_ids
            .insert(r.receipt_id, p.batch_id);
    }

    state.wallet_tool_batches.insert(
        p.batch_id,
        StoredWalletToolBatch {
            relayer: signer,
            provider_id: p.provider_id,
            tool_class: p.tool_class,
            receipt_root: p.receipt_root,
            receipt_count,
            total_cost: p.total_cost,
            payout_to: p.payout_to,
            submitted_at: p.submitted_at,
        },
    );

    if bump_nonce {
        state.bump_nonce(signer);
    }
    Ok(())
}
