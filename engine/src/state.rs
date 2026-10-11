//! What OpenAgora remembers about installed and running apps.

use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Installed {
    pub id: String,
    /// Seconds since the Unix epoch.
    pub installed_at: u64,
    /// Release assets downloaded during the install.
    #[serde(default)]
    pub assets: Vec<String>,
    /// Already on this computer, so OpenAgora recorded it without running its
    /// installer.
    #[serde(default)]
    pub adopted: bool,
    /// OpenAgora ran the installer on a computer that didn't have the app, so
    /// there is no earlier setup of it to bring along. False for records made
    /// before OpenAgora tracked this.
    #[serde(default)]
    pub clean: bool,
}

/// The personal agent's own profile, once OpenAgora has set it up.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProfile {
    pub profile: String,
    /// The profile's folder, as the agent reported it.
    pub dir: String,
    /// The agent version seen at setup or update, if it could be read.
    pub version: Option<String>,
    pub set_up_at: u64,
    /// The profile uses OpenAgora's MCP gateway, per-reply hook and skills.
    #[serde(default)]
    pub connected: bool,
    /// `hooks_auto_accept` as it was before OpenAgora turned it on, so
    /// disconnecting can put it back.
    #[serde(default)]
    pub hooks_auto_accept_before: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Running {
    pub pid: u32,
    pub port: Option<u16>,
    pub url: Option<String>,
    pub started_at: u64,
    /// The agent's profile it was started under (personal agent only).
    #[serde(default)]
    pub profile: Option<String>,
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn read<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    match fs::read_to_string(path) {
        Ok(text) => toml::from_str(&text)
            .map(Some)
            .with_context(|| format!("reading {}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}

/// Writes atomically, so a crash never leaves a half-written state file.
pub fn write<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let dir = path.parent().context("state path has no parent")?;
    fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, toml::to_string(value)?)
        .with_context(|| format!("writing {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("writing {}", path.display()))
}

pub fn remove(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            Err(e).with_context(|| format!("removing {}", path.display()))
        }
        _ => Ok(()),
    }
}
