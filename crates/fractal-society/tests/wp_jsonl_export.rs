use serde::{Deserialize, Serialize};

use fractal_society::pkgs::jsonl_export::to_jsonl;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Record {
    id: String,
    value: i64,
}

#[test]
fn n_records_produce_n_parseable_lines() {
    let records = vec![
        Record {
            id: "a".to_string(),
            value: 1,
        },
        Record {
            id: "b".to_string(),
            value: 2,
        },
    ];

    let jsonl = to_jsonl(&records).unwrap();
    let lines: Vec<&str> = jsonl.lines().collect();

    assert_eq!(lines.len(), 2);
    assert_eq!(
        serde_json::from_str::<Record>(lines[0]).unwrap(),
        records[0]
    );
    assert_eq!(
        serde_json::from_str::<Record>(lines[1]).unwrap(),
        records[1]
    );
}

#[test]
fn empty_input_returns_empty_string() {
    let records: Vec<Record> = Vec::new();

    assert_eq!(to_jsonl(&records).unwrap(), "");
}

#[test]
fn output_has_no_trailing_newline() {
    let records = vec![Record {
        id: "a".to_string(),
        value: 1,
    }];

    let jsonl = to_jsonl(&records).unwrap();

    assert!(!jsonl.ends_with('\n'));
}
