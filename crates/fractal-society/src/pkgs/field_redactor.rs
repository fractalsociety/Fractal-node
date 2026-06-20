//! Field-redaction package.
//!
//! Redacts JSON values at object dot-paths and numeric array indexes.

/// Return a copy of `value` with every node at a dot-path replaced by `"REDACTED"`.
pub fn redact(value: &serde_json::Value, paths: &[&str]) -> serde_json::Value {
    let mut redacted = value.clone();
    for path in paths {
        let segments: Vec<&str> = path
            .split('.')
            .filter(|segment| !segment.is_empty())
            .collect();
        if !segments.is_empty() {
            redact_path(&mut redacted, &segments);
        }
    }
    redacted
}

fn redact_path(value: &mut serde_json::Value, segments: &[&str]) {
    if segments.is_empty() {
        *value = serde_json::Value::String("REDACTED".to_string());
        return;
    }

    match value {
        serde_json::Value::Object(map) => {
            if let Some(next) = map.get_mut(segments[0]) {
                redact_path(next, &segments[1..]);
            }
        }
        serde_json::Value::Array(items) => {
            if let Ok(index) = segments[0].parse::<usize>() {
                if let Some(next) = items.get_mut(index) {
                    redact_path(next, &segments[1..]);
                }
            }
        }
        serde_json::Value::Null
        | serde_json::Value::Bool(_)
        | serde_json::Value::Number(_)
        | serde_json::Value::String(_) => {}
    }
}
