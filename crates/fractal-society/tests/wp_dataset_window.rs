use chrono::{Duration, Utc};
use fractal_society::pkgs::dataset_window::validate;
use fractal_society::protocol::{DatasetBoundaries, WindowSpec};

fn window(start: i64, end: i64) -> WindowSpec {
    let epoch = chrono::DateTime::<Utc>::from_timestamp(0, 0).unwrap();
    WindowSpec {
        start: epoch + Duration::seconds(start),
        end: epoch + Duration::seconds(end),
        seed: 1,
    }
}

fn valid_boundaries() -> DatasetBoundaries {
    DatasetBoundaries {
        development: window(0, 10),
        validation: window(10, 20),
        evaluation: window(20, 30),
    }
}

#[test]
fn valid_ordered_windows_pass() {
    assert!(validate(&valid_boundaries()).is_ok());
}

#[test]
fn overlapping_windows_fail() {
    let mut boundaries = valid_boundaries();
    boundaries.validation.start = boundaries.development.end - Duration::seconds(1);

    let errors = validate(&boundaries).unwrap_err();

    assert!(errors.iter().any(|error| error.contains("development")));
}

#[test]
fn reversed_window_fails() {
    let mut boundaries = valid_boundaries();
    boundaries.evaluation = window(30, 20);

    let errors = validate(&boundaries).unwrap_err();

    assert!(errors.iter().any(|error| error.contains("evaluation")));
}

#[test]
fn validation_after_evaluation_fails() {
    let mut boundaries = valid_boundaries();
    boundaries.evaluation.start = boundaries.validation.end - Duration::seconds(1);

    let errors = validate(&boundaries).unwrap_err();

    assert!(errors.iter().any(|error| error.contains("validation")));
}
