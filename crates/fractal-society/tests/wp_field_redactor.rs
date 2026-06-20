use fractal_society::pkgs::field_redactor::redact;

#[test]
fn nested_object_path_is_redacted() {
    let value = serde_json::json!({
        "agent": {
            "strategy": "secret",
            "name": "public"
        }
    });

    let redacted = redact(&value, &["agent.strategy"]);

    assert_eq!(redacted["agent"]["strategy"], "REDACTED");
}

#[test]
fn sibling_paths_are_intact() {
    let value = serde_json::json!({
        "agent": {
            "strategy": "secret",
            "name": "public"
        }
    });

    let redacted = redact(&value, &["agent.strategy"]);

    assert_eq!(redacted["agent"]["name"], "public");
    assert_eq!(value["agent"]["strategy"], "secret");
}

#[test]
fn missing_path_is_noop() {
    let value = serde_json::json!({ "agent": { "name": "public" } });

    assert_eq!(redact(&value, &["agent.strategy"]), value);
}

#[test]
fn arrays_are_handled_by_numeric_index() {
    let value = serde_json::json!({
        "orders": [
            { "id": 1, "secret": "a" },
            { "id": 2, "secret": "b" }
        ]
    });

    let redacted = redact(&value, &["orders.1.secret"]);

    assert_eq!(redacted["orders"][0]["secret"], "a");
    assert_eq!(redacted["orders"][1]["secret"], "REDACTED");
}
