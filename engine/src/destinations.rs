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
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use crate::desk::{self, Item};
use crate::github::sha256_hex;
use crate::inbox::{self, Sent};
use crate::jsonrpc::Peer;
use crate::paths::Paths;
use crate::state;

/// The built-in fallback that puts an item on the clipboard.
pub const CLIPBOARD: &str = "clipboard";
/// The built-in fallback that saves items into a folder Sunday names.
pub const FOLDER: &str = "folder";

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
    /// Whether the user may allow it "always". Not where each handoff goes
    /// somewhere new (a folder Sunday names).
    fn may_always(&self) -> bool {
        true
    }
    /// Whether it takes one item at a time (the clipboard holds one).
    fn one_item(&self) -> bool {
        false
    }
    /// How OpenAgora's question puts the handoff: "send to", "save into".
    fn verb(&self) -> &str {
        "send to"
    }
    /// What happens to what's handed over, for the user's question.
    fn consequence(&self) -> String {
        format!(
            "{} can then see them, and so can the model provider it uses. OpenAgora sends nothing else.",
            self.name()
        )
    }
    /// What Sunday tells the user once items went to `places` (what deliver
    /// returned).
    fn next_step(&self, places: &[String]) -> String {
        let _ = places;
        String::new()
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

    fn next_step(&self, _: &[String]) -> String {
        format!(
            "In {name}, the user can ask \"what did Sunday send me?\": {name} reads its inbox with OpenAgora's tools.",
            name = self.name
        )
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

/// How a fallback hands an item over, without the app's own tools.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Way {
    /// On the clipboard, for the user to paste where they want.
    Clipboard,
    /// Saved into a folder.
    Folder(PathBuf),
    /// Saved, then opened with a URL in the app's own scheme, with `{file}`
    /// the saved file.
    UrlScheme(String),
}

/// The fallback for any agent or app (issue #9, B8): the clipboard, a
/// folder, or the app's own URL scheme. Also any catalog app that declares
/// one of these as its way in.
pub struct Fallback {
    pub paths: Paths,
    pub id: String,
    pub name: String,
    pub accepts: Vec<String>,
    pub way: Way,
    /// Sunday named the folder: each save is asked for on its own.
    pub ask_each_time: bool,
    /// OpenAgora's window, which writes the clipboard and opens URLs.
    pub desktop: Option<Arc<Peer>>,
}

/// A name for an item's file: its own, made safe, or one from its id.
pub fn file_name(item: &Item) -> String {
    let extension = match item.mime.as_str() {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "text/markdown" => "md",
        "application/json" => "json",
        "text/csv" => "csv",
        "application/pdf" => "pdf",
        "text/uri-list" => "url",
        m if m.starts_with("text/") => "txt",
        _ => "bin",
    };
    let own = item
        .name
        .as_deref()
        .and_then(|n| Path::new(n).file_name())
        .map(|n| {
            n.to_string_lossy()
                .replace(|c: char| c.is_control() || c == '\\', "_")
        })
        .filter(|n| !n.is_empty() && !n.starts_with('.'));
    own.unwrap_or_else(|| format!("sunday-{}.{extension}", item.id))
}

/// A path for people: without the `\\?\` that Windows puts in front of a
/// canonical path.
pub fn shown(path: &Path) -> String {
    let text = path.display().to_string();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC\\") => rest.to_string(),
        _ => text,
    }
}

/// Whether nothing, not even a link, is at `path`.
fn free(path: &Path) -> bool {
    path.symlink_metadata().is_err()
}

/// `dir/name`, or `dir/name-2.ext` and so on when that's taken: nothing is
/// ever overwritten.
fn free_path(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if free(&first) {
        return first;
    }
    let (stem, extension) = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem.to_string(), format!(".{ext}")),
        _ => (name.to_string(), String::new()),
    };
    (2..)
        .map(|n| dir.join(format!("{stem}-{n}{extension}")))
        .find(|p| free(p))
        .unwrap_or(first)
}

/// Saves `bytes` as a new file in `dir`, named after `name`: never over
/// another file, and never through a link.
pub fn save_new(dir: &Path, name: &str, bytes: &[u8]) -> Result<PathBuf> {
    use std::io::Write as _;
    std::fs::create_dir_all(dir)?;
    for _ in 0..5 {
        let file = free_path(dir, name);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&file)
        {
            Ok(mut out) => {
                out.write_all(bytes)?;
                return Ok(file);
            }
            // Taken since we looked: look again.
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
    bail!("couldn't find a free name for {name} in {}", dir.display())
}

impl Fallback {
    fn window(&self) -> Result<&Peer> {
        match &self.desktop {
            Some(desktop) => Ok(desktop),
            None => bail!(
                "OpenAgora's window isn't open, so it can't do that. Ask the user to open OpenAgora."
            ),
        }
    }
}

impl Destination for Fallback {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn accepts(&self) -> Vec<String> {
        self.accepts.clone()
    }

    fn may_always(&self) -> bool {
        !self.ask_each_time
    }

    fn one_item(&self) -> bool {
        self.way == Way::Clipboard
    }

    fn verb(&self) -> &str {
        match (&self.way, self.id.as_str()) {
            (Way::Clipboard, CLIPBOARD) => "put on",
            (Way::Folder(_), FOLDER) => "save into",
            _ => "send to",
        }
    }

    fn consequence(&self) -> String {
        match &self.way {
            Way::Clipboard if self.id == CLIPBOARD => {
                "It goes on the clipboard for you to paste where you want it. Any app can read the clipboard until something else is copied.".into()
            }
            Way::Clipboard => format!(
                "OpenAgora puts it on the clipboard for you to paste into {}. Any app can read the clipboard until something else is copied.",
                self.name
            ),
            Way::Folder(dir) => format!(
                "They're saved in {} as new files; nothing there is overwritten. Anything that reads that folder can then see them.",
                shown(dir)
            ),
            Way::UrlScheme(_) => format!(
                "They're saved in OpenAgora's data folder and opened in {}, which can then see them.",
                self.name
            ),
        }
    }

    fn next_step(&self, places: &[String]) -> String {
        match &self.way {
            Way::Clipboard if self.id == CLIPBOARD => {
                "Tell the user it's on the clipboard, ready to paste where they want it (e.g. into a coding agent's chat).".into()
            }
            Way::Clipboard => format!(
                "Tell the user it's on the clipboard, ready to paste into {}.",
                self.name
            ),
            Way::Folder(_) => format!(
                "Saved as {}. Tell the user where.",
                places.join(", ")
            ),
            Way::UrlScheme(_) => format!(
                "{} opened it (saved as {}).",
                self.name,
                places.join(", ")
            ),
        }
    }

    fn deliver(&self, item: &Item, task: Option<&Task>, _message: Option<&str>) -> Result<String> {
        if task.is_some() {
            bail!("{} runs no tasks; leave out the task", self.name);
        }
        let bytes = desk::payload(&self.paths, item)?;
        match &self.way {
            Way::Clipboard => {
                let params = if desk::image_type(&bytes).is_some() {
                    serde_json::json!({"kind": "image", "type": item.mime, "data": base64_of(&bytes)})
                } else {
                    let text = String::from_utf8(bytes).map_err(|_| {
                        anyhow::anyhow!(
                            "{} isn't text or a picture, so it can't go on the clipboard",
                            item.id
                        )
                    })?;
                    serde_json::json!({"kind": "text", "text": text})
                };
                self.window()?
                    .request("clipboard/write", params, Duration::from_secs(30))
                    .map_err(|e| {
                        anyhow::anyhow!("OpenAgora's window couldn't write the clipboard: {e}")
                    })?;
                Ok("the clipboard (the user pastes it where they want it)".into())
            }
            Way::Folder(dir) => {
                let file = save_new(dir, &file_name(item), &bytes)?;
                Ok(shown(&file))
            }
            Way::UrlScheme(template) => {
                let file = save_new(&self.paths.root.join("handoff"), &file_name(item), &bytes)?;
                let url =
                    template.replace("{file}", &crate::oauth::encode(&file.display().to_string()));
                let scheme = template.split(':').next().unwrap_or_default().to_string();
                self.window()?
                    .request(
                        "url/open",
                        serde_json::json!({"url": url, "scheme": scheme}),
                        Duration::from_secs(30),
                    )
                    .map_err(|e| {
                        anyhow::anyhow!("OpenAgora's window couldn't open {}: {e}", self.name)
                    })?;
                Ok(shown(&file))
            }
        }
    }

    fn start_task(&self, _: &str, _: &str, _: &[Item], _: Option<&str>) -> Result<Task> {
        bail!("{} runs no tasks; send_to hands things over", self.name)
    }

    fn status(&self, _: &Task, _: Duration) -> Result<Progress> {
        bail!("{} runs no tasks", self.name)
    }

    fn result(&self, _: &Task) -> Result<String> {
        bail!("{} runs no tasks", self.name)
    }
}

fn base64_of(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
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
        assert_eq!(shown(Path::new(r"\\?\C:\Users\me")), r"C:\Users\me");
        assert_eq!(shown(Path::new("/home/me")), "/home/me");
        assert!(!valid_task_id("t-../../etc"));
    }
}
