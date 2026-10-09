//! The `app.toml` listing format (schema `openagora/v1`).
//!
//! A listing says how to install an app, how to run it, how to show it, and how
//! the built-in agent may connect to it. Unknown fields are rejected so a typo
//! never reads as a working setting.

use std::collections::BTreeSet;
use std::fmt;

use serde::Deserialize;

pub const SCHEMA: &str = "openagora/v1";
/// Placeholder the engine replaces with the port it allocated for the app.
pub const PORT_PLACEHOLDER: &str = "{port}";

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
    pub run: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Install {
    /// Executable the install puts on PATH; the engine checks for it afterwards.
    pub provides: String,
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
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Run {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// `"auto"` lets the engine pick a free port; a number pins one.
    pub port: Port,
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
            let steps = self
                .install
                .steps
                .iter()
                .filter(|s| s.os.contains(os))
                .count();
            if steps == 0 {
                out.push(format!("no install step for listed platform {os}"));
            }
        }
        for (i, step) in self.install.steps.iter().enumerate() {
            if step.run.trim().is_empty() {
                out.push(format!("install step {} has an empty run", i + 1));
            }
            if let Some(os) = step.os.iter().find(|os| !platforms.contains(os)) {
                out.push(format!(
                    "install step {} targets {os}, which is not in platforms",
                    i + 1
                ));
            }
            if step.shell == Shell::Powershell && step.os.iter().any(|os| *os != Os::Windows) {
                out.push(format!(
                    "install step {} uses powershell outside windows",
                    i + 1
                ));
            }
        }
        if let Port::Named(name) = &self.run.port
            && name != "auto"
        {
            out.push(format!(
                "run.port must be a number or \"auto\", got {name:?}"
            ));
        }
        if !self.run.health.tcp {
            out.push("run.health needs a check (tcp = true)".into());
        }
        match (self.ui.kind, &self.ui.url) {
            (UiKind::Web, None) => out.push("ui.url is required for kind = \"web\"".into()),
            (UiKind::Web, Some(url)) if !is_loopback_http(url) => {
                out.push("ui.url must point at 127.0.0.1 or localhost".into())
            }
            _ => {}
        }
        if let Some(agent) = &self.agent {
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
