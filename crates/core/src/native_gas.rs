//! PRD §9.2 fixed native gas (+ 4 gas / calldata byte on the native payload).

use crate::address::Address;
use crate::error::ExecError;
use crate::tx::{NativeCall, Transaction, TxBody, VmKind};

pub const PER_BYTE: u64 = 4;
pub const TRANSFER_GAS: u64 = 21_000;

pub fn intrinsic_gas(tx: &Transaction) -> Result<u64, ExecError> {
    let native_payload = match (&tx.vm, &tx.body) {
        (VmKind::Native, TxBody::Native(c)) => borsh::to_vec(c).map_err(|_| ExecError::InvalidShape)?,
        (VmKind::Evm, TxBody::Transfer { .. }) => return Ok(TRANSFER_GAS),
        _ => return Err(ExecError::InvalidShape),
    };
    let base = native_base_gas(match &tx.body {
        TxBody::Native(c) => c,
        _ => unreachable!(),
    });
    let extra = PER_BYTE.saturating_mul(native_payload.len() as u64);
    Ok(base.saturating_add(extra))
}

fn native_base_gas(call: &NativeCall) -> u64 {
    match call {
        NativeCall::RegisterAgent { .. } => 5_000,
        NativeCall::UpdateAgent { .. } => 5_000,
        NativeCall::SuspendAgent { .. } => 5_000,
        NativeCall::SettleReceipt(_) => 8_000,
        NativeCall::SettleBatch(p) => 15_000u64.saturating_add(200u64.saturating_mul(p.receipts.len() as u64)),
        NativeCall::ClaimPayout { .. } => 12_000,
        NativeCall::FileDispute { .. } => 10_000,
        NativeCall::ResolveDispute { .. } => 12_000,
        NativeCall::Stake { .. } => 8_000,
        NativeCall::Unstake { .. } => 8_000,
        NativeCall::Slash { .. } => 10_000,
        NativeCall::Delegate { .. } => 8_000,
        NativeCall::WithdrawRewards { .. } => 8_000,
        NativeCall::NoOp => 100,
    }
}

/// `0xFC00..0xFCFF` reserved precompile namespace (PRD §9.3): second byte = opcode when first is `0xfc`.
pub fn is_native_precompile_address(addr: &Address) -> bool {
    addr[0] == 0xfc && addr[1] >= OP_RANGE.0 && addr[1] <= OP_RANGE.1
}

const OP_RANGE: (u8, u8) = (0x01, 0x0d);

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
