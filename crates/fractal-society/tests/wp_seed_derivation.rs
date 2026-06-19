use std::collections::HashSet;

use fractal_society::pkgs::seed_derivation::{expand, sub_seed};

#[test]
fn sub_seed_is_deterministic_for_same_inputs() {
    assert_eq!(sub_seed(42, "agent"), sub_seed(42, "agent"));
}

#[test]
fn distinct_labels_produce_distinct_seeds() {
    assert_ne!(sub_seed(42, "agent"), sub_seed(42, "dataset"));
}

#[test]
fn expand_yields_requested_number_of_distinct_values() {
    let seeds = expand(7, 64);
    let unique: HashSet<u64> = seeds.iter().copied().collect();

    assert_eq!(seeds.len(), 64);
    assert_eq!(unique.len(), 64);
}

#[test]
fn expand_is_deterministic_across_calls() {
    assert_eq!(expand(99, 16), expand(99, 16));
}
