//! Block header `gas_used` reflects revm execution gas for `EvmCall`, not only intrinsic gas.

use fractal_consensus::{execute_and_build_block, genesis_parent_qc};
use fractal_core::{Account, State, Transaction, TxBody, VmKind};

fn addr(byte0: u8, byte1: u8) -> fractal_core::Address {
    let mut a = [0u8; 20];
    a[0] = byte0;
    a[1] = byte1;
    a
}

#[test]
fn block_header_gas_used_reflects_evm_execution() {
    let caller = addr(0x10, 0x01);
    let contract = addr(0x20, 0x02);

    // Same runtime as `m4_eth_call_bytecode`: SSTORE + RETURN (cost >> 21k intrinsic).
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

    let tx = Transaction {
        signer: caller,
        nonce: 0,
        vm: VmKind::Evm,
        body: TxBody::EvmCall {
            to: contract,
            value: 0,
            calldata: vec![],
            gas_limit: 1_000_000,
        },
    };

    let intrinsic = fractal_core::intrinsic_gas(&tx).expect("intrinsic");
    let gq = genesis_parent_qc();
    let block = execute_and_build_block(
            41,
            0,
            1,
        0,
        [7u8; 32],
        gq,
        vec![],
        [0u8; 32],
        1,
        60_000_000,
        &mut st,
        vec![tx],
        fractal_consensus::eth_signed_raws_for_txs(1),
        None,
    )
    .expect("block");

    assert!(
        block.header.gas_used > intrinsic,
        "header gas_used {} should exceed intrinsic {}",
        block.header.gas_used,
        intrinsic
    );
    assert!(block.header.gas_used <= 1_000_000);
}
