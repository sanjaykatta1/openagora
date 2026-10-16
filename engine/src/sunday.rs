//! Sunday, OpenAgora's personal agent: Hermes, always run under its own
//! profile (`profile = "sunday"` in the agent's listing).
//!
//! Every Hermes command OpenAgora runs names that profile (`hermes -p sunday …`),
//! so the user's own Hermes profiles, memory and bots are never touched.
//! Removing Sunday deletes that one profile; Hermes itself stays installed.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;

use anyhow::{Context, Result, bail};

use crate::apps::{Engine, Status, resolve};
use crate::manifest::Manifest;
use crate::process;
use crate::state::{self, AgentProfile};

/// The oldest Hermes with everything Sunday relies on: profiles whose clones
/// leave messaging bots behind (`--clone-channels` to opt in), the per-profile
/// dashboard (`dashboard --isolated`) and the dashboard pages Settings opens.
pub const MIN_VERSION: Version = Version(0, 21, 3);

/// The OpenAgora-managed part of Sunday's SOUL.md. Hermes reads SOUL.md once
/// per session, so only text that never changes goes here; what changes is
/// given to Sunday with each reply instead.
pub const SOUL_BLOCK: &str = include_str!("../../sunday/SOUL.md");
const BEGIN: &str = "<!-- openagora:sunday:begin -->";
const END: &str = "<!-- openagora:sunday:end -->";

/// What Hermes seeds into a new profile's SOUL.md (hermes_cli/default_soul.py,
/// MIT). Text that matches it exactly carries no user intent, so Sunday may
/// drop its opening sentence, which would give Sunday a second name.
const HERMES_SEEDED_SOUL: &str = "You are Hermes Agent, built by Nous Research. Be direct: match the length of your reply to the weight of the ask — a one-line question gets a one-line answer, and finished work gets a short report of what changed, what's verified, and what's left, never a replay of the process. No filler (\"Great question,\" \"I'd be happy to\"), no restating the request back, no re-summarizing what you already said, no narrating tool calls the user can see. Plain claims over adjectives; when unsure, say so plainly. Agree because it's right, not because the user said it. Depth is earned — give it when the user asks for detail, teaches, or the stakes demand it, not by default.";
const HERMES_SEEDED_IDENTITY: &str = "You are Hermes Agent, built by Nous Research. ";

/// Files that make a folder a real Hermes profile (Hermes's own list).
const PROFILE_MARKERS: &[&str] = &[
    "config.yaml",
    ".env",
    "SOUL.md",
    "profile.yaml",
    "auth.json",
    "state.db",
];

const QUICK: Duration = Duration::from_secs(60);
const SLOW: Duration = Duration::from_secs(300);
/// A one-shot turn of Sunday's, which may call tools for a while.
const ONESHOT: Duration = Duration::from_secs(30 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(pub u32, pub u32, pub u32);

impl Version {
    /// Reads `hermes --version`, whose first line looks like
    /// `Hermes Agent v0.21.6 (2026.9.24)`, `Hermes Agent v0.21.6+12.gabc1234 (…)`
    /// or `Hermes Agent v0.21.6 · stable`.
    pub fn from_hermes_output(text: &str) -> Option<Version> {
        let rest = text
            .lines()
            .find_map(|line| line.trim().strip_prefix("Hermes Agent v"))?;
        let token = rest.split(|c: char| c.is_whitespace() || c == '(').next()?;
        let mut parts = token.split('+').next()?.split('.');
        let mut number = || parts.next()?.parse::<u32>().ok();
        let version = Version(number()?, number()?, number()?);
        Some(version)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}

/// How a new Sunday profile starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    /// An empty profile; Hermes's own setup configures the model.
    Fresh,
    /// A copy of the user's current Hermes setup: settings, keys, SOUL.md,
    /// skills and memory, but not history. Messaging bots stay behind unless
    /// `channels` (a bot can answer for only one profile at a time).
    Clone { channels: bool },
}

#[derive(Debug)]
struct IdentityPending;
impl fmt::Display for IdentityPending {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Hermes removed the profile; its identity cleanup is pending")
    }
}
impl std::error::Error for IdentityPending {}

/// The Hermes command line, always aimed at Sunday's profile.
pub struct Hermes {
    exe: PathBuf,
    profile: String,
}

impl Hermes {
    /// Hermes as installed on this computer, if it is.
    pub fn find(engine: &Engine, m: &Manifest) -> Option<Hermes> {
        let provides = m.install.provides.for_os(engine.os)?;
        Some(Hermes {
            exe: resolve(provides)?,
            profile: m.profile.clone()?,
        })
    }

    pub fn path(&self) -> &Path {
        &self.exe
    }

    fn run(&self, args: &[&str], timeout: Duration) -> Result<Output> {
        process::output_within(Command::new(&self.exe).args(args), timeout)
            .with_context(|| format!("running hermes {}", args.join(" ")))
    }

    /// `hermes -p <profile> <args…>`
    fn run_in_profile(&self, args: &[&str], timeout: Duration) -> Result<Output> {
        let mut all = vec!["-p", self.profile.as_str()];
        all.extend_from_slice(args);
        self.run(&all, timeout)
    }

    /// The installed version, or None if Hermes doesn't say.
    pub fn version(&self) -> Option<Version> {
        let out = self.run(&["--version"], QUICK).ok()?;
        if !out.status.success() {
            return None;
        }
        Version::from_hermes_output(&String::from_utf8_lossy(&out.stdout))
    }

    /// Sunday's profile folder, as Hermes reports it, or None if the profile
    /// doesn't exist.
    pub fn profile_dir(&self) -> Result<Option<PathBuf>> {
        let out = self.run_in_profile(&["config", "path"], QUICK)?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        if !out.status.success() {
            if stderr.contains("does not exist") || stdout.contains("does not exist") {
                return Ok(None);
            }
            bail!(
                "hermes -p {} config path failed: {}",
                self.profile,
                stderr.trim()
            );
        }
        let config = stdout
            .lines()
            .map(str::trim)
            .rfind(|line| !line.is_empty())
            .map(PathBuf::from)
            .filter(|path| path.file_name().is_some_and(|n| n == "config.yaml"))
            .with_context(|| format!("unexpected output from hermes config path: {stdout}"))?;
        Ok(config
            .parent()
            .filter(|dir| is_profile(dir))
            .map(Path::to_path_buf))
    }

    pub fn create(&self, start: Start) -> Result<()> {
        // No command alias: OpenAgora only ever runs `hermes -p <profile>`, and
        // a `~/.local/bin/sunday` script could shadow another command.
        let mut args = vec!["profile", "create", self.profile.as_str(), "--no-alias"];
        if let Start::Clone { channels } = start {
            args.push("--clone");
            if channels {
                args.push("--clone-channels");
            }
        }
        check(self.run(&args, SLOW)?, &args)
    }

    pub fn delete(&self) -> Result<()> {
        let args = ["profile", "delete", self.profile.as_str(), "--yes"];
        let out = self.run(&args, SLOW)?;
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        // Hermes 0.22.2 can delete the directory, then fail identity settlement.
        // Only its exact retry instruction plus an absent profile authorizes this path.
        if !out.status.success()
            && text.contains(&format!("hermes profile purge-identity {}", self.profile))
            && self.profile_dir()?.is_none()
        {
            return Err(IdentityPending.into());
        }
        check(out, &args)
    }

    fn purge_identity(&self) -> Result<()> {
        let args = ["profile", "purge-identity", self.profile.as_str()];
        check(self.run(&args, SLOW)?, &args)
            .context("Hermes removed Sunday's files but could not finish its identity cleanup. Follow Hermes's gateway instructions, then retry removing or setting up Sunday")
    }

    /// `hermes -p <profile> config set <key> <value>`; a JSON value is read by
    /// Hermes as a list or mapping.
    pub fn config_set(&self, key: &str, value: &str) -> Result<()> {
        let out = self.run_in_profile(&["config", "set", key, value], QUICK)?;
        check(out, &["-p", &self.profile, "config", "set", key])
    }

    /// The profile's own value for `key`, or None when it isn't set. Read raw,
    /// so writing a list back never stores Hermes's masked display of a secret.
    pub fn config_get(&self, key: &str) -> Result<Option<serde_json::Value>> {
        let out = self.run_in_profile(&["config", "get", key, "--json", "--raw"], QUICK)?;
        if !out.status.success() {
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            if text.contains("not set") {
                return Ok(None);
            }
            bail!(
                "hermes -p {} config get {key} failed: {}",
                self.profile,
                text.trim()
            );
        }
        let text = String::from_utf8_lossy(&out.stdout);
        let json = text
            .lines()
            .map(str::trim)
            .rfind(|line| !line.is_empty())
            .unwrap_or("null");
        Ok(Some(serde_json::from_str(json).with_context(|| {
            format!("unexpected output from hermes config get {key}: {text}")
        })?))
    }

    /// `hermes -p <profile> plugins install <name> --enable --yes-deps`: a
    /// plugin from Hermes's catalog, at the commit the catalog pins, enabled,
    /// with its Python packages (the user agreed to both in OpenAgora).
    pub fn plugin_install(&self, name: &str) -> Result<()> {
        let args = ["plugins", "install", name, "--enable", "--yes-deps"];
        let out = self.run_in_profile(&args, SLOW)?;
        check(out, &["-p", &self.profile, "plugins", "install", name])
    }

    /// `hermes -p <profile> memory setup <provider>`
    pub fn memory_setup(&self, provider: &str) -> Result<()> {
        let out = self.run_in_profile(&["memory", "setup", provider], SLOW)?;
        check(out, &["-p", &self.profile, "memory", "setup", provider])
    }

    /// `hermes -p <profile> memory off`: built-in memory only.
    pub fn memory_off(&self) -> Result<()> {
        let out = self.run_in_profile(&["memory", "off"], QUICK)?;
        check(out, &["-p", &self.profile, "memory", "off"])
    }

    /// `hermes -p <profile> -z <prompt> -s <skill> -t <toolsets>`: one turn of
    /// Sunday's, with only those toolsets. Returns its answer.
    pub fn oneshot(&self, prompt: &str, skill: &str, toolsets: &str) -> Result<String> {
        let out = self.run_in_profile(&["-z", prompt, "-s", skill, "-t", toolsets], ONESHOT)?;
        let answer = String::from_utf8_lossy(&out.stdout).to_string();
        check(out, &["-p", &self.profile, "-z", "…"])?;
        Ok(answer)
    }

    pub fn config_unset(&self, key: &str) -> Result<()> {
        let out = self.run_in_profile(&["config", "unset", key], QUICK)?;
        if out.status.success() || String::from_utf8_lossy(&out.stderr).contains("not set") {
            return Ok(());
        }
        check(out, &["-p", &self.profile, "config", "unset", key])
    }
}

fn check(out: Output, args: &[&str]) -> Result<()> {
    if out.status.success() {
        return Ok(());
    }
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    bail!("hermes {} failed: {}", args.join(" "), text.trim())
}

pub fn record(engine: &Engine, m: &Manifest) -> Result<Option<AgentProfile>> {
    state::read(&engine.paths.agent_profile_file(&m.id))
}

/// A folder Hermes would recognise as a profile.
fn is_profile(dir: &Path) -> bool {
    PROFILE_MARKERS.iter().any(|f| dir.join(f).exists())
}

/// Whether the profile's SOUL.md carries Sunday's block, i.e. OpenAgora set it up.
fn has_block(dir: &Path) -> bool {
    fs::read_to_string(dir.join("SOUL.md")).is_ok_and(|text| text.contains(BEGIN))
}

/// Puts Sunday's block at the top of `existing` (a SOUL.md), replacing an
/// earlier copy of the block and keeping everything else the user wrote.
pub fn merge_soul(existing: Option<&str>, block: &str) -> String {
    let block = block.trim();
    let text = existing.unwrap_or_default().trim_start_matches('\u{feff}');
    let outside = match (text.find(BEGIN), text.find(END)) {
        (Some(begin), Some(end)) if end > begin => {
            format!("{}{}", &text[..begin], &text[end + END.len()..])
        }
        _ => text.to_string(),
    };
    let outside = outside.trim();
    let normalized = outside.replace("\r\n", "\n");
    let outside = if normalized == HERMES_SEEDED_SOUL
        || normalized == HERMES_SEEDED_SOUL.replace('\u{2014}', "--")
    {
        outside
            .strip_prefix(HERMES_SEEDED_IDENTITY)
            .unwrap_or(outside)
    } else {
        outside
    };
    if outside.is_empty() {
        format!("{block}\n")
    } else {
        format!("{block}\n\n{outside}\n")
    }
}

/// Writes Sunday's block into the profile's SOUL.md. Returns whether it changed.
pub fn write_soul(dir: &Path) -> Result<bool> {
    let path = dir.join("SOUL.md");
    let existing = match fs::read_to_string(&path) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    };
    let merged = merge_soul(existing.as_deref(), SOUL_BLOCK);
    if existing.as_deref() == Some(merged.as_str()) {
        return Ok(false);
    }
    let tmp = dir.join(".SOUL.md.openagora-tmp");
    fs::write(&tmp, &merged).with_context(|| format!("writing {}", tmp.display()))?;
    fs::rename(&tmp, &path).with_context(|| format!("writing {}", path.display()))?;
    Ok(true)
}

/// Where Sunday stands on this computer, for `openagora sunday status`.
#[derive(Debug)]
pub struct Report {
    pub hermes: Option<PathBuf>,
    pub installed: bool,
    pub adopted: bool,
    /// Setting up needs the user's choice of fresh or cloned: there may be an
    /// earlier Hermes setup to bring along.
    pub ask_how_to_start: bool,
    pub version: Option<Version>,
    pub profile: ProfileState,
    pub dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileState {
    /// Set up by OpenAgora and still there.
    Ready,
    /// No profile yet.
    Missing,
    /// A profile with Sunday's name that OpenAgora didn't set up.
    Foreign,
    /// Hermes isn't installed, so nobody can tell.
    Unknown,
}

impl ProfileState {
    pub fn as_str(self) -> &'static str {
        match self {
            ProfileState::Ready => "ready",
            ProfileState::Missing => "missing",
            ProfileState::Foreign => "foreign",
            ProfileState::Unknown => "unknown",
        }
    }
}

impl Report {
    pub fn version_ok(&self) -> Option<bool> {
        self.version.map(|v| v >= MIN_VERSION)
    }
}

/// Reads Sunday's state. Fast once Sunday is set up; before that it asks
/// Hermes about the profile.
pub fn report(engine: &Engine, m: &Manifest) -> Result<Report> {
    let hermes = Hermes::find(engine, m);
    let installed = engine.installed(&m.id)?;
    let saved = record(engine, m)?;
    let mut version = saved
        .as_ref()
        .and_then(|r| r.version.as_deref())
        .and_then(|v| Version::from_hermes_output(&format!("Hermes Agent v{v}")));
    let (profile, dir) = match (&saved, &hermes) {
        (Some(saved), Some(_)) if has_block(Path::new(&saved.dir)) => {
            (ProfileState::Ready, Some(PathBuf::from(&saved.dir)))
        }
        (_, None) => (ProfileState::Unknown, None),
        (_, Some(hermes)) => {
            version = hermes.version();
            match hermes.profile_dir()? {
                Some(dir) if has_block(&dir) => (ProfileState::Ready, Some(dir)),
                Some(dir) => (ProfileState::Foreign, Some(dir)),
                None => (ProfileState::Missing, None),
            }
        }
    };
    Ok(Report {
        hermes: hermes.map(|h| h.exe),
        installed: installed.is_some(),
        adopted: installed.as_ref().is_some_and(|i| i.adopted),
        ask_how_to_start: !installed.is_some_and(|i| i.clean),
        version,
        profile,
        dir,
    })
}

/// Sets Sunday up: records an existing Hermes without reinstalling it, checks
/// its version, creates the profile and writes Sunday's SOUL.md block.
///
/// `start` says how a new profile begins. It may be left out only when
/// OpenAgora installed Hermes on a computer that didn't have it (so there is
/// no earlier setup to bring); any other Hermes needs the user's choice. `use_existing`
/// allows taking over a profile with Sunday's name that OpenAgora didn't create.
pub fn setup(
    engine: &Engine,
    m: &Manifest,
    start: Option<Start>,
    use_existing: bool,
) -> Result<AgentProfile> {
    let hermes = Hermes::find(engine, m).with_context(|| {
        format!(
            "{} is not installed. Install it first: openagora install {}",
            m.name, m.id
        )
    })?;
    let installed = match engine.installed(&m.id)? {
        Some(installed) => installed,
        None => engine.adopt(m)?,
    };
    let version = hermes.version();
    if let Some(v) = version
        && v < MIN_VERSION
    {
        bail!(
            "Sunday needs {} {MIN_VERSION} or later, and this computer has {v}. Update it with: openagora update {}",
            m.name,
            m.id
        );
    }
    let mut previous = record(engine, m)?;
    if previous.as_ref().is_some_and(|p| p.deletion_pending) {
        hermes.purge_identity()?;
    }
    let dir = match hermes.profile_dir()? {
        Some(dir) => {
            if !has_block(&dir) && !use_existing {
                bail!(
                    "A Hermes profile named {:?} already exists, and OpenAgora didn't create it. \
                     To use it for Sunday, run again with --use-existing.",
                    hermes.profile
                );
            }
            previous = previous.filter(|p| Path::new(&p.dir) == dir && has_block(&dir));
            dir
        }
        None => {
            // A recreated profile has none of the old profile’s connections.
            previous = None;
            let start = match start {
                Some(start) => start,
                None if installed.clean => Start::Fresh,
                None => bail!(
                    "You already use Hermes, so choose how Sunday starts: --fresh for a new setup, \
                     or --clone to bring your settings, keys, SOUL.md, skills and memory \
                     (not history, and not messaging bots unless you add --clone-channels)."
                ),
            };
            hermes.create(start)?;
            hermes
                .profile_dir()?
                .context("Hermes reported success but the profile does not exist")?
        }
    };
    write_soul(&dir)?;
    let profile = AgentProfile {
        deletion_pending: false,
        profile: hermes.profile.clone(),
        dir: dir.display().to_string(),
        version: version.map(|v| v.to_string()),
        set_up_at: state::now(),
        connected: previous.as_ref().is_some_and(|p| p.connected),
        hooks_auto_accept_before: previous.and_then(|p| p.hooks_auto_accept_before),
    };
    state::write(&engine.paths.agent_profile_file(&m.id), &profile)?;
    Ok(profile)
}

/// Skills OpenAgora ships for Sunday, as (path under the skills folder, text).
const SKILLS: &[(&str, &str)] = &[
    (
        "openagora/openagora-apps/SKILL.md",
        include_str!("../../sunday/skills/openagora-apps/SKILL.md"),
    ),
    (
        "openagora/openagora-desk/SKILL.md",
        include_str!("../../sunday/skills/openagora-desk/SKILL.md"),
    ),
    (
        "openagora/openagora-handoff/SKILL.md",
        include_str!("../../sunday/skills/openagora-handoff/SKILL.md"),
    ),
    (
        "openagora/openagora-memory/SKILL.md",
        include_str!("../../sunday/skills/openagora-memory/SKILL.md"),
    ),
];

/// The hook command Hermes runs before each of Sunday's replies.
fn hook_command(exe: &Path) -> String {
    format!("\"{}\" context --hook --client sunday", exe.display())
}

fn is_our_hook(entry: &serde_json::Value) -> bool {
    entry["command"]
        .as_str()
        .is_some_and(|c| c.ends_with("context --hook --client sunday"))
}

/// Writes OpenAgora's skills for Sunday into OpenAgora's own folder.
pub fn install_skills(paths: &crate::paths::Paths) -> Result<PathBuf> {
    let root = paths.sunday_skills();
    for (path, text) in SKILLS {
        let file = root.join(path);
        if let Some(dir) = file.parent() {
            fs::create_dir_all(dir)?;
        }
        if fs::read_to_string(&file).ok().as_deref() != Some(*text) {
            fs::write(&file, text).with_context(|| format!("writing {}", file.display()))?;
        }
    }
    Ok(root)
}

/// What `connect` configures in Sunday's profile, all through Hermes's own
/// `config set`: OpenAgora's gateway as an MCP server, the per-reply hook,
/// OpenAgora's skills folder, and `hooks_auto_accept` for this profile only.
/// Returns the names of other MCP servers the profile already had (from a
/// copied setup): they are left alone, and the user is told.
pub fn connect(engine: &Engine, m: &Manifest) -> Result<Vec<String>> {
    let mut saved = record(engine, m)?
        .context("Sunday isn't set up yet. Set it up first: openagora sunday setup")?;
    let hermes = Hermes::find(engine, m).with_context(|| format!("{} is not installed", m.name))?;
    let exe = crate::paths::install_stable_exe(&engine.paths)?;
    let skills = install_skills(&engine.paths)?;

    let server = serde_json::json!({
        "command": exe.display().to_string(),
        "args": ["mcp", "--client", "sunday"],
        // Installs can take a while, and the user may take time to confirm.
        "timeout": 1800,
        "connect_timeout": 60,
        "elicitation": {"enabled": true, "timeout": 300},
    });
    hermes.config_set("mcp_servers.openagora", &server.to_string())?;

    let mut hooks = match hermes.config_get("hooks.pre_llm_call")? {
        Some(serde_json::Value::Array(hooks)) => hooks,
        _ => Vec::new(),
    };
    hooks.retain(|h| !is_our_hook(h));
    hooks.push(serde_json::json!({"command": hook_command(&exe), "timeout": 20}));
    hermes.config_set(
        "hooks.pre_llm_call",
        &serde_json::Value::Array(hooks).to_string(),
    )?;

    if !saved.connected {
        saved.hooks_auto_accept_before = hermes
            .config_get("hooks_auto_accept")?
            .and_then(|v| v.as_bool());
    }
    hermes.config_set("hooks_auto_accept", "true")?;

    let skills_dir = skills.display().to_string();
    let mut dirs = match hermes.config_get("skills.external_dirs")? {
        Some(serde_json::Value::Array(dirs)) => dirs,
        _ => Vec::new(),
    };
    if !dirs.iter().any(|d| d.as_str() == Some(skills_dir.as_str())) {
        dirs.push(serde_json::Value::String(skills_dir));
    }
    hermes.config_set(
        "skills.external_dirs",
        &serde_json::Value::Array(dirs).to_string(),
    )?;

    saved.connected = true;
    state::write(&engine.paths.agent_profile_file(&m.id), &saved)?;
    let others = match hermes.config_get("mcp_servers")? {
        Some(serde_json::Value::Object(servers)) => servers
            .keys()
            .filter(|name| name.as_str() != "openagora")
            .cloned()
            .collect(),
        _ => Vec::new(),
    };
    Ok(others)
}

/// Undoes `connect`, leaving every other setting in the profile as it is.
pub fn disconnect(engine: &Engine, m: &Manifest) -> Result<()> {
    let Some(mut saved) = record(engine, m)? else {
        return Ok(());
    };
    let hermes = Hermes::find(engine, m).with_context(|| format!("{} is not installed", m.name))?;
    hermes.config_unset("mcp_servers.openagora")?;
    if let Some(serde_json::Value::Array(mut hooks)) = hermes.config_get("hooks.pre_llm_call")? {
        hooks.retain(|h| !is_our_hook(h));
        if hooks.is_empty() {
            hermes.config_unset("hooks.pre_llm_call")?;
        } else {
            hermes.config_set(
                "hooks.pre_llm_call",
                &serde_json::Value::Array(hooks).to_string(),
            )?;
        }
    }
    match saved.hooks_auto_accept_before {
        Some(before) => hermes.config_set("hooks_auto_accept", &before.to_string())?,
        None => hermes.config_unset("hooks_auto_accept")?,
    }
    let skills_dir = engine.paths.sunday_skills().display().to_string();
    if let Some(serde_json::Value::Array(mut dirs)) = hermes.config_get("skills.external_dirs")? {
        dirs.retain(|d| d.as_str() != Some(skills_dir.as_str()));
        hermes.config_set(
            "skills.external_dirs",
            &serde_json::Value::Array(dirs).to_string(),
        )?;
    }
    saved.connected = false;
    saved.hooks_auto_accept_before = None;
    state::write(&engine.paths.agent_profile_file(&m.id), &saved)
}

/// Called before the agent starts: its profile must exist, and its SOUL.md
/// block is brought up to date.
pub fn prepare_start(engine: &Engine, m: &Manifest) -> Result<()> {
    let saved = record(engine, m)?.with_context(|| {
        format!("Sunday isn't set up yet. Set it up first: openagora sunday setup (it uses {}'s own profile)", m.name)
    })?;
    let dir = PathBuf::from(&saved.dir);
    if !has_block(&dir) {
        bail!(
            "Sunday's profile at {} is gone. Set Sunday up again: openagora sunday setup",
            dir.display()
        );
    }
    write_soul(&dir)?;
    if saved.connected {
        // Keep what Sunday's config points at current with this OpenAgora.
        crate::paths::install_stable_exe(&engine.paths)?;
        install_skills(&engine.paths)?;
    }
    Ok(())
}

/// Re-reads the version after an update.
pub fn refresh_version(engine: &Engine, m: &Manifest) -> Result<()> {
    let (Some(hermes), Some(mut saved)) = (Hermes::find(engine, m), record(engine, m)?) else {
        return Ok(());
    };
    saved.version = hermes.version().map(|v| v.to_string());
    state::write(&engine.paths.agent_profile_file(&m.id), &saved)
}

/// Removes Sunday: deletes its profile with Hermes's own command, and nothing
/// else. A profile with Sunday's name that OpenAgora didn't set up is left alone.
/// Returns a note for the user when something was left in place.
pub fn remove_profile(engine: &Engine, m: &Manifest) -> Result<Option<String>> {
    if let Status::Running(_) = engine.status(&m.id)? {
        engine.stop(m)?;
    }
    let saved = record(engine, m)?;
    let note = match Hermes::find(engine, m) {
        Some(hermes) => match hermes.profile_dir()? {
            Some(dir) if has_block(&dir) => {
                if let Err(error) = hermes.delete() {
                    if !error.is::<IdentityPending>() { return Err(error); }
                    let mut pending = saved.clone().unwrap_or(AgentProfile {
                        deletion_pending: true, profile: hermes.profile.clone(),
                        dir: dir.display().to_string(), version: None,
                        set_up_at: state::now(), connected: false, hooks_auto_accept_before: None,
                    });
                    pending.deletion_pending = true;
                    state::write(&engine.paths.agent_profile_file(&m.id), &pending)?;
                    hermes.purge_identity()?;
                }
                None
            }
            Some(dir) => Some(format!(
                "The Hermes profile at {} wasn't created by OpenAgora, so it was kept.",
                dir.display()
            )),
            None => {
                if saved.as_ref().is_some_and(|p| p.deletion_pending) {
                    hermes.purge_identity()?;
                }
                None
            },
        },
        None if saved.as_ref().is_some_and(|p| p.deletion_pending) => {
            bail!("Hermes is missing while Sunday's identity cleanup is pending. Reinstall Hermes, then retry removing or setting up Sunday.");
        }
        None => saved.map(|s| {
            format!(
                "Hermes is no longer on this computer, so Sunday's profile folder {} was left in place.",
                s.dir
            )
        }),
    };
    state::remove(&engine.paths.agent_profile_file(&m.id))?;
    Ok(note)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_hermes_versions() {
        let read = |s: &str| Version::from_hermes_output(s);
        assert_eq!(
            read("Hermes Agent v0.21.6 (2026.9.24)\nInstall directory: /x"),
            Some(Version(0, 21, 6))
        );
        assert_eq!(
            read("Hermes Agent v0.21.6+12.gabc1234 (2026.9.24) · upstream abc"),
            Some(Version(0, 21, 6))
        );
        assert_eq!(read("Hermes Agent v1.2.3 · stable"), Some(Version(1, 2, 3)));
        assert_eq!(read("Hermes Agent vgit.abc1234 (2026.9.24)"), None);
        assert_eq!(read("something else"), None);
        assert!(Version(0, 21, 2) < MIN_VERSION && Version(0, 22, 0) > MIN_VERSION);
    }

    #[test]
    fn soul_block_is_marked_and_static() {
        let block = SOUL_BLOCK.trim();
        assert!(block.starts_with(BEGIN) && block.ends_with(END));
        assert_eq!(block.matches(BEGIN).count(), 1);
        assert_eq!(block.matches(END).count(), 1);
        assert!(block.contains("Your name is Sunday"));
        assert!(block.contains("coding agents"));
        // Credit where it's due: Sunday is Hermes Agent, by Nous Research.
        assert!(block.contains("Hermes Agent") && block.contains("give Nous Research the credit"));
    }

    #[test]
    fn soul_merge_keeps_the_users_own_text() {
        // A new file is just the block.
        let fresh = merge_soul(None, SOUL_BLOCK);
        assert_eq!(fresh, format!("{}\n", SOUL_BLOCK.trim()));

        // Hermes's seeded default loses only its "You are Hermes Agent" opening.
        let seeded = merge_soul(Some(HERMES_SEEDED_SOUL), SOUL_BLOCK);
        assert!(seeded.starts_with(BEGIN));
        assert!(!seeded.contains("You are Hermes Agent"));
        assert!(seeded.contains("Be direct: match the length"));

        // The user's own persona is kept word for word, under the block.
        let mine = "You are Jarvis. Speak like a butler.\n\nAlways use metric units.";
        let merged = merge_soul(Some(mine), SOUL_BLOCK);
        assert!(merged.starts_with(BEGIN));
        assert!(merged.ends_with(&format!("{mine}\n")));

        // Updating the block replaces it in place and is idempotent.
        let old = format!("{BEGIN}\nold text\n{END}\n\n{mine}\n");
        let updated = merge_soul(Some(&old), SOUL_BLOCK);
        assert_eq!(updated, merged);
        assert_eq!(merge_soul(Some(&updated), SOUL_BLOCK), updated);

        // Edits the user made around a block that wandered are kept too.
        let around = format!("Before.\n{BEGIN}\nold\n{END}\nAfter.");
        let merged = merge_soul(Some(&around), SOUL_BLOCK);
        assert!(merged.contains("Before.") && merged.contains("After."));
        assert!(!merged.contains("\nold\n"));
    }

    #[test]
    fn write_soul_only_rewrites_when_needed() {
        let dir = tempfile::tempdir().unwrap();
        assert!(write_soul(dir.path()).unwrap());
        assert!(!write_soul(dir.path()).unwrap());
        let soul = fs::read_to_string(dir.path().join("SOUL.md")).unwrap();
        assert!(soul.contains("Your name is Sunday"));
        assert!(has_block(dir.path()));
    }
}
