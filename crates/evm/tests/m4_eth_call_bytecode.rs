use fractal_core::{Account, Address, EvmEngine, State};

fn addr(byte0: u8, byte1: u8) -> Address {
    let mut a = [0u8; 20];
    a[0] = byte0;
    a[1] = byte1;
    a
}

#[test]
fn evm_call_executes_bytecode_and_persists_storage() {
    let caller = addr(0x10, 0x01);
    let contract = addr(0x20, 0x02);

    // Runtime code:
    // SSTORE(slot=0, value=0x2a)
    // MSTORE(0, 0x2a)
    // RETURN(0, 32)
    //
    // 60 2a 60 00 55 60 2a 60 00 52 60 20 60 00 f3
    let runtime = vec![
        0x60, 0x2a, 0x60, 0x00, 0x55, 0x60, 0x2a, 0x60, 0x00, 0x52, 0x60, 0x20, 0x60, 0x00, 0xf3,
    ];

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
    st.evm_code.insert(contract, runtime);

    let mut evm = fractal_evm::RevmEngine::default();
    let out = evm
        .execute_call(&mut st, caller, contract, 0, vec![], 1_000_000)
        .expect("evm call");

    assert_eq!(out.return_data.len(), 32);
    assert_eq!(out.return_data[31], 0x2a);

    let slot0 = [0u8; 32];
    let stored = st
        .evm_storage
        .get(&(contract, slot0))
        .copied()
        .unwrap_or([0u8; 32]);
    assert_eq!(stored[31], 0x2a);
}
