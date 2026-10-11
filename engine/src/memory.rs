//! Sunday's memory across apps and coding agents (issue #9, A6), through
//! Hermes's own memory and scheduled jobs. Hermes's built-in memory
//! (MEMORY.md and USER.md) is always on; a memory provider adds long-term
//! memory; and an opt-in digest job keeps it up to date with what the user did
//! in their coding agents. OpenAgora runs Hermes's own commands, for the
//! `sunday` profile only, after the user agrees.

use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::apps::Engine;
use crate::broker::{self, Scope};
use crate::capture::SUNDAY;
use crate::manifest::Manifest;
use crate::paths::Paths;
use crate::sessions;
use crate::state;
use crate::sunday::Hermes;

/// The digest job's name in Hermes's scheduler.
pub const DIGEST_JOB: &str = "openagora-digest";
/// How often the digest runs.
pub const DIGEST_SCHEDULE: &str = "every 2h";
/// How often OpenAgora runs Sunday's due jobs while the digest is on
/// (`OPENAGORA_TICK_SECONDS` for tests).
pub fn tick_every() -> Duration {
    let seconds = std::env::var("OPENAGORA_TICK_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|s| *s > 0)
        .unwrap_or(15 * 60);
    Duration::from_secs(seconds)
}
/// The skill the digest job runs with.
pub const DIGEST_SKILL: &str = "openagora-memory";

/// What the digest job asks Sunday to do, each time it runs.
pub const DIGEST_PROMPT: &str = "OpenAgora digest. With the openagora tools, call coding_activity with since set to the time your previous digest covered up to (it is in your previous output, if there is one), or \"today\" on the first run. Save to memory only what will still matter later: decisions the user made, conventions, facts about their projects and problems left open, each with its project's name. Never save code, secrets, keys or personal data, nor what memory already holds. Then answer in one line: what you saved, ending with \"Covered up to <the 'to' time coding_activity gave>.\"";

/// What OpenAgora remembers about the digest.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Saved {
    digest: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    digest_job: Option<String>,
}

fn saved_file(paths: &Paths) -> std::path::PathBuf {
    paths.root.join("memory.toml")
}

fn saved(paths: &Paths) -> Saved {
    state::read(&saved_file(paths))
        .ok()
        .flatten()
        .unwrap_or_default()
}

/// Where Sunday's memory stands.
#[derive(Debug, Serialize)]
pub struct Status {
    /// The memory provider Hermes uses for Sunday, or None for built-in only.
    pub provider: Option<String>,
    /// Whether the digest job is in Sunday's scheduler.
    pub digest: bool,
    /// When the digest last ran, as Hermes recorded it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub digest_last_run: Option<String>,
    /// The coding agents whose sessions OpenAgora can read, and whether Sunday
    /// may always read them.
    pub sessions: Vec<AgentSessions>,
}

#[derive(Debug, Serialize)]
pub struct AgentSessions {
    pub agent: String,
    pub name: String,
    pub always: bool,
}

fn hermes(engine: &Engine, m: &Manifest) -> Result<Hermes> {
    Hermes::find(engine, m).with_context(|| {
        format!(
            "{} is not installed. Install it first: openagora install {}",
            m.name, m.id
        )
    })
}

/// Sunday's job named `name`, from its scheduler's own file (read only).
fn job(profile: &Path, name: &str) -> Option<Value> {
    let text = std::fs::read_to_string(profile.join("cron").join("jobs.json")).ok()?;
    let jobs: Value = serde_json::from_str(&text).ok()?;
    jobs["jobs"]
        .as_array()?
        .iter()
        .find(|j| j["name"] == name)
        .cloned()
}

/// The coding agents whose sessions OpenAgora can read on this computer.
pub fn readable_agents() -> Vec<(String, String)> {
    let Ok(home) = crate::paths::home() else {
        return Vec::new();
    };
    sessions::readers_at(&home)
        .iter()
        .map(|r| (r.agent().to_string(), r.name().to_string()))
        .collect()
}

pub fn status(engine: &Engine, m: &Manifest) -> Result<Status> {
    let hermes = hermes(engine, m)?;
    let dir = hermes
        .profile_dir()?
        .context("Sunday isn't set up yet. Set it up first: openagora sunday setup")?;
    let provider = hermes
        .config_get("memory.provider")?
        .and_then(|v| v.as_str().map(str::to_string))
        .filter(|p| !p.is_empty());
    let job = job(&dir, DIGEST_JOB);
    let sessions = readable_agents()
        .into_iter()
        .map(|(agent, name)| AgentSessions {
            always: matches!(
                broker::allowed(
                    &engine.paths,
                    SUNDAY,
                    &sessions::source_key(&agent),
                    SUNDAY,
                    "sessions/*"
                ),
                Ok(Some(Scope::Always))
            ),
            agent,
            name,
        })
        .collect();
    Ok(Status {
        provider,
        digest: job.is_some(),
        digest_last_run: job
            .as_ref()
            .and_then(|j| j["last_run_at"].as_str().map(str::to_string)),
        sessions,
    })
}

/// Lets Sunday always read an agent's sessions.
pub fn allow_sessions(paths: &Paths, agent: &str, name: &str) -> Result<()> {
    broker::grant(
        paths,
        broker::Grant {
            client: SUNDAY.into(),
            source: sessions::source_key(agent),
            source_name: format!("{name} sessions"),
            destination: SUNDAY.into(),
            family: "sessions/*".into(),
            scope: Scope::Always,
            granted_at: state::now(),
        },
    )
}

/// Takes back Sunday's permission to read an agent's sessions.
pub fn revoke_sessions(paths: &Paths, agent: &str) -> Result<usize> {
    broker::revoke_where(paths, &sessions::source_key(agent), Some(SUNDAY), None)
}

/// The digest job: what it reads (the agents named) and Hermes's job for it.
/// Returns the job's id.
pub fn digest_on(engine: &Engine, m: &Manifest, agents: &[(String, String)]) -> Result<String> {
    let hermes = hermes(engine, m)?;
    let dir = hermes
        .profile_dir()?
        .context("Sunday isn't set up yet. Set it up first: openagora sunday setup")?;
    for (agent, name) in agents {
        allow_sessions(&engine.paths, agent, name)?;
    }
    let id = match job(&dir, DIGEST_JOB) {
        Some(existing) => existing["id"].as_str().unwrap_or(DIGEST_JOB).to_string(),
        None => hermes.cron_create(DIGEST_SCHEDULE, DIGEST_PROMPT, DIGEST_JOB, &[DIGEST_SKILL])?,
    };
    state::write(
        &saved_file(&engine.paths),
        &Saved {
            digest: true,
            digest_job: Some(id.clone()),
        },
    )?;
    Ok(id)
}

/// Removes the digest job from Sunday's scheduler. True if there was one.
pub fn digest_off(engine: &Engine, m: &Manifest) -> Result<bool> {
    let hermes = hermes(engine, m)?;
    let mut saved = saved(&engine.paths);
    let existed = match hermes.profile_dir()?.and_then(|dir| job(&dir, DIGEST_JOB)) {
        Some(job) => {
            hermes.cron_remove(job["id"].as_str().unwrap_or(DIGEST_JOB))?;
            true
        }
        None => false,
    };
    saved.digest = false;
    saved.digest_job = None;
    state::write(&saved_file(&engine.paths), &saved)?;
    Ok(existed)
}

/// Whether OpenAgora should run Sunday's due jobs: the user turned the digest
/// on.
pub fn digest_wanted(paths: &Paths) -> bool {
    saved(paths).digest
}

/// Runs Sunday's due jobs once, with Hermes's own `cron tick` (Hermes's lock
/// keeps it to one tick at a time, alongside its gateway).
pub fn tick(engine: &Engine, m: &Manifest) -> Result<()> {
    hermes(engine, m)?.cron_tick()
}

impl crate::gateway::Gateway {
    /// Runs Sunday's due jobs once, if the user turned the digest on.
    pub fn tick_sunday_jobs(&self) -> Result<()> {
        if !digest_wanted(&self.paths) {
            return Ok(());
        }
        let catalog = self.catalog()?;
        let Some(m) = catalog.agent() else {
            return Ok(());
        };
        tick(&Engine::new()?, m)
    }
}
