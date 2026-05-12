// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.20;

/// @title FractalNative
/// @notice Helpers for FractalChain `0xFC..` native syscall precompiles (PRD §9.3).
/// @dev Calldata must be **borsh-encoded** `NativeCall` as defined in `fractal-core` (`crates/core/src/tx.rs`).
///      Variant order is stability-tested in `crates/core/tests/native_call_borsh_snapshots.rs`.
library FractalNative {
    /// @dev Thrown when `opcode` is outside the reserved `0x01..=0x0d` range.
    error FractalOpcodeOutOfRange();

    /// @notice Computes `address(0xfc || opcode || 0x00 * 18)` (20 bytes).
    function syscallAddress(uint8 opcode) internal pure returns (address a) {
        if (opcode < 0x01 || opcode > 0x0d) revert FractalOpcodeOutOfRange();
        return address(uint160((uint160(0xfc) << 152) | (uint160(opcode) << 144)));
    }

    /// @notice Low-level `CALL` into a Fractal native precompile slot.
    function syscallCall(uint8 opcode, bytes memory borshCalldata)
        internal
        returns (bool ok, bytes memory returndata)
    {
        return syscallAddress(opcode).call(borshCalldata);
    }
}
