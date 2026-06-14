//! PRD M4: a contract’s runtime bytecode may `CALL` into `0xfc01…` and dispatch `NativeCall::NoOp`
//! (same path as `AgentBountyEscrow.pingNativeNoOp`).

use fractal_core::{apply_block_with_evm, Account, Address, State, Transaction, TxBody, VmKind};

fn addr(byte0: u8, byte1: u8) -> Address {
    let mut a = [0u8; 20];
    a[0] = byte0;
    a[1] = byte1;
    a
}

/// Runtime: MSTORE8(0, 0x0d); CALL(gas=1_000_000, to=0xfc01…00, …, in=[0..1), ret=[]).
const RUNTIME: [u8; 37] = [
    0x60, 0x0d, 0x5f, 0x53, // PUSH1 0x0d; PUSH0; MSTORE8
    0x5f, 0x5f, 0x60, 0x01, 0x5f, 0x5f, // retSize, retOff, inSize, inOff, value
    0x73, // PUSH20
    0xfc, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x62, 0x0f, 0x42, 0x40, // PUSH3 1_000_000
    0xf1, // CALL
    0x00, // STOP
];

#[test]
fn contract_runtime_call_to_fc_precompile_succeeds() {
    let caller = addr(0x10, 0x01);
    let contract = addr(0xab, 0xcd);

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
    st.evm_code.insert(contract, RUNTIME.to_vec());

    let tx = Transaction {
        signer: caller,
        nonce: 0,
        vm: VmKind::Evm,
        body: TxBody::EvmCall {
            to: contract,
            value: 0,
            calldata: vec![],
            gas_limit: 2_000_000,
        },
    };

    let mut evm = fractal_evm::RevmEngine::default();
    apply_block_with_evm(&mut st, &[tx], &mut evm)
        .expect("nested call to precompile should succeed");

    assert_eq!(st.accounts.get(&caller).unwrap().nonce, 1);
}
