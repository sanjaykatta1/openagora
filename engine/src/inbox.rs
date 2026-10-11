//! What Sunday sent each coding agent: its inbox. The items themselves stay
//! on the desk, shared with the agent; the inbox says who sent each one, when,
//! with what message, and whether the agent was told about it or read it.

use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::desk::{self, Item};
use crate::paths::Paths;
use crate::state;

/// Inboxes change one at a time.
static WRITE: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sent {
    /// The desk item.
    pub item: String,
    /// Who sent it, e.g. `sunday`.
    pub from: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub sent_at: u64,
    /// When the agent's prompt hook told it about the item.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub told_at: Option<u64>,
    /// When the agent read it with `context_get`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_at: Option<u64>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Inbox {
    #[serde(default)]
    sent: Vec<Sent>,
}

fn file(paths: &Paths, agent: &str) -> PathBuf {
    paths.root.join("inbox").join(format!("{agent}.toml"))
}

fn change(paths: &Paths, agent: &str, edit: impl FnOnce(&mut Vec<Sent>)) -> Result<()> {
    let _guard = WRITE.lock().unwrap();
    let path = file(paths, agent);
    let mut inbox = state::read::<Inbox>(&path)?.unwrap_or_default();
    edit(&mut inbox.sent);
    state::write(&path, &inbox)
}

/// Puts an item in an agent's inbox; sending it again brings it back to the
/// top, with the new message.
pub fn add(paths: &Paths, agent: &str, sent: Sent) -> Result<()> {
    change(paths, agent, |all| {
        all.retain(|s| s.item != sent.item);
        all.push(sent);
    })
}

/// What's in an agent's inbox, newest first, with the items still on the
/// desk (expired ones are left out).
pub fn list(paths: &Paths, agent: &str) -> Result<Vec<(Sent, Item)>> {
    let inbox = state::read::<Inbox>(&file(paths, agent))?.unwrap_or_default();
    let mut all: Vec<(Sent, Item)> = inbox
        .sent
        .into_iter()
        .filter_map(|s| {
            let item = desk::get(paths, &s.item).ok().flatten()?;
            item.is_shared_with(agent).then_some((s, item))
        })
        .collect();
    all.sort_by_key(|s| std::cmp::Reverse(s.0.sent_at));
    Ok(all)
}

/// Notes that the agent was told about these items (by its prompt hook).
pub fn mark_told(paths: &Paths, agent: &str, items: &[String]) -> Result<()> {
    let now = state::now();
    change(paths, agent, |all| {
        for s in all.iter_mut().filter(|s| items.contains(&s.item)) {
            s.told_at.get_or_insert(now);
        }
    })
}

/// Notes that the agent read an item.
pub fn mark_read(paths: &Paths, agent: &str, item: &str) -> Result<()> {
    let now = state::now();
    change(paths, agent, |all| {
        for s in all.iter_mut().filter(|s| s.item == item) {
            s.read_at.get_or_insert(now);
        }
    })
}
