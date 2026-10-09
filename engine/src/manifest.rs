//! The `app.toml` listing format (schema `openagora/v1`).
//!
//! A listing says how to install an app, how to run it, how to show it, and how
//! the built-in agent may connect to it. Unknown fields are rejected so a typo
//! never reads as a working setting.

use std::collections::BTreeSet;
use std::fmt;

use serde::Deserialize;

pub const SCHEMA: &str = "openagora/v1";

/// Placeholders the engine fills in, and where each may appear.
/// `{app_dir}`: the app's own folder under OpenAgora's data directory.
/// `{download}`: the file a step's `download` fetched.
/// `{port}`: the port the engine allocated.
const STEP_PLACEHOLDERS: &[&str] = &["app_dir", "download"];
const RUN_PLACEHOLDERS: &[&str] = &["app_dir", "port"];
const URL_PLACEHOLDERS: &[&str] = &["port"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Os {
    Macos,
    Linux,
    Windows,
}

impl Os {
    pub fn current() -> Option<Os> {
        match std::env::consts::OS {
            "macos" => Some(Os::Macos),
            "linux" => Some(Os::Linux),
            "windows" => Some(Os::Windows),
            _ => None,
        }
    }
}

impl fmt::Display for Os {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Os::Macos => "macos",
            Os::Linux => "linux",
            Os::Windows => "windows",
        })
    }
}

/// A value that is either the same everywhere or set per OS.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum PerOs<T> {
    All(T),
    ByOs(ByOs<T>),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ByOs<T> {
    pub macos: Option<T>,
    pub linux: Option<T>,
    pub windows: Option<T>,
}

impl<T> PerOs<T> {
    pub fn for_os(&self, os: Os) -> Option<&T> {
        match self {
            PerOs::All(value) => Some(value),
            PerOs::ByOs(by) => match os {
                Os::Macos => by.macos.as_ref(),
                Os::Linux => by.linux.as_ref(),
                Os::Windows => by.windows.as_ref(),
            },
        }
    }

    fn values(&self) -> Vec<&T> {
        match self {
            PerOs::All(value) => vec![value],
            PerOs::ByOs(by) => [&by.macos, &by.linux, &by.windows]
                .into_iter()
                .flatten()
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    X64,
    Arm64,
}

impl Arch {
    pub fn current() -> Option<Arch> {
        match std::env::consts::ARCH {
            "x86_64" => Some(Arch::X64),
            "aarch64" => Some(Arch::Arm64),
            _ => None,
        }
    }
}

/// A release asset name, the same for every CPU or set per CPU.
/// `*` matches the version, e.g. `Handy_*_amd64.AppImage`.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Asset {
    Any(String),
    ByArch {
        x64: Option<String>,
        arm64: Option<String>,
    },
}

impl Asset {
    pub fn for_arch(&self, arch: Arch) -> Option<&str> {
        match self {
            Asset::Any(name) => Some(name),
            Asset::ByArch { x64, arm64 } => match arch {
                Arch::X64 => x64.as_deref(),
                Arch::Arm64 => arm64.as_deref(),
            },
        }
    }

    fn patterns(&self) -> Vec<&str> {
        match self {
            Asset::Any(name) => vec![name],
            Asset::ByArch { x64, arm64 } => [x64, arm64]
                .into_iter()
                .flatten()
                .map(String::as_str)
                .collect(),
        }
    }
}

/// Fetch a file from the latest GitHub release before running the step.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Download {
    /// `owner/repo`; must be the app's own repository.
    pub github: String,
    pub asset: Asset,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// An ordinary app from the store.
    App,
    /// The built-in agent. Installed with OpenAgora and not removable.
    Agent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Shell {
    Sh,
    Powershell,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub os: Vec<Os>,
    pub shell: Shell,
    pub download: Option<Download>,
    pub run: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Install {
    /// What the install leaves behind, checked afterwards: an executable name on
    /// PATH, or a path under `{app_dir}`.
    pub provides: PerOs<String>,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Command {
    pub run: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Health {
    /// Healthy once the allocated port accepts TCP connections.
    #[serde(default)]
    pub tcp: bool,
    /// Healthy while the started process is alive (apps without a port).
    #[serde(default)]
    pub process: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Run {
    pub command: PerOs<String>,
    #[serde(default)]
    pub args: Vec<String>,
    /// `"auto"` lets the engine pick a free port; a number pins one. Apps
    /// without a web UI have none.
    pub port: Option<Port>,
    pub health: Health,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum Port {
    Fixed(u16),
    Named(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UiKind {
    /// The app's own web UI, shown in a tab.
    Web,
    /// A terminal app, shown in a terminal tab.
    Terminal,
    /// A native app that opens its own window.
    Window,
    /// No window of its own in OpenAgora: a menu-bar, tray or shortcut utility.
    Background,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ui {
    pub kind: UiKind,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum McpAuth {
    None,
    Oauth,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mcp {
    pub url: String,
    pub auth: McpAuth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Grant {
    Granted,
    Ask,
    Denied,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Permission {
    pub id: String,
    pub description: String,
    pub default: Grant,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Agent {
    pub mcp: Mcp,
    #[serde(default)]
    pub permissions: Vec<Permission>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: String,
    pub id: String,
    pub name: String,
    pub summary: String,
    pub category: String,
    pub homepage: String,
    pub source: String,
    pub license: String,
    pub platforms: Vec<Os>,
    #[serde(default = "default_role")]
    pub role: Role,
    pub install: Install,
    pub update: Option<Command>,
    pub uninstall: Option<Command>,
    pub run: Run,
    pub ui: Ui,
    /// How the built-in agent connects to this app. Absent: the agent cannot see it.
    pub agent: Option<Agent>,
}

fn default_role() -> Role {
    Role::App
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Manifest, String> {
        let manifest: Manifest = toml::from_str(text).map_err(|e| e.to_string())?;
        let problems = manifest.problems();
        if problems.is_empty() {
            Ok(manifest)
        } else {
            Err(problems.join("\n"))
        }
    }

    /// Install steps that apply on `os`, in order.
    pub fn steps_for(&self, os: Os) -> impl Iterator<Item = &Step> {
        self.install
            .steps
            .iter()
            .filter(move |s| s.os.contains(&os))
    }

    /// Every rule the type system cannot express. Empty means valid.
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.schema != SCHEMA {
            out.push(format!("schema must be {SCHEMA:?}, got {:?}", self.schema));
        }
        if !valid_id(&self.id) {
            out.push(format!(
                "id {:?} must be lowercase letters, digits and dashes, starting with a letter",
                self.id
            ));
        }
        for (field, value) in [("name", &self.name), ("summary", &self.summary)] {
            if value.trim().is_empty() {
                out.push(format!("{field} is empty"));
            }
        }
        for (field, value) in [("homepage", &self.homepage), ("source", &self.source)] {
            if !value.starts_with("https://") {
                out.push(format!("{field} must be an https:// URL"));
            }
        }
        if self.platforms.is_empty() {
            out.push("platforms is empty".into());
        }
        let platforms: BTreeSet<Os> = self.platforms.iter().copied().collect();
        for os in &platforms {
            if self.steps_for(*os).next().is_none() {
                out.push(format!("no install step for listed platform {os}"));
            }
            if self.install.provides.for_os(*os).is_none() {
                out.push(format!("install.provides has no value for {os}"));
            }
            if self.run.command.for_os(*os).is_none() {
                out.push(format!("run.command has no value for {os}"));
            }
        }
        for (i, step) in self.install.steps.iter().enumerate() {
            let n = i + 1;
            if step.run.trim().is_empty() {
                out.push(format!("install step {n} has an empty run"));
            }
            if let Some(os) = step.os.iter().find(|os| !platforms.contains(os)) {
                out.push(format!(
                    "install step {n} targets {os}, which is not in platforms"
                ));
            }
            if step.shell == Shell::Powershell && step.os.iter().any(|os| *os != Os::Windows) {
                out.push(format!("install step {n} uses powershell outside windows"));
            }
            check_placeholders(
                &mut out,
                &format!("install step {n}"),
                &step.run,
                STEP_PLACEHOLDERS,
            );
            match &step.download {
                Some(download) => {
                    if !step.run.contains("{download}") {
                        out.push(format!(
                            "install step {n} downloads a file but never uses {{download}}"
                        ));
                    }
                    if !self
                        .source
                        .ends_with(&format!("github.com/{}", download.github))
                    {
                        out.push(format!(
                            "install step {n} downloads from {:?}, which is not this app's source",
                            download.github
                        ));
                    }
                    let patterns = download.asset.patterns();
                    if patterns.is_empty()
                        || patterns.iter().any(|p| p.is_empty() || p.contains('/'))
                    {
                        out.push(format!("install step {n} has an invalid asset name"));
                    }
                }
                None if step.run.contains("{download}") => {
                    out.push(format!(
                        "install step {n} uses {{download}} without a download"
                    ));
                }
                None => {}
            }
        }
        for provides in self.install.provides.values() {
            check_placeholders(&mut out, "install.provides", provides, &["app_dir"]);
        }
        for command in self.run.command.values() {
            check_placeholders(&mut out, "run.command", command, RUN_PLACEHOLDERS);
        }
        for arg in &self.run.args {
            check_placeholders(&mut out, "run.args", arg, RUN_PLACEHOLDERS);
        }
        match &self.run.port {
            Some(Port::Named(name)) if name != "auto" => out.push(format!(
                "run.port must be a number or \"auto\", got {name:?}"
            )),
            None if self.run.args.iter().any(|a| a.contains("{port}")) => {
                out.push("run.args uses {port} but run.port is not set".into())
            }
            _ => {}
        }
        let health = &self.run.health;
        match (health.tcp, health.process) {
            (false, false) => out.push("run.health needs a check (tcp or process)".into()),
            (true, true) => out.push("run.health: choose tcp or process, not both".into()),
            (true, false) if self.run.port.is_none() => {
                out.push("run.health.tcp needs run.port".into())
            }
            _ => {}
        }
        match (self.ui.kind, &self.ui.url) {
            (UiKind::Web, None) => out.push("ui.url is required for kind = \"web\"".into()),
            (UiKind::Web, Some(url)) => {
                if !is_loopback_http(url) {
                    out.push("ui.url must point at 127.0.0.1 or localhost".into());
                }
                if self.run.port.is_none() {
                    out.push("kind = \"web\" needs run.port".into());
                }
                check_placeholders(&mut out, "ui.url", url, URL_PLACEHOLDERS);
            }
            (_, Some(_)) => out.push("ui.url is only for kind = \"web\"".into()),
            (_, None) => {}
        }
        if let Some(agent) = &self.agent {
            check_placeholders(&mut out, "agent.mcp.url", &agent.mcp.url, URL_PLACEHOLDERS);
            if !is_loopback_http(&agent.mcp.url) {
                // Remote MCP endpoints are allowed later, with their own review rules.
                out.push("agent.mcp.url must point at 127.0.0.1 or localhost".into());
            }
            let mut seen = BTreeSet::new();
            for p in &agent.permissions {
                if !seen.insert(p.id.as_str()) {
                    out.push(format!("agent permission {:?} is listed twice", p.id));
                }
            }
        }
        if self.role == Role::Agent && self.uninstall.is_some() {
            out.push("the agent cannot declare uninstall; it ships with OpenAgora".into());
        }
        out
    }
}

/// Reports `{name}` placeholders that are not in `allowed`. Braces that are not
/// a plain lowercase name (shell or PowerShell syntax) are left alone.
fn check_placeholders(out: &mut Vec<String>, field: &str, text: &str, allowed: &[&str]) {
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        rest = &rest[start + 1..];
        let Some(end) = rest.find('}') else { break };
        let name = &rest[..end];
        let plain = !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase() || c == '_');
        if plain && !allowed.contains(&name) {
            out.push(format!("{field}: unknown placeholder {{{name}}}"));
        }
    }
}

fn valid_id(id: &str) -> bool {
    let mut chars = id.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !id.ends_with('-')
}

fn is_loopback_http(url: &str) -> bool {
    ["http://127.0.0.1:", "http://localhost:"]
        .iter()
        .any(|prefix| url.starts_with(prefix))
}
