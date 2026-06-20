use fractal_society::pkgs::id_uniqueness::{duplicates, unique};

fn make_ids(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
}

#[test]
fn all_distinct_is_unique_and_has_no_duplicates() {
    let ids = make_ids(&["a", "b", "c"]);

    assert!(unique(&ids));
    assert!(duplicates(&ids).is_empty());
}

#[test]
fn repeated_id_is_listed_once() {
    let ids = make_ids(&["a", "b", "a", "c", "a"]);

    assert!(!unique(&ids));
    assert_eq!(duplicates(&ids), make_ids(&["a"]));
}

#[test]
fn multiple_duplicates_preserve_first_duplicate_order() {
    let ids = make_ids(&["a", "b", "a", "c", "b", "b", "c"]);

    assert_eq!(duplicates(&ids), make_ids(&["a", "b", "c"]));
}
