use borsh::BorshDeserialize;
use fractal_core::{is_native_precompile_address, Address, EvmCallOutcome, EvmEngine, ExecError, NativeCall, State};
use revm::context::Context;
use revm::MainBuilder;
use revm::MainContext;

/// Minimal `revm`-backed engine (M4 initial slice).
///
/// For now, we only support Fractal native-precompile addresses (`0xfc..`) and route them to
/// `State::apply_native_syscall`. Full bytecode execution + state DB wiring lands next.
#[derive(Default)]
pub struct RevmEngine;

impl RevmEngine {
    fn decode(&self, calldata: &[u8]) -> Result<NativeCall, ExecError> {
        NativeCall::try_from_slice(calldata).map_err(|_| ExecError::InvalidShape)
    }
}

impl EvmEngine for RevmEngine {
    fn execute_call(
        &mut self,
        state: &mut State,
        caller: Address,
        to: Address,
        _value: u128,
        calldata: Vec<u8>,
        _gas_limit: u64,
    ) -> Result<EvmCallOutcome, ExecError> {
        // This "wires in" revm in the sense that we construct a mainnet context here.
        // The actual execution path for native precompiles is routed directly and deterministically.
        let _evm = Context::mainnet().build_mainnet();

        if !is_native_precompile_address(&to) {
            return Err(ExecError::InvalidShape);
        }

        let call = self.decode(&calldata)?;
        state.apply_native_syscall(caller, &call)?;

        Ok(EvmCallOutcome {
            gas_used: 0,
            return_data: Vec::new(),
        })
    }
}

