#![no_main]

use fractal_core::{
    Account, ChainEconomicsParams, LifeCommandKind, LifeCommandV1, NativeCall, State, Transaction,
    TxBody, VmKind,
};
use libfuzzer_sys::fuzz_target;

fn signer() -> [u8; 20] {
    [7u8; 20]
}

fuzz_target!(|data: &[u8]| {
    let mut state = State::default();
    state.governance = signer();
    state.chain_economics = ChainEconomicsParams::mainnet();
    state.accounts.insert(
        signer(),
        Account {
            nonce: 0,
            balance: 1_000_000,
        },
    );

    let raw = data
        .iter()
        .take(16)
        .fold(0u128, |acc, byte| (acc << 8) | u128::from(*byte));
    let amount = raw % 1_000_000;
    state.emission_intelligence_pool_wei = amount;
    let has_royalty = data.get(16).copied().unwrap_or(0) % 2 == 0;
    let counterparty_hash = has_royalty.then_some([3u8; 32]);
    let tx = Transaction {
        signer: signer(),
        nonce: 0,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::LifeCommandV1(LifeCommandV1 {
            command_id: [1u8; 32],
            kind: LifeCommandKind::IntelligencePayout,
            soul_id_hash: [2u8; 32],
            counterparty_hash,
            epoch: u64::from(data.get(17).copied().unwrap_or(1)),
            amount_micro_credits: amount,
            payload_hash: [4u8; 32],
        })),
    };

    state.apply_transaction(&tx).unwrap();

    assert_eq!(state.emission_intelligence_pool_wei, 0);
    let payout_sum: u128 = state.life_payouts.iter().map(|p| p.amount_wei).sum();
    let vesting_sum: u128 = state.life_vesting.values().map(|v| v.amount_wei).sum();
    assert_eq!(payout_sum, amount);
    assert_eq!(vesting_sum, amount);
    assert_eq!(state.life_payouts.len(), state.life_vesting.len());
});
