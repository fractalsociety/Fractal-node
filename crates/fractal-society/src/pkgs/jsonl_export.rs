//! JSONL export package.
//!
//! Serialize a slice of serializable records to newline-delimited JSON (JSONL)
//! bytes for export/bulk ingest.

/// Serialize records to newline-delimited JSON.
pub fn to_jsonl<T: serde::Serialize>(records: &[T]) -> crate::Result<String> {
    let mut lines = Vec::with_capacity(records.len());
    for record in records {
        lines.push(serde_json::to_string(record)?);
    }
    Ok(lines.join("\n"))
}
