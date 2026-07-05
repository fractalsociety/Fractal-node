#![no_main]

use fractal_agent_life::{add_soul, create_initial_life_state, create_soul, mark_dead, settle_inheritance, LifeGenesisParams};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let params: LifeGenesisParams = serde_json::from_str(include_str!("../../crates/agent-life/src/test_params.json")).unwrap();
    let mut state = create_initial_life_state();
    let estate = u64::from(*data.first().unwrap_or(&99)).saturating_mul(1_000);
    add_soul(&mut state, create_soul(&params, "parent", "npc", "owner", estate, 0, None));
    let child_count = data.get(1).copied().unwrap_or(3).clamp(1, 8);
    for i in 0..child_count {
        add_soul(&mut state, create_soul(&params, format!("child-{i}"), "npc", "owner", 0, 0, Some("parent".to_string())));
    }
    mark_dead(&mut state, "parent", 1, "fuzz").unwrap();
    let payouts = settle_inheritance(&mut state, "parent", 1).unwrap();
    let total: u64 = payouts.iter().map(|(_, amount)| *amount).sum();
    assert_eq!(total, estate);
    let min = payouts.iter().map(|(_, amount)| *amount).min().unwrap_or(0);
    let max = payouts.iter().map(|(_, amount)| *amount).max().unwrap_or(0);
    assert!(max.saturating_sub(min) <= 1);
});
