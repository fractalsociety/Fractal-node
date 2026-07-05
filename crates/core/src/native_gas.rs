//! PRD §9.2 fixed native gas (+ 4 gas / calldata byte on the native payload).

use crate::address::Address;
use crate::error::ExecError;
use crate::tx::{NativeCall, Transaction, TxBody, VmKind};

pub const PER_BYTE: u64 = 4;
pub const TRANSFER_GAS: u64 = 21_000;
pub const EVM_CALL_BASE_GAS: u64 = 21_000;

/// Maximum gas this transaction may consume against the block limit (EIP-style `gas` on EVM txs).
pub fn tx_gas_limit(tx: &Transaction) -> Result<u64, ExecError> {
    match (&tx.vm, &tx.body) {
        (VmKind::Evm, TxBody::EvmCall { gas_limit, .. })
        | (VmKind::Evm, TxBody::EvmCreate { gas_limit, .. }) => Ok(*gas_limit),
        _ => intrinsic_gas(tx),
    }
}

pub fn intrinsic_gas(tx: &Transaction) -> Result<u64, ExecError> {
    match (&tx.vm, &tx.body) {
        (VmKind::Native, TxBody::Native(c)) => {
            let native_payload = borsh::to_vec(c).map_err(|_| ExecError::InvalidShape)?;
            let base = native_base_gas(c);
            let extra = PER_BYTE.saturating_mul(native_payload.len() as u64);
            Ok(base.saturating_add(extra))
        }
        (VmKind::Evm, TxBody::Transfer { .. }) => Ok(TRANSFER_GAS),
        (VmKind::Evm, TxBody::EvmCall { calldata, .. }) => {
            let extra = PER_BYTE.saturating_mul(calldata.len() as u64);
            Ok(EVM_CALL_BASE_GAS.saturating_add(extra))
        }
        (VmKind::Evm, TxBody::EvmCreate { init_code, .. }) => {
            let extra = PER_BYTE.saturating_mul(init_code.len() as u64);
            Ok(EVM_CALL_BASE_GAS.saturating_add(extra))
        }
        _ => Err(ExecError::InvalidShape),
    }
}

fn native_base_gas(call: &NativeCall) -> u64 {
    match call {
        NativeCall::RegisterAgent { .. } => 5_000,
        NativeCall::UpdateAgent { .. } => 5_000,
        NativeCall::SuspendAgent { .. } => 5_000,
        NativeCall::SettleReceipt(_) => 8_000,
        NativeCall::SettleBatch(p) => {
            15_000u64.saturating_add(200u64.saturating_mul(p.receipts.len() as u64))
        }
        NativeCall::ClaimPayout { .. } => 12_000,
        NativeCall::FileDispute { .. } => 10_000,
        NativeCall::ResolveDispute { .. } => 12_000,
        NativeCall::Stake { .. } => 8_000,
        NativeCall::Unstake { .. } => 8_000,
        NativeCall::Slash { .. } => 10_000,
        NativeCall::Delegate { .. } => 8_000,
        NativeCall::WithdrawRewards { .. } => 8_000,
        NativeCall::WalletTaskReceiptAnchorV1 { .. } => 6_000,
        NativeCall::ProofCommitmentV1 { .. } => 6_000,
        NativeCall::LifeCommandV1(_) => 9_000,
        NativeCall::NoOp => 100,
        NativeCall::SetChainEconomics { .. } => 10_000,
    }
}

/// `0xFC00..0xFCFF` reserved precompile namespace (PRD §9.3): second byte = opcode when first is `0xfc`.
pub fn is_native_precompile_address(addr: &Address) -> bool {
    addr[0] == 0xfc && addr[1] >= OP_RANGE.0 && addr[1] <= OP_RANGE.1
}

const OP_RANGE: (u8, u8) = (0x01, 0x0e);

pub fn native_opcode_from_precompile_address(addr: &Address) -> Option<u8> {
    if addr[0] != 0xfc {
        return None;
    }
    let op = addr[1];
    if op >= OP_RANGE.0 && op <= OP_RANGE.1 {
        Some(op)
    } else {
        None
    }
}
