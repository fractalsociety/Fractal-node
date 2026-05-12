use fractal_core::{Account, Address, EvmEngine, ExecError, State};

fn addr(byte0: u8, byte1: u8) -> Address {
    let mut a = [0u8; 20];
    a[0] = byte0;
    a[1] = byte1;
    a
}

/// PUSH1 0x00 PUSH1 0x00 REVERT — revert with empty return data.
const REVERT_EMPTY: [u8; 5] = [0x60, 0x00, 0x60, 0x00, 0xfd];

#[test]
fn evm_call_revert_maps_to_exec_error_with_return_data() {
    let caller = addr(0x10, 0x01);
    let contract = addr(0x20, 0x02);

    let mut st = State::default();
    st.accounts.insert(
        caller,
        Account {
            nonce: 0,
            balance: 1_000_000,
        },
    );
    st.accounts.insert(
        contract,
        Account {
            nonce: 0,
            balance: 0,
        },
    );
    st.evm_code.insert(contract, REVERT_EMPTY.to_vec());

    let mut evm = fractal_evm::RevmEngine::default();
    let err = evm
        .execute_call(&mut st, caller, contract, 0, vec![], 1_000_000)
        .expect_err("expected revert");

    assert_eq!(
        err,
        ExecError::EvmRevert {
            return_data: Vec::new()
        }
    );
}
