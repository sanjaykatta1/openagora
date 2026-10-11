//! Where Sunday hands work over: coding agents and apps, every one behind the
//! same interface (issue #9, B8). There are two ways in:
//!
//! - **Pull**, for any agent that speaks MCP: items go into the agent's inbox,
//!   shared with it on the desk, and it reads them with its own tools
//!   (`context_inbox`, `context_get`). This is the baseline for every agent.
//! - **Push**, for apps with a built-in adapter (T3 Code first, in `t3.rs`):
//!   the engine delivers items itself and can start tasks and report on them.
//!
//! Nothing here knows how Sunday asks; the gateway (`handoff.rs`) checks the
//! user's permission and writes the audit log before calling a destination.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use crate::desk::{self, Item};
use crate::github::sha256_hex;
use crate::inbox::{self, Sent};
use crate::paths::Paths;
use crate::state;

/// A task a destination runs for the user, e.g. a T3 Code thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    /// OpenAgora's id for it, `t-` and ten hex digits.
    pub id: String,
    pub destination: String,
    pub title: String,
    pub created_at: u64,
    /// Who started it, e.g. `sunday`.
    pub started_by: String,
    /// What the destination calls it (a T3 thread, run and project).
    #[serde(default)]
    pub remote: BTreeMap<String, String>,
    /// The desk items it started with.
    #[serde(default)]
    pub items: Vec<String>,
    /// A link that opens it in the destination, when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
}

/// How a task is going.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progress {
    /// The destination's own word for it, e.g. `running`, `completed`.
    pub state: String,
    pub finished: bool,
}

/// One destination, whatever it is.
pub trait Destination {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    /// The MIME types or families it takes, e.g. `image/*`.
    fn accepts(&self) -> Vec<String>;
    /// Whether items go only into a task it runs (no inbox of its own).
    fn needs_task(&self) -> bool {
        false
    }
    /// Hands one item over: into the agent's inbox, or into `task`. Says
    /// where it went.
    fn deliver(&self, item: &Item, task: Option<&Task>, message: Option<&str>) -> Result<String>;
    /// Starts a task with `prompt` and `items`, in `project` where the
    /// destination has projects.
    fn start_task(
        &self,
        title: &str,
        prompt: &str,
        items: &[Item],
        project: Option<&str>,
    ) -> Result<Task>;
    /// How the task is going, waiting up to `wait` for it to finish.
    fn status(&self, task: &Task, wait: Duration) -> Result<Progress>;
    /// What the task produced, as text.
    fn result(&self, task: &Task) -> Result<String>;
}

/// Whether `mime` is one of the types in `accepts` (types or `family/*`).
pub fn takes(accepts: &[String], mime: &str) -> bool {
    let family = desk::family(mime);
    accepts
        .iter()
        .any(|a| a == mime || *a == family || a == "*/*")
}

/// A coding agent that reads what it's sent with its own MCP tools.
pub struct Pull {
    pub paths: Paths,
    pub id: String,
    pub name: String,
    /// Who sends: `sunday`.
    pub from: String,
}

impl Destination for Pull {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn accepts(&self) -> Vec<String> {
        // `context_get` gives pictures as images, text as text, and other
        // files as embedded resources.
        ["image/*", "text/*", "application/*"]
            .map(String::from)
            .to_vec()
    }

    fn deliver(&self, item: &Item, task: Option<&Task>, message: Option<&str>) -> Result<String> {
        if task.is_some() {
            bail!(
                "{} has no tasks OpenAgora can add to; leave out the task",
                self.name
            );
        }
        if desk::share(&self.paths, &item.id, &self.id)?.is_none() {
            bail!("{} is no longer on the desk", item.id);
        }
        inbox::add(
            &self.paths,
            &self.id,
            Sent {
                item: item.id.clone(),
                from: self.from.clone(),
                message: message
                    .map(str::trim)
                    .filter(|m| !m.is_empty())
                    .map(str::to_string),
                sent_at: state::now(),
                told_at: None,
                read_at: None,
            },
        )?;
        Ok(format!("{}'s inbox", self.name))
    }

    fn start_task(&self, _: &str, _: &str, _: &[Item], _: Option<&str>) -> Result<Task> {
        bail!(
            "OpenAgora can't start tasks in {} yet. Use send_to: it puts things in {}'s inbox, and the user asks it \"what did Sunday send me?\"",
            self.name,
            self.name
        )
    }

    fn status(&self, _: &Task, _: Duration) -> Result<Progress> {
        bail!("{} runs no tasks for OpenAgora", self.name)
    }

    fn result(&self, _: &Task) -> Result<String> {
        bail!("{} runs no tasks for OpenAgora", self.name)
    }
}

fn tasks_dir(paths: &Paths) -> PathBuf {
    paths.root.join("tasks")
}

/// Task ids are `t-` and ten hex digits.
fn valid_task_id(id: &str) -> bool {
    id.strip_prefix("t-")
        .is_some_and(|hex| hex.len() == 10 && hex.chars().all(|c| c.is_ascii_hexdigit()))
}

/// A fresh task id.
pub fn new_task_id(seed: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let digest = sha256_hex(format!("{seed}:{nanos}:{}", std::process::id()).as_bytes());
    format!("t-{}", &digest[..10])
}

pub fn save_task(paths: &Paths, task: &Task) -> Result<()> {
    state::write(&tasks_dir(paths).join(format!("{}.toml", task.id)), task)
}

pub fn task(paths: &Paths, id: &str) -> Result<Option<Task>> {
    if !valid_task_id(id) {
        return Ok(None);
    }
    state::read(&tasks_dir(paths).join(format!("{id}.toml")))
}

/// Every task OpenAgora started, newest first.
pub fn tasks(paths: &Paths) -> Result<Vec<Task>> {
    let Ok(entries) = std::fs::read_dir(tasks_dir(paths)) else {
        return Ok(Vec::new());
    };
    let mut all: Vec<Task> = entries
        .flatten()
        .filter_map(|e| state::read::<Task>(&e.path()).ok().flatten())
        .collect();
    all.sort_by_key(|t| std::cmp::Reverse(t.created_at));
    Ok(all)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn types_match_by_family() {
        let accepts = vec!["image/png".to_string(), "text/*".to_string()];
        assert!(takes(&accepts, "image/png"));
        assert!(!takes(&accepts, "image/webp"));
        assert!(takes(&accepts, "text/markdown"));
        assert!(!takes(&accepts, "application/pdf"));
        let id = new_task_id("x");
        assert!(valid_task_id(&id), "{id}");
        assert!(!valid_task_id("t-../../etc"));
    }
}
