use fractal_consensus::{
    eth_signed_raws_for_txs, execute_and_build_block, genesis_parent_qc, hash_qc,
};
use fractal_core::{EmissionParams, State};

#[test]
fn block_build_mints_emission_into_protocol_pools() {
    let mut state = State::default();
    state.chain_economics.emission = EmissionParams {
        total_pool_wei: 1_000,
        quarter_count: 1,
        blocks_per_quarter: 10,
        ..EmissionParams::fractal_emission_v3()
    };

    let block = execute_and_build_block(
        41,
        1,
        0,
        [0u8; 32],
        hash_qc(&genesis_parent_qc()).unwrap(),
        [1u8; 32],
        1_000,
        1_000_000,
        &mut state,
        Vec::new(),
        eth_signed_raws_for_txs(0),
    )
    .unwrap();

    assert_eq!(block.header.height, 1);
    assert_eq!(state.protocol_minted_wei, 100);
    assert_eq!(state.emission_provider_pool_wei, 55);
    assert_eq!(state.emission_consensus_pool_wei, 20);
    assert_eq!(state.emission_intelligence_pool_wei, 25);
    assert_eq!(
        state.circulating_supply_wei(),
        state
            .protocol_minted_wei
            .saturating_sub(state.protocol_burned_wei)
    );
}
