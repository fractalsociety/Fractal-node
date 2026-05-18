//! Wallet-native §16.3 multi–tool-receipt batch settlement (distinct from L1 `SettleBatch` / M3).

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use thiserror::Error;

use crate::market::provider_id_from_public_key;
use crate::task_receipt::{tool_receipt_root, ToolReceipt};
use crate::types::{Amount, ProviderId, PublicKey, ToolClass};

/// Sign bytes for [`fractal_core::native_types::WalletToolBatchSettlePayload::provider_batch_sig`].
pub fn wallet_tool_batch_sign_message(
    batch_id: &[u8; 32],
    receipt_root: &[u8; 32],
    total_cost: Amount,
    receipt_count: u32,
    payout_to: &[u8; 20],
) -> Vec<u8> {
    let mut msg = Vec::with_capacity(32 + 32 + 16 + 4 + 20);
    msg.extend_from_slice(batch_id);
    msg.extend_from_slice(receipt_root);
    msg.extend_from_slice(&total_cost.to_le_bytes());
    msg.extend_from_slice(&receipt_count.to_le_bytes());
    msg.extend_from_slice(payout_to);
    msg
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum WalletBatchSettleBuildError {
    #[error("empty receipt list")]
    Empty,
    #[error("receipt provider_id mismatch")]
    ProviderMismatch,
    #[error("receipt tool_class mismatch")]
    ToolClassMismatch,
    #[error("total cost mismatch")]
    CostMismatch,
    #[error("encode error")]
    Encode,
}

/// Build sorted Merkle root + borsh receipt blobs for on-chain submission.
pub fn prepare_wallet_batch_receipts(
    receipts: &[ToolReceipt],
    expected_provider: ProviderId,
    expected_class: ToolClass,
    expected_total: Amount,
) -> Result<( [u8; 32], Vec<Vec<u8>>), WalletBatchSettleBuildError> {
    if receipts.is_empty() {
        return Err(WalletBatchSettleBuildError::Empty);
    }
    let mut total = 0u128;
    for r in receipts {
        if r.body.provider_id != expected_provider {
            return Err(WalletBatchSettleBuildError::ProviderMismatch);
        }
        if r.body.tool_class != expected_class {
            return Err(WalletBatchSettleBuildError::ToolClassMismatch);
        }
        total = total.saturating_add(r.body.cost);
    }
    if total != expected_total {
        return Err(WalletBatchSettleBuildError::CostMismatch);
    }
    let root = tool_receipt_root(receipts);
    let blobs: Vec<Vec<u8>> = receipts
        .iter()
        .map(|r| borsh::to_vec(r).map_err(|_| WalletBatchSettleBuildError::Encode))
        .collect::<Result<_, _>>()?;
    Ok((root, blobs))
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum WalletBatchSettleSigError {
    #[error("invalid provider public key")]
    BadKey,
    #[error("provider_id does not match public key")]
    ProviderIdMismatch,
    #[error("invalid batch signature")]
    BadSig,
}

pub fn sign_wallet_tool_batch(
    provider_sk: &SigningKey,
    batch_id: [u8; 32],
    receipt_root: [u8; 32],
    total_cost: Amount,
    receipt_count: u32,
    payout_to: [u8; 20],
) -> Result<([u8; 32], [u8; 64]), std::io::Error> {
    let pk = provider_sk.verifying_key().to_bytes();
    let provider_id = provider_id_from_public_key(&pk);
    let msg = wallet_tool_batch_sign_message(
        &batch_id,
        &receipt_root,
        total_cost,
        receipt_count,
        &payout_to,
    );
    let sig = provider_sk.sign(&msg).to_bytes();
    Ok((provider_id, sig))
}

pub fn verify_wallet_tool_batch_sig(
    provider_id: ProviderId,
    provider_public_key: &PublicKey,
    batch_id: &[u8; 32],
    receipt_root: &[u8; 32],
    total_cost: Amount,
    receipt_count: u32,
    payout_to: &[u8; 20],
    sig: &[u8; 64],
) -> Result<(), WalletBatchSettleSigError> {
    if provider_id_from_public_key(provider_public_key) != provider_id {
        return Err(WalletBatchSettleSigError::ProviderIdMismatch);
    }
    let vk = VerifyingKey::from_bytes(provider_public_key)
        .map_err(|_| WalletBatchSettleSigError::BadKey)?;
    let msg = wallet_tool_batch_sign_message(
        batch_id,
        receipt_root,
        total_cost,
        receipt_count,
        payout_to,
    );
    let signature = Signature::from_bytes(sig);
    vk.verify(&msg, &signature)
        .map_err(|_| WalletBatchSettleSigError::BadSig)
}

/// Parse and cryptographically verify every receipt in a batch payload.
pub fn verify_wallet_batch_receipts(
    receipts_borsh: &[Vec<u8>],
    provider_id: ProviderId,
    provider_public_key: &PublicKey,
    tool_class: u8,
    receipt_root: &[u8; 32],
    total_cost: u128,
) -> Result<Vec<ToolReceipt>, WalletBatchSettleVerifyError> {
    if receipts_borsh.is_empty() {
        return Err(WalletBatchSettleVerifyError::Empty);
    }
    if provider_id_from_public_key(provider_public_key) != provider_id {
        return Err(WalletBatchSettleVerifyError::ProviderMismatch);
    }
    let mut receipts = Vec::with_capacity(receipts_borsh.len());
    for blob in receipts_borsh {
        let r: ToolReceipt = borsh::from_slice(blob)
            .map_err(|_| WalletBatchSettleVerifyError::InvalidReceipt)?;
        if r.body.provider_id != provider_id {
            return Err(WalletBatchSettleVerifyError::ProviderMismatch);
        }
        if r.body.tool_class as u8 != tool_class {
            return Err(WalletBatchSettleVerifyError::ToolClassMismatch);
        }
        r.verify_provider(provider_public_key)
            .map_err(|_| WalletBatchSettleVerifyError::ReceiptProviderSig)?;
        r.verify_agent_ack()
            .map_err(|_| WalletBatchSettleVerifyError::ReceiptAgentAck)?;
        receipts.push(r);
    }
    let root = tool_receipt_root(&receipts);
    if root != *receipt_root {
        return Err(WalletBatchSettleVerifyError::RootMismatch);
    }
    let sum: u128 = receipts.iter().map(|r| r.body.cost).sum();
    if sum != total_cost {
        return Err(WalletBatchSettleVerifyError::CostMismatch);
    }
    Ok(receipts)
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum WalletBatchSettleVerifyError {
    #[error("empty receipt list")]
    Empty,
    #[error("invalid receipt borsh")]
    InvalidReceipt,
    #[error("receipt provider_id mismatch")]
    ProviderMismatch,
    #[error("receipt tool_class mismatch")]
    ToolClassMismatch,
    #[error("receipt provider signature invalid")]
    ReceiptProviderSig,
    #[error("receipt missing or invalid agent ack")]
    ReceiptAgentAck,
    #[error("receipt merkle root mismatch")]
    RootMismatch,
    #[error("total cost mismatch")]
    CostMismatch,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task_receipt::{MeteringRecord, ToolReceipt, ToolReceiptBody};
    use crate::types::ToolClass;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    fn sample(provider_sk: &SigningKey, agent_sk: &SigningKey, intent: u8, cost: u128) -> ToolReceipt {
        let provider_pk = provider_sk.verifying_key().to_bytes();
        let body = ToolReceiptBody {
            intent_id: [intent; 32],
            task_id: 1,
            agent_session: agent_sk.verifying_key().to_bytes(),
            provider_id: provider_id_from_public_key(&provider_pk),
            tool_class: ToolClass::Browser,
            payload_commitment: [0x01; 32],
            output_commitment: [0x02; 32],
            output_pointer: "da://t".into(),
            metering: MeteringRecord {
                input_tokens: 0,
                output_tokens: 0,
                wall_duration_ms: 0,
                bytes_metered: 0,
            },
            cost,
            started_at: 1,
            completed_at: 2,
            attestation: None,
        };
        let r = ToolReceipt::sign_new(body, provider_sk).unwrap();
        let ack = r.sign_agent_ack(agent_sk).unwrap();
        r.with_agent_ack(ack)
    }

    #[test]
    fn prepare_and_verify_batch_roundtrip() {
        let provider_sk = SigningKey::generate(&mut OsRng);
        let agent_sk = SigningKey::generate(&mut OsRng);
        let provider_pk = provider_sk.verifying_key().to_bytes();
        let provider_id = provider_id_from_public_key(&provider_pk);
        let r1 = sample(&provider_sk, &agent_sk, 1, 100);
        let r2 = sample(&provider_sk, &agent_sk, 2, 50);
        let (root, blobs) =
            prepare_wallet_batch_receipts(&[r1, r2], provider_id, ToolClass::Browser, 150).unwrap();
        let batch_id = [0x77u8; 32];
        let payout = [0x99u8; 20];
        let (_, sig) =
            sign_wallet_tool_batch(&provider_sk, batch_id, root, 150, 2, payout).unwrap();
        verify_wallet_tool_batch_sig(
            provider_id,
            &provider_pk,
            &batch_id,
            &root,
            150,
            2,
            &payout,
            &sig,
        )
        .unwrap();
        verify_wallet_batch_receipts(
            &blobs,
            provider_id,
            &provider_pk,
            ToolClass::Browser as u8,
            &root,
            150,
        )
        .unwrap();
    }
}
