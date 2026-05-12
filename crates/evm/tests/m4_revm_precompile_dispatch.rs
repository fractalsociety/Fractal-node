use fractal_core::{apply_block_with_evm, Account, NativeCall, State, Transaction, TxBody, VmKind, Address};

fn addr(byte0: u8, byte1: u8) -> Address {
    let mut a = [0u8; 20];
    a[0] = byte0;
    a[1] = byte1;
    a
}

#[test]
fn evm_call_to_fc_precompile_dispatches_native_syscall() {
    let caller = addr(0x11, 0x22);
    let to_precompile = addr(0xfc, 0x01);

    let mut st = State::default();
    st.accounts.insert(
        caller,
        Account {
            nonce: 0,
            balance: 1_000_000,
        },
    );

    let tx = Transaction {
        signer: caller,
        nonce: 0,
        vm: VmKind::Evm,
        body: TxBody::EvmCall {
            to: to_precompile,
            value: 0,
            calldata: borsh::to_vec(&NativeCall::NoOp).unwrap(),
            gas_limit: 1_000_000,
        },
    };

    let mut evm = fractal_evm::RevmEngine::default();
    apply_block_with_evm(&mut st, &[tx], &mut evm).expect("evm call should succeed");

    assert_eq!(st.accounts.get(&caller).unwrap().nonce, 1);
}

