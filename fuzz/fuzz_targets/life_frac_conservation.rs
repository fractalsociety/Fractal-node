#![no_main]

use fractal_agent_life::{add_soul, charge_rent, create_initial_life_state, create_soul, owner_top_up, LifeGenesisParams};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let params: LifeGenesisParams = serde_json::from_str(include_str!("../../crates/agent-life/src/test_params.json")).unwrap();
    let mut state = create_initial_life_state();
    add_soul(&mut state, create_soul(&params, "soul-a", "npc", "owner", 0, 0, None));
    let top_up = u64::from(*data.first().unwrap_or(&20)).saturating_mul(10_000);
    owner_top_up(&mut state, "soul-a", top_up, 0).unwrap();
    let mut paid_total = 0u64;
    for (i, byte) in data.iter().copied().skip(1).take(8).enumerate() {
        let before_balance = state.souls["soul-a"].balance_micro_credits;
        let _ = charge_rent(&mut state, &params, "soul-a", i as u64 + 1, u64::from(byte) * 100, "q3");
        let after_balance = state.souls["soul-a"].balance_micro_credits;
        paid_total = paid_total.saturating_add(before_balance.saturating_sub(after_balance));
    }
    let soul = &state.souls["soul-a"];
    assert_eq!(top_up, soul.balance_micro_credits + paid_total);
});
