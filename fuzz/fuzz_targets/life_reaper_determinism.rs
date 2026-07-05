#![no_main]

use fractal_agent_life::{add_soul, create_initial_life_state, create_soul, reaper_epoch, LifeGenesisParams};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let params: LifeGenesisParams = serde_json::from_str(include_str!("../../crates/agent-life/src/test_params.json")).unwrap();
    let mut a = create_initial_life_state();
    let mut b = create_initial_life_state();
    let n = data.first().copied().unwrap_or(1).min(12);
    for i in 0..n {
        let bal = u64::from(*data.get(i as usize + 1).unwrap_or(&10)).saturating_mul(10_000);
        let soul = create_soul(&params, format!("soul-{i}"), "npc", "owner", bal, 0, None);
        add_soul(&mut a, soul.clone());
        add_soul(&mut b, soul);
    }
    let epoch = u64::from(*data.get(20).unwrap_or(&3)).min(30);
    reaper_epoch(&mut a, &params, epoch).unwrap();
    reaper_epoch(&mut b, &params, epoch).unwrap();
    assert_eq!(a, b);
});
