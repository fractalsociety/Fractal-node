use fractal_society::pkgs::gap_detection::{detect, DataGap};

#[test]
fn detects_multiple_gaps() {
    let gaps = detect(&[1, 2, 4, 7]);

    assert_eq!(
        gaps,
        vec![
            DataGap {
                after: 2,
                before: 4,
                missing_count: 1,
            },
            DataGap {
                after: 4,
                before: 7,
                missing_count: 2,
            },
        ]
    );
}

#[test]
fn contiguous_sequence_has_no_gaps() {
    assert!(detect(&[1, 2, 3, 4]).is_empty());
}

#[test]
fn empty_sequence_has_no_gaps() {
    assert!(detect(&[]).is_empty());
}

#[test]
fn unsorted_input_is_sorted_first() {
    assert_eq!(detect(&[7, 1, 4, 2]), detect(&[1, 2, 4, 7]));
}
