use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

/// A single history entry stored as JSONL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub timestamp: i64,
    pub nickname: String,
    pub text: String,
}

/// Append an entry to the history file.
pub fn append(path: &Path, entry: &HistoryEntry) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .expect("failed to open history file");
    let line = serde_json::to_string(entry).expect("history serialization failed");
    writeln!(file, "{line}").expect("failed to write history");
}

/// Load the last `n` entries from the history file.
pub fn load_recent(path: &Path, n: usize) -> Vec<HistoryEntry> {
    if !path.exists() {
        return Vec::new();
    }
    let file = fs::File::open(path).expect("failed to open history file");
    let reader = BufReader::new(file);
    let entries: Vec<HistoryEntry> = reader
        .lines()
        .filter_map(|line| {
            let line = line.ok()?;
            serde_json::from_str(&line).ok()
        })
        .collect();
    let skip = entries.len().saturating_sub(n);
    entries.into_iter().skip(skip).collect()
}
