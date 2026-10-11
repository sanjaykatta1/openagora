//! The audit log: every app action, capture and handoff, one JSON object per
//! line in `<data>/audit/audit.jsonl`. It stays on this computer.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::sync::Mutex;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::paths::Paths;
use crate::state;

/// Keeps lines from interleaving within this process.
static WRITE: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Entry {
    /// Seconds since the Unix epoch.
    pub at: u64,
    /// Who asked: an MCP client (e.g. `sunday`), or `user` for OpenAgora's window.
    pub client: String,
    /// What was asked, e.g. `app_install`.
    pub action: String,
    /// What it was about, e.g. an app id.
    #[serde(default)]
    pub target: String,
    /// `allowed` (no confirmation needed), `confirmed`, `declined`, `refused`.
    pub decision: String,
    /// Where the user answered, when they were asked: `chat` or `window`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub via: Option<String>,
    /// `ok`, or what went wrong.
    pub outcome: String,
}

impl Entry {
    pub fn new(client: &str, action: &str, target: &str) -> Entry {
        Entry {
            at: state::now(),
            client: client.to_string(),
            action: action.to_string(),
            target: target.to_string(),
            decision: "allowed".into(),
            via: None,
            outcome: "ok".into(),
        }
    }
}

pub fn file(paths: &Paths) -> std::path::PathBuf {
    paths.root.join("audit").join("audit.jsonl")
}

pub fn record(paths: &Paths, entry: &Entry) -> Result<()> {
    let path = file(paths);
    let _guard = WRITE.lock().unwrap();
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut out = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("opening {}", path.display()))?;
    // One write per line, so concurrent writers never split a line.
    let line = format!("{}\n", serde_json::to_string(entry)?);
    out.write_all(line.as_bytes())?;
    Ok(())
}

/// The newest `limit` entries, oldest first. Unreadable lines are skipped.
pub fn read(paths: &Paths, limit: usize) -> Result<Vec<Entry>> {
    let text = match fs::read_to_string(file(paths)) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let entries: Vec<Entry> = text
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    Ok(entries[entries.len().saturating_sub(limit)..].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_and_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths {
            root: dir.path().to_path_buf(),
        };
        assert!(read(&paths, 10).unwrap().is_empty());
        let mut entry = Entry::new("sunday", "app_install", "t3code");
        entry.decision = "confirmed".into();
        entry.via = Some("chat".into());
        record(&paths, &entry).unwrap();
        record(&paths, &Entry::new("sunday", "app_start", "t3code")).unwrap();
        fs::write(
            file(&paths),
            format!("{}garbage\n", fs::read_to_string(file(&paths)).unwrap()),
        )
        .unwrap();
        let read = read(&paths, 10).unwrap();
        assert_eq!(read.len(), 2);
        assert_eq!(read[0], entry);
        assert_eq!(read[1].action, "app_start");
    }
}
