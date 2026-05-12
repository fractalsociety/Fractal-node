// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.20;

import {FractalNative} from "./FractalNative.sol";

/// @title AgentBountyEscrow (PRD M4 example)
/// @notice Minimal pattern contract: on-chain bounty bookkeeping plus a **Fractal native precompile** call.
/// @dev This is devnet documentation-grade Solidity, not a production escrow (no fund custody until `EvmCall` value is enabled on-chain).
contract AgentBountyEscrow {
    using FractalNative for uint8;

    /// @dev Default precompile slot used for generic native probes (`0xFC01…00`).
    uint8 public constant DEFAULT_NATIVE_SLOT = 0x01;

    event BountyOpened(bytes32 indexed bountyId, address indexed poster);
    event NativeCallResult(bool success);

    mapping(bytes32 bountyId => address poster) public posterOf;

    /// @notice Record a bounty poster (tFRAC / reward mechanics are off-chain or future VM work).
    function openBounty(bytes32 bountyId) external {
        require(posterOf[bountyId] == address(0), "bounty exists");
        posterOf[bountyId] = msg.sender;
        emit BountyOpened(bountyId, msg.sender);
    }

    /// @notice Example native bridge: `NativeCall::NoOp` borsh wire is a single byte `0x0d` (see core tests).
    function pingNativeNoOp() external {
        bytes memory calldata_ = hex"0d";
        (bool ok,) = DEFAULT_NATIVE_SLOT.syscallCall(calldata_);
        emit NativeCallResult(ok);
    }

    /// @notice Forward arbitrary borsh-encoded `NativeCall` bytes to the default syscall slot.
    function forwardNative(bytes calldata borshCalldata) external {
        bytes memory m = borshCalldata;
        (bool ok,) = DEFAULT_NATIVE_SLOT.syscallCall(m);
        emit NativeCallResult(ok);
    }
}
