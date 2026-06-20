//! Append-only event log implementations.
//!
//! Events are stored in insertion order. Implementations ignore duplicate event
//! identifiers so appending the same event twice is idempotent.

use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::Result;

/// Durable pipeline event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    /// Stable event identifier used for idempotency.
    pub id: String,
    /// Event type label.
    pub kind: String,
    /// Event payload.
    pub payload: serde_json::Value,
}

impl Event {
    /// Create a new event.
    pub fn new(id: impl Into<String>, kind: impl Into<String>, payload: serde_json::Value) -> Self {
        Self {
            id: id.into(),
            kind: kind.into(),
            payload,
        }
    }
}

/// Append-only event log interface.
pub trait EventLog {
    /// Append `event`; returns `false` if an event with the same id already exists.
    fn append(&mut self, event: Event) -> Result<bool>;

    /// Replay all events in append order.
    fn replay(&self) -> Result<Vec<Event>>;
}

/// In-memory event log.
#[derive(Debug, Default, Clone)]
pub struct InMemoryEventLog {
    events: Vec<Event>,
    seen_ids: HashSet<String>,
}

impl InMemoryEventLog {
    /// Create an empty in-memory event log.
    pub fn new() -> Self {
        Self::default()
    }
}

impl EventLog for InMemoryEventLog {
    fn append(&mut self, event: Event) -> Result<bool> {
        if !self.seen_ids.insert(event.id.clone()) {
            return Ok(false);
        }
        self.events.push(event);
        Ok(true)
    }

    fn replay(&self) -> Result<Vec<Event>> {
        Ok(self.events.clone())
    }
}

/// JSONL file-backed event log.
#[derive(Debug, Clone)]
pub struct FileEventLog {
    path: PathBuf,
}

impl FileEventLog {
    /// Create a file-backed event log at `path`.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Borrow the backing file path.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl EventLog for FileEventLog {
    fn append(&mut self, event: Event) -> Result<bool> {
        if self
            .replay()?
            .iter()
            .any(|existing| existing.id == event.id)
        {
            return Ok(false);
        }

        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        serde_json::to_writer(&mut file, &event)?;
        file.write_all(b"\n")?;
        Ok(true)
    }

    fn replay(&self) -> Result<Vec<Event>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }

        let contents = fs::read_to_string(&self.path)?;
        let mut events = Vec::new();
        let mut seen_ids = HashSet::new();
        for line in contents.lines().filter(|line| !line.trim().is_empty()) {
            let event: Event = serde_json::from_str(line)?;
            if seen_ids.insert(event.id.clone()) {
                events.push(event);
            }
        }
        Ok(events)
    }
}
