//! Dataset-window validation package.
//!
//! Validates development, validation, and evaluation window ordering.

use crate::protocol::{DatasetBoundaries, WindowSpec};

/// Validate dataset windows and return all detected errors.
pub fn validate(boundaries: &DatasetBoundaries) -> std::result::Result<(), Vec<String>> {
    let mut errors = Vec::new();

    validate_window("development", &boundaries.development, &mut errors);
    validate_window("validation", &boundaries.validation, &mut errors);
    validate_window("evaluation", &boundaries.evaluation, &mut errors);

    if boundaries.development.end > boundaries.validation.start {
        errors.push("development window must end before or at validation start".to_string());
    }
    if boundaries.validation.start > boundaries.validation.end {
        errors
            .push("validation window start must be before or equal to validation end".to_string());
    }
    if boundaries.validation.end > boundaries.evaluation.start {
        errors.push("validation window must end before or at evaluation start".to_string());
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn validate_window(name: &str, window: &WindowSpec, errors: &mut Vec<String>) {
    if window.start >= window.end {
        errors.push(format!("{name} window start must be before end"));
    }
}
