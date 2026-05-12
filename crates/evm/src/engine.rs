use borsh::BorshDeserialize;
use fractal_core::{
    is_native_precompile_address, Address, EvmCallOutcome, EvmEngine, EvmLog, ExecError, NativeCall,
    State,
};
use revm::bytecode::Bytecode;
use revm::context::Context;
use revm::primitives::{Address as RAddress, Bytes, FixedBytes, KECCAK_EMPTY, U256};
use revm::state::{Account, AccountInfo};
use revm::{Database, DatabaseCommit, ExecuteEvm, MainBuilder, MainContext};
use sha3::Digest;
use std::collections::HashMap;
use std::convert::Infallible;

/// Minimal `revm`-backed engine (M4 initial slice).
///
/// Supports:
/// - Fractal native-precompile addresses (`0xfc..`) routed to `State::apply_native_syscall`.
/// - Devnet EVM bytecode CALL execution with persistent storage writes into `State.evm_storage`.
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
        // Fast-path: Fractal native syscalls.
        if is_native_precompile_address(&to) {
            let call = self.decode(&calldata)?;
            state.apply_native_syscall(caller, &call)?;
            return Ok(EvmCallOutcome {
                gas_used: 0,
                return_data: Vec::new(),
                logs: Vec::new(),
            });
        }

        // Devnet EVM CALL: execute stored bytecode using revm with a State-backed DB.
        let mut db = StateDb { st: state };

        let mut evm = Context::mainnet().with_db(&mut db).build_mainnet();
        let mut tx = evm.ctx.tx.clone();
        tx.caller = to_raddr(caller);
        tx.kind = revm::primitives::TxKind::Call(to_raddr(to));
        tx.data = Bytes::from(calldata);
        tx.value = U256::from(0u64);
        tx.gas_limit = _gas_limit;

        let out = evm.transact(tx).map_err(|_| ExecError::InvalidShape)?;
        // Commit state changes (storage/code/balance/nonce) back into StateDb.
        db.commit(out.state);

        let logs = out
            .result
            .logs()
            .iter()
            .map(|l| EvmLog {
                address: {
                    let mut a = [0u8; 20];
                    a.copy_from_slice(l.address.as_slice());
                    a
                },
                topics: l
                    .data
                    .topics()
                    .iter()
                    .map(|t| t.as_slice().try_into().unwrap_or([0u8; 32]))
                    .collect(),
                data: l.data.data.to_vec(),
            })
            .collect::<Vec<_>>();

        Ok(EvmCallOutcome {
            gas_used: out.result.tx_gas_used(),
            return_data: out
                .result
                .output()
                .map(|b| b.to_vec())
                .unwrap_or_default(),
            logs,
        })
    }
}

fn to_raddr(a: Address) -> RAddress {
    RAddress::from_slice(&a)
}

fn to_h256(v: U256) -> [u8; 32] {
    v.to_be_bytes::<32>()
}

fn keccak(bytes: &[u8]) -> [u8; 32] {
    let mut h = sha3::Keccak256::new();
    h.update(bytes);
    h.finalize().into()
}

struct StateDb<'a> {
    st: &'a mut State,
}

impl<'a> Database for StateDb<'a> {
    type Error = Infallible;

    fn basic(&mut self, address: RAddress) -> Result<Option<AccountInfo>, Self::Error> {
        let mut a = [0u8; 20];
        a.copy_from_slice(address.as_slice());

        let (nonce, balance) = self
            .st
            .accounts
            .get(&a)
            .map(|acc| (acc.nonce, acc.balance))
            .unwrap_or((0, 0));

        let code = self.st.evm_code.get(&a).cloned().unwrap_or_default();
        let code_hash: FixedBytes<32> = if code.is_empty() {
            KECCAK_EMPTY
        } else {
            keccak(&code).into()
        };

        let mut info = AccountInfo::new(U256::from(balance), nonce, code_hash, Bytecode::new_raw(Bytes::from(code)));
        // If code is empty, keep default empty bytecode.
        if info.code_hash == KECCAK_EMPTY {
            info.code = Some(Bytecode::default());
        }
        Ok(Some(info))
    }

    fn code_by_hash(&mut self, code_hash: FixedBytes<32>) -> Result<Bytecode, Self::Error> {
        if code_hash == KECCAK_EMPTY {
            return Ok(Bytecode::default());
        }
        // linear scan: devnet only.
        for code in self.st.evm_code.values() {
            if !code.is_empty() && FixedBytes::<32>::from(keccak(code)) == code_hash {
                return Ok(Bytecode::new_raw(Bytes::from(code.clone())));
            }
        }
        Ok(Bytecode::default())
    }

    fn storage(&mut self, address: RAddress, index: U256) -> Result<U256, Self::Error> {
        let mut a = [0u8; 20];
        a.copy_from_slice(address.as_slice());
        let slot = to_h256(index);
        let v = self
            .st
            .evm_storage
            .get(&(a, slot))
            .copied()
            .unwrap_or([0u8; 32]);
        Ok(U256::from_be_bytes(v))
    }

    fn block_hash(&mut self, _number: u64) -> Result<FixedBytes<32>, Self::Error> {
        Ok(FixedBytes::<32>::ZERO)
    }
}

impl<'a> DatabaseCommit for StateDb<'a> {
    fn commit(&mut self, changes: HashMap<RAddress, Account, revm::primitives::map::FbBuildHasher<20>>) {
        for (addr, acc) in changes {
            let mut a = [0u8; 20];
            a.copy_from_slice(addr.as_slice());

            let info = acc.info;
                self.st
                    .accounts
                    .entry(a)
                    .or_insert(fractal_core::Account { nonce: 0, balance: 0 })
                    .balance = info.balance.try_into().unwrap_or(0);
                self.st
                    .accounts
                    .entry(a)
                    .or_insert(fractal_core::Account { nonce: 0, balance: 0 })
                    .nonce = info.nonce;
                if let Some(code) = info.code {
                    let raw = code.bytecode().to_vec();
                    if !raw.is_empty() {
                        self.st.evm_code.insert(a, raw);
                    }
                }

            for (k, v) in acc.storage {
                let key = to_h256(k);
                let val = v.present_value.to_be_bytes::<32>();
                self.st.evm_storage.insert((a, key), val);
            }
        }
    }
}

