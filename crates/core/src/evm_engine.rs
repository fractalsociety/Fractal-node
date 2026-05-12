use crate::{Address, ExecError, NativeCall, State};
use borsh::BorshDeserialize;
use crate::state::EvmLog;

/// Deterministic EVM execution interface (M4).
///
/// `fractal-core` owns canonical state transitions; an implementation (e.g. `fractal-evm`)
/// executes EVM bytecode and routes Fractal native syscalls via `State::apply_native_syscall`.
pub trait EvmEngine {
    /// Execute an EVM CALL.
    ///
    /// Implementations must be deterministic given the same inputs and must not bump
    /// the transaction nonce (that is handled by `State`).
    fn execute_call(
        &mut self,
        state: &mut State,
        caller: Address,
        to: Address,
        value: u128,
        calldata: Vec<u8>,
        gas_limit: u64,
    ) -> Result<EvmCallOutcome, ExecError>;

    /// Execute a top-level `CREATE` (init code); on success the runtime bytecode is committed by the engine.
    fn execute_create(
        &mut self,
        state: &mut State,
        caller: Address,
        value: u128,
        init_code: Vec<u8>,
        gas_limit: u64,
    ) -> Result<EvmCallOutcome, ExecError>;

    /// Decode native syscalls when routed from EVM precompiles.
    ///
    /// Default implementation uses borsh M3 wire format.
    fn decode_native_syscall(&self, calldata: &[u8]) -> Result<NativeCall, ExecError> {
        NativeCall::try_from_slice(calldata).map_err(|_| ExecError::InvalidShape)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvmCallOutcome {
    pub gas_used: u64,
    pub return_data: Vec<u8>,
    pub logs: Vec<EvmLog>,
}

