use fractal_core::{
    emission_budget_for_quarter, emission_for_block, projected_emission_total, quarter_of,
    EmissionParams, MAX_SUPPLY_WEI,
};

#[test]
fn emission_curve_mints_exact_23m_wei_with_q39_remainder() {
    let params = EmissionParams {
        blocks_per_quarter: 7,
        ..EmissionParams::fractal_emission_v3()
    };

    let by_quarter: u128 = (0..params.quarter_count)
        .map(|q| emission_budget_for_quarter(&params, q))
        .sum();
    let by_block: u128 = (0..params.quarter_count * params.blocks_per_quarter)
        .map(|h| emission_for_block(h, &params).total_wei)
        .sum();

    assert_eq!(projected_emission_total(&params), MAX_SUPPLY_WEI);
    assert_eq!(by_quarter, MAX_SUPPLY_WEI);
    assert_eq!(by_block, MAX_SUPPLY_WEI);
    assert!(emission_budget_for_quarter(&params, 39) > 0);
}

#[test]
fn emission_quarters_and_post_decade_zero_are_locked() {
    let params = EmissionParams {
        blocks_per_quarter: 11,
        ..EmissionParams::fractal_emission_v3()
    };

    assert_eq!(quarter_of(0, &params), Some(0));
    assert_eq!(quarter_of(10, &params), Some(0));
    assert_eq!(quarter_of(11, &params), Some(1));
    assert_eq!(quarter_of(39 * 11, &params), Some(39));
    assert_eq!(quarter_of(40 * 11, &params), None);
    assert_eq!(emission_for_block(40 * 11, &params).total_wei, 0);
}

#[test]
fn pool_splits_conserve_every_block_despite_rounding() {
    let params = EmissionParams {
        blocks_per_quarter: 13,
        ..EmissionParams::fractal_emission_v3()
    };

    for h in [0, 1, 12, 13, 39 * 13, 40 * 13 - 1] {
        let block = emission_for_block(h, &params);
        assert_eq!(
            block.provider_pool_wei + block.consensus_pool_wei + block.intelligence_pool_wei,
            block.total_wei
        );
        assert_eq!(
            block.provider_pool_wei,
            block.total_wei * u128::from(params.provider_pool_bps) / 10_000
        );
        assert_eq!(
            block.consensus_pool_wei,
            block.total_wei * u128::from(params.consensus_pool_bps) / 10_000
        );
    }
}
