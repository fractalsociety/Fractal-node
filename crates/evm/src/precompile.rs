//! EVM ↔ native bridge: reserved `0xFC00..0xFCFF` precompile namespace (PRD §9.3).
//!
//! Full `revm` dispatch lands in M4; here we expose address parsing + calldata → [`NativeCall`].

use borsh::BorshDeserialize;
use fractal_core::{Address, NativeCall};

/// `true` if `addr` is in the Fractal native syscall range (`0xFC**…`).
pub fn is_fractal_native_precompile(addr: &Address) -> bool {
    fractal_core::is_native_precompile_address(addr)
}

/// If `addr` encodes a native opcode (second byte `0x01..=0x0E`), returns opcode id.
pub fn native_opcode(addr: &Address) -> Option<u8> {
    fractal_core::native_opcode_from_precompile_address(addr)
}

/// Decode calldata as borsh-encoded [`NativeCall`] (M3 wire format for tooling / future ABI wrapper).
pub fn decode_native_calldata(calldata: &[u8]) -> Option<NativeCall> {
    NativeCall::try_from_slice(calldata).ok()
}
