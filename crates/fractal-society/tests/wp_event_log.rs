use std::fs;

use fractal_society::persistence::event_log::{Event, EventLog, FileEventLog, InMemoryEventLog};

fn event(id: &str, kind: &str) -> Event {
    Event::new(id, kind, serde_json::json!({ "id": id }))
}

#[test]
fn in_memory_log_replays_in_append_order() {
    let mut log = InMemoryEventLog::new();
    let events = vec![event("1", "run_recorded"), event("2", "proof_committed")];

    assert!(log.append(events[0].clone()).unwrap());
    assert!(log.append(events[1].clone()).unwrap());

    assert_eq!(log.replay().unwrap(), events);
}

#[test]
fn file_log_reopens_and_replays_in_order() {
    let path = temp_path("reopen");
    let events = vec![event("1", "run_recorded"), event("2", "proof_committed")];
    {
        let mut log = FileEventLog::new(&path);
        assert!(log.append(events[0].clone()).unwrap());
        assert!(log.append(events[1].clone()).unwrap());
    }

    let reopened = FileEventLog::new(&path);

    assert_eq!(reopened.replay().unwrap(), events);
    let _ = fs::remove_file(path);
}

#[test]
fn duplicate_event_id_is_not_appended() {
    let path = temp_path("dedupe");
    let mut log = FileEventLog::new(&path);
    let original = event("1", "run_recorded");
    let duplicate = event("1", "proof_committed");

    assert!(log.append(original.clone()).unwrap());
    assert!(!log.append(duplicate).unwrap());

    assert_eq!(log.replay().unwrap(), vec![original]);
    let _ = fs::remove_file(path);
}

fn temp_path(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "fractal_society_wp_event_log_{label}_{}.jsonl",
        std::process::id()
    ))
}
