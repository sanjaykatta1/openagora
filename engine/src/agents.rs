//! Coding agents OpenAgora didn't install (Claude Code, Codex, OpenCode, or
//! any other MCP agent the user names), found on this computer and set up to
//! use OpenAgora's MCP server through their own commands, only after the user
//! agrees. Each is a destination for what Sunday hands over: it reads what
//! Sunday sent it with its own `context_inbox` and `context_get` tools.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::apps::{resolve, search_path};
use crate::manifest::valid_id;
use crate::paths::{self, Paths};
use crate::process::output_within;
use crate::state;

/// The name OpenAgora's server gets in each agent's MCP config.
pub const SERVER_NAME: &str = "openagora";

/// How a coding agent is found and set up.
#[derive(Debug)]
pub struct Descriptor {
    /// Also the MCP client id it connects with (`openagora mcp --client <id>`).
    pub id: &'static str,
    pub name: &'static str,
    /// Its command-line tool.
    pub command: &'static str,
    /// Where its installers put the command, under the home folder, when that
    /// isn't on PATH yet.
    pub install_dirs: &'static [&'static str],
    pub homepage: &'static str,
}

pub const AGENTS: &[Descriptor] = &[
    Descriptor {
        id: "claude-code",
        name: "Claude Code",
        command: "claude",
        install_dirs: &[".claude/local", ".npm-global/bin"],
        homepage: "https://code.claude.com/docs",
    },
    Descriptor {
        id: "codex",
        name: "Codex",
        command: "codex",
        install_dirs: &[".npm-global/bin"],
        homepage: "https://developers.openai.com/codex",
    },
    Descriptor {
        id: "opencode",
        name: "OpenCode",
        command: "opencode",
        install_dirs: &[".opencode/bin", ".bun/bin", ".npm-global/bin"],
        homepage: "https://opencode.ai/docs",
    },
];

pub fn descriptor(id: &str) -> Option<&'static Descriptor> {
    AGENTS.iter().find(|d| d.id == id)
}

/// Whether `id` names a coding agent: a built-in one, or one the user added.
pub fn is_agent(paths: &Paths, id: &str) -> bool {
    descriptor(id).is_some() || custom(paths).unwrap_or_default().iter().any(|c| c.id == id)
}

/// A coding agent as found on this computer.
#[derive(Debug, Clone)]
pub struct Found {
    pub descriptor: &'static Descriptor,
    /// Its command, if it is installed.
    pub exe: Option<PathBuf>,
    /// The first line of `<command> --version`.
    pub version: Option<String>,
}

impl Found {
    /// The major version (OpenCode 2 writes a project's config unless told
    /// `--global`, and has a shared server).
    pub(crate) fn major(&self) -> Option<u32> {
        let version = self.version.as_deref()?;
        version
            .split(|c: char| !c.is_ascii_digit() && c != '.')
            .find(|part| part.contains('.'))?
            .split('.')
            .next()?
            .parse()
            .ok()
    }
}

fn run(exe: &Path, args: &[String]) -> Result<Output> {
    let mut cmd = Command::new(exe);
    cmd.args(args).env("PATH", search_path());
    output_within(&mut cmd, Duration::from_secs(60))
        .with_context(|| format!("running {}", exe.display()))
}

/// The agent's command on this computer: on PATH, in `~/.local/bin`, or where
/// its own installer puts it.
fn locate(d: &Descriptor) -> Option<PathBuf> {
    if let Some(exe) = resolve(d.command) {
        return Some(exe);
    }
    let home = paths::home().ok()?;
    let names: Vec<String> = if cfg!(windows) {
        [".exe", ".cmd", ""]
            .iter()
            .map(|ext| format!("{}{ext}", d.command))
            .collect()
    } else {
        vec![d.command.to_string()]
    };
    let home = &home;
    d.install_dirs
        .iter()
        .flat_map(|dir| names.iter().map(move |name| home.join(dir).join(name)))
        .find(|candidate| candidate.is_file())
}

pub fn find(d: &'static Descriptor) -> Found {
    let exe = locate(d);
    let version = exe.as_deref().and_then(|exe| {
        let out = run(exe, &["--version".to_string()]).ok()?;
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .next()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
    });
    Found {
        descriptor: d,
        exe,
        version,
    }
}

/// The command each MCP client runs to reach OpenAgora.
pub fn server_command(engine: &Path, client: &str) -> Vec<String> {
    vec![
        engine.display().to_string(),
        "mcp".into(),
        "--client".into(),
        client.into(),
    ]
}

/// The agent's own command that adds OpenAgora's MCP server to its user-wide
/// config, pointing at `engine` (the stable copy of OpenAgora's engine).
pub fn add_args(found: &Found, engine: &Path) -> Vec<String> {
    let mut args: Vec<String> = match found.descriptor.id {
        // `claude mcp add --scope user --transport stdio <name> -- <command…>`
        "claude-code" => [
            "mcp",
            "add",
            "--scope",
            "user",
            "--transport",
            "stdio",
            SERVER_NAME,
        ]
        .map(String::from)
        .to_vec(),
        // `codex mcp add <name> -- <command…>`
        "codex" => ["mcp", "add", SERVER_NAME].map(String::from).to_vec(),
        // `opencode mcp add <name> [--global] -- <command…>`: OpenCode 1 always
        // writes its global config, 2 needs `--global`.
        _ => {
            let mut args: Vec<String> = ["mcp", "add", SERVER_NAME].map(String::from).to_vec();
            if found.major().is_some_and(|major| major >= 2) {
                args.push("--global".into());
            }
            args
        }
    };
    args.push("--".into());
    args.extend(server_command(engine, found.descriptor.id));
    args
}

/// The agent's own command that removes OpenAgora's MCP server, where it has
/// one. OpenCode has none.
pub fn remove_args(found: &Found) -> Option<Vec<String>> {
    match found.descriptor.id {
        "claude-code" => Some(
            ["mcp", "remove", SERVER_NAME, "--scope", "user"]
                .map(String::from)
                .to_vec(),
        ),
        "codex" => Some(["mcp", "remove", SERVER_NAME].map(String::from).to_vec()),
        _ => None,
    }
}

/// How to remove OpenAgora's server from an agent that has no command for it.
pub fn remove_by_hand(found: &Found) -> String {
    format!(
        "{} has no command to remove an MCP server: remove \"{SERVER_NAME}\" from the \"mcp\" section of its opencode.json (in ~/.config/opencode/) yourself, or set \"enabled\": false there.",
        found.descriptor.name
    )
}

/// A command line, for people: the agent's command and its arguments, quoted
/// where needed.
pub fn display(command: &str, args: &[String]) -> String {
    std::iter::once(command.to_string())
        .chain(args.iter().map(|a| {
            if a.is_empty() || a.contains(|c: char| c.is_whitespace() || "'\"$`\\".contains(c)) {
                format!("'{}'", a.replace('\'', "'\\''"))
            } else {
                a.clone()
            }
        }))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whether the agent's config has OpenAgora's server, asked of the agent's own
/// command (`mcp get`, or `mcp list` for OpenCode).
pub fn is_set_up(found: &Found) -> Result<bool> {
    let Some(exe) = &found.exe else {
        return Ok(false);
    };
    let args = |list: &[&str]| list.iter().map(|a| a.to_string()).collect::<Vec<_>>();
    Ok(match found.descriptor.id {
        "claude-code" => {
            let out = run(exe, &args(&["mcp", "get", SERVER_NAME]))?;
            let said = format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            out.status.success() && !said.contains("No MCP server found")
        }
        "codex" => run(exe, &args(&["mcp", "get", SERVER_NAME, "--json"]))?
            .status
            .success(),
        _ => {
            let out = run(exe, &args(&["mcp", "list"]))?;
            out.status.success()
                && String::from_utf8_lossy(&out.stdout).lines().any(|line| {
                    line.split_whitespace().any(|word| {
                        word.trim_matches(|c: char| !c.is_alphanumeric()) == SERVER_NAME
                    })
                })
        }
    })
}

fn run_checked(found: &Found, args: &[String]) -> Result<String> {
    let Some(exe) = &found.exe else {
        bail!("{} isn't installed on this computer", found.descriptor.name);
    };
    let shown = display(found.descriptor.command, args);
    let out = run(exe, args)?;
    if !out.status.success() {
        let why = String::from_utf8_lossy(&out.stderr);
        let why = why.trim();
        let why = if why.is_empty() {
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        } else {
            why.to_string()
        };
        bail!("{shown} failed: {why}");
    }
    Ok(shown)
}

/// Runs the agent's own command to add OpenAgora's server. Returns the
/// command it ran.
pub fn set_up(found: &Found, engine: &Path) -> Result<String> {
    run_checked(found, &add_args(found, engine))
}

/// Runs the agent's own command to remove OpenAgora's server.
pub fn remove(found: &Found) -> Result<String> {
    match remove_args(found) {
        Some(args) => run_checked(found, &args),
        None => bail!("{}", remove_by_hand(found)),
    }
}

/// Another MCP agent the user set up by hand (`openagora agents add`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Custom {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Customs {
    #[serde(default)]
    agent: Vec<Custom>,
}

fn custom_file(paths: &Paths) -> PathBuf {
    paths.root.join("state").join("agents.toml")
}

pub fn custom(paths: &Paths) -> Result<Vec<Custom>> {
    Ok(state::read::<Customs>(&custom_file(paths))?
        .unwrap_or_default()
        .agent)
}

/// Adds (or renames) an agent the user set up by hand.
pub fn add_custom(paths: &Paths, id: &str, name: &str) -> Result<()> {
    if !valid_id(id) {
        bail!("{id:?} must be lowercase letters, digits and dashes, starting with a letter");
    }
    let built_in = [
        "sunday",
        "user",
        crate::destinations::CLIPBOARD,
        crate::destinations::FOLDER,
    ];
    if descriptor(id).is_some() || built_in.contains(&id) {
        bail!("{id:?} is already taken; OpenAgora knows that one");
    }
    if name.trim().is_empty() {
        bail!("give the agent a name");
    }
    let mut all = state::read::<Customs>(&custom_file(paths))?.unwrap_or_default();
    all.agent.retain(|c| c.id != id);
    all.agent.push(Custom {
        id: id.into(),
        name: name.trim().into(),
    });
    state::write(&custom_file(paths), &all)
}

pub fn forget_custom(paths: &Paths, id: &str) -> Result<bool> {
    let mut all = state::read::<Customs>(&custom_file(paths))?.unwrap_or_default();
    let before = all.agent.len();
    all.agent.retain(|c| c.id != id);
    if all.agent.len() == before {
        return Ok(false);
    }
    state::write(&custom_file(paths), &all)?;
    Ok(true)
}

/// What OpenAgora knows of an agent without asking it: when the user set it
/// up through OpenAgora, and when it last connected to the gateway.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Seen {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set_up_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connected_at: Option<u64>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Seens {
    #[serde(default)]
    agent: Vec<Seen>,
}

static SEEN_WRITE: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn seen_file(paths: &Paths) -> PathBuf {
    paths.root.join("state").join("agents-seen.toml")
}

pub fn seen(paths: &Paths) -> Result<Vec<Seen>> {
    Ok(state::read::<Seens>(&seen_file(paths))?
        .unwrap_or_default()
        .agent)
}

fn change_seen(paths: &Paths, id: &str, edit: impl FnOnce(&mut Seen)) -> Result<()> {
    let _guard = SEEN_WRITE.lock().unwrap();
    let mut all = state::read::<Seens>(&seen_file(paths))?.unwrap_or_default();
    let index = match all.agent.iter().position(|s| s.id == id) {
        Some(i) => i,
        None => {
            all.agent.push(Seen {
                id: id.into(),
                ..Seen::default()
            });
            all.agent.len() - 1
        }
    };
    edit(&mut all.agent[index]);
    state::write(&seen_file(paths), &all)
}

pub fn mark_set_up(paths: &Paths, id: &str) -> Result<()> {
    change_seen(paths, id, |s| s.set_up_at = Some(state::now()))
}

pub fn mark_removed(paths: &Paths, id: &str) -> Result<()> {
    change_seen(paths, id, |s| {
        s.set_up_at = None;
        s.connected_at = None;
    })
}

/// The agent connected to the gateway: it's set up, whoever did it.
pub fn mark_connected(paths: &Paths, id: &str) -> Result<()> {
    change_seen(paths, id, |s| s.connected_at = Some(state::now()))
}

/// Claude Code's prompt hook, so it hears about what Sunday sent it with the
/// user's next prompt. It comes as a Claude Code plugin in a marketplace of
/// OpenAgora's own (a folder under OpenAgora's data folder), installed with
/// Claude Code's own commands, never by editing its settings.
pub mod claude_hook {
    use super::*;

    pub const MARKETPLACE: &str = "openagora";
    pub const PLUGIN: &str = "openagora";

    pub fn marketplace_dir(paths: &Paths) -> PathBuf {
        paths.root.join("claude-code").join("marketplace")
    }

    /// Writes the marketplace and its one plugin, whose hook runs `engine`.
    pub fn write(paths: &Paths, engine: &Path) -> Result<PathBuf> {
        let root = marketplace_dir(paths);
        let plugin = root.join("plugins").join(PLUGIN);
        let files = [
            (
                root.join(".claude-plugin/marketplace.json"),
                serde_json::json!({
                    "name": MARKETPLACE,
                    "description": "OpenAgora's plugins for Claude Code",
                    "owner": {"name": "OpenAgora"},
                    "plugins": [{
                        "name": PLUGIN,
                        "source": format!("./plugins/{PLUGIN}"),
                        "description": "Tells Claude Code what Sunday sent it, with your next prompt",
                    }],
                }),
            ),
            (
                plugin.join(".claude-plugin/plugin.json"),
                serde_json::json!({
                    "name": PLUGIN,
                    "description": "Tells Claude Code what Sunday, OpenAgora's personal agent, sent it, with your next prompt",
                    "homepage": "https://github.com/sanjaykatta1/openagora",
                }),
            ),
            (
                plugin.join("hooks/hooks.json"),
                serde_json::json!({
                    "hooks": {"UserPromptSubmit": [{"hooks": [{
                        "type": "command",
                        "command": engine.display().to_string(),
                        "args": ["context", "--hook", "--client", "claude-code", "--format", "claude-code"],
                        "timeout": 10,
                    }]}]},
                }),
            ),
        ];
        for (file, value) in files {
            if let Some(dir) = file.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(&file, serde_json::to_vec_pretty(&value)?)
                .with_context(|| format!("writing {}", file.display()))?;
        }
        Ok(root)
    }

    /// Claude Code's own commands that add the marketplace and install the
    /// plugin from it.
    pub fn install_args(paths: &Paths) -> Vec<Vec<String>> {
        let root = marketplace_dir(paths).display().to_string();
        vec![
            [
                "plugin",
                "marketplace",
                "add",
                root.as_str(),
                "--scope",
                "user",
            ]
            .map(String::from)
            .to_vec(),
            [
                "plugin",
                "install",
                &format!("{PLUGIN}@{MARKETPLACE}"),
                "--scope",
                "user",
            ]
            .map(String::from)
            .to_vec(),
        ]
    }

    /// The commands that take them out again.
    pub fn uninstall_args() -> Vec<Vec<String>> {
        vec![
            [
                "plugin",
                "uninstall",
                &format!("{PLUGIN}@{MARKETPLACE}"),
                "--scope",
                "user",
            ]
            .map(String::from)
            .to_vec(),
            [
                "plugin",
                "marketplace",
                "remove",
                MARKETPLACE,
                "--scope",
                "user",
            ]
            .map(String::from)
            .to_vec(),
        ]
    }

    /// Installs the hook. Returns the commands it ran.
    pub fn install(found: &Found, paths: &Paths, engine: &Path) -> Result<Vec<String>> {
        write(paths, engine)?;
        install_args(paths)
            .iter()
            .map(|args| run_checked(found, args))
            .collect()
    }

    /// Takes the hook out; what fails is reported, the rest still runs.
    pub fn uninstall(found: &Found) -> Vec<Result<String>> {
        uninstall_args()
            .iter()
            .map(|args| run_checked(found, args))
            .collect()
    }
}

/// A coding agent's name, for people.
pub fn name(paths: &Paths, id: &str) -> Option<String> {
    descriptor(id).map(|d| d.name.to_string()).or_else(|| {
        custom(paths)
            .ok()?
            .into_iter()
            .find(|c| c.id == id)
            .map(|c| c.name)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(id: &str, version: &str) -> Found {
        Found {
            descriptor: descriptor(id).unwrap(),
            exe: Some(PathBuf::from("/bin/true")),
            version: Some(version.into()),
        }
    }

    #[test]
    fn uses_each_agents_own_command() {
        let engine = Path::new("/data/bin/openagora");
        assert_eq!(
            add_args(&found("claude-code", "2.1.300 (Claude Code)"), engine).join(" "),
            "mcp add --scope user --transport stdio openagora -- /data/bin/openagora mcp --client claude-code"
        );
        assert_eq!(
            add_args(&found("codex", "codex-cli 0.130.0"), engine).join(" "),
            "mcp add openagora -- /data/bin/openagora mcp --client codex"
        );
        assert_eq!(
            add_args(&found("opencode", "1.19.3"), engine).join(" "),
            "mcp add openagora -- /data/bin/openagora mcp --client opencode"
        );
        assert_eq!(
            add_args(&found("opencode", "opencode 2.0.5"), engine).join(" "),
            "mcp add openagora --global -- /data/bin/openagora mcp --client opencode"
        );
        assert!(remove_args(&found("opencode", "1.19.3")).is_none());
        assert_eq!(
            remove_args(&found("claude-code", "2")).unwrap().join(" "),
            "mcp remove openagora --scope user"
        );
        assert_eq!(
            display(
                "claude",
                &["mcp".into(), "--".into(), "/My Data/bin/openagora".into()]
            ),
            "claude mcp -- '/My Data/bin/openagora'"
        );
    }
}
