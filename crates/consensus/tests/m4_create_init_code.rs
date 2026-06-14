//! `EvmCreate` runs init code via revm; deployed runtime is returned bytecode, not raw init.

use fractal_consensus::execute_and_build_block;
use fractal_core::{create_contract_address, Account, State, Transaction, TxBody, VmKind};

fn addr(byte0: u8, byte1: u8) -> fractal_core::Address {
    let mut a = [0u8; 20];
    a[0] = byte0;
    a[1] = byte1;
    a
}

#[test]
fn create_deploys_returned_runtime_via_revm() {
    let deployer = addr(0xde, 0x01);
    // Minimal init: copy 1 byte runtime (0x00) from end of init blob, RETURN it.
    let init_code = vec![
        0x60, 0x01, 0x60, 0x0c, 0x60, 0x00, 0x39, 0x60, 0x01, 0x60, 0x00, 0xf3, 0x00,
    ];

    let mut st = State::default();
    st.accounts.insert(
        deployer,
        Account {
            nonce: 0,
            balance: 10_000_000,
        },
    );

    let tx = Transaction {
        signer: deployer,
        nonce: 0,
        vm: VmKind::Evm,
        body: TxBody::EvmCreate {
            value: 0,
            init_code: init_code.clone(),
            gas_limit: 2_000_000,
        },
    };

    let expected = create_contract_address(deployer, 0);
    let block = execute_and_build_block(
        41,
        1,
        0,
        [7u8; 32],
        [0u8; 32],
        [0u8; 32],
        1,
        60_000_000,
        &mut st,
        vec![tx],
        fractal_consensus::eth_signed_raws_for_txs(1),
    )
    .expect("block");

    assert!(block.header.gas_used > 0);
    let code = st.evm_code.get(&expected).expect("deployed code");
    assert_eq!(
        code,
        &vec![0x00],
        "runtime should be returned init output, not full init blob"
    );
    assert_eq!(st.accounts.get(&deployer).unwrap().nonce, 1);
}
