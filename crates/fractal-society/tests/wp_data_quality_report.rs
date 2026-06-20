use fractal_society::pkgs::data_quality_report::report;

#[test]
fn complete_sequence_has_full_completeness() {
    let quality = report(&[1, 2, 3, 4], 1, 4);

    assert_eq!(quality.observed, 4);
    assert_eq!(quality.expected, 4);
    assert_eq!(quality.completeness, 1.0);
    assert_eq!(quality.gap_count, 0);
    assert_eq!(quality.missing_count, 0);
}

#[test]
fn gappy_sequence_reports_missing_values() {
    let quality = report(&[1, 2, 4, 7], 1, 7);

    assert!(quality.completeness < 1.0);
    assert_eq!(quality.observed, 4);
    assert_eq!(quality.expected, 7);
    assert_eq!(quality.gap_count, 2);
    assert_eq!(quality.missing_count, 3);
}

#[test]
fn empty_expected_range_has_defined_completeness() {
    let quality = report(&[1, 2, 3], 5, 4);

    assert_eq!(quality.observed, 0);
    assert_eq!(quality.expected, 0);
    assert_eq!(quality.completeness, 1.0);
    assert_eq!(quality.gap_count, 0);
    assert_eq!(quality.missing_count, 0);
}

#[test]
fn duplicates_and_out_of_range_values_do_not_inflate_observed() {
    let quality = report(&[0, 1, 1, 2, 5], 1, 3);

    assert_eq!(quality.observed, 2);
    assert_eq!(quality.expected, 3);
    assert_eq!(quality.missing_count, 1);
}
