//! Where OpenAgora keeps its data on each OS.
//!
//! ```text
//! <root>/apps/<id>/      the app's own folder ({app_dir})
//! <root>/state/<id>.toml what was installed
//! <root>/run/<id>.toml   the running process, while it runs
//! <root>/logs/<id>.log   the app's output
//! <root>/tmp/            downloads in progress
//! ```

use std::env;
use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::manifest::Os;

#[derive(Debug, Clone)]
pub struct Paths {
    pub root: PathBuf,
}

impl Paths {
    /// `OPENAGORA_HOME` overrides the OS default (used by tests and portable setups).
    pub fn from_env() -> Result<Paths> {
        if let Some(root) = env::var_os("OPENAGORA_HOME").filter(|v| !v.is_empty()) {
            return Ok(Paths { root: root.into() });
        }
        let root = match Os::current() {
            Some(Os::Macos) => home()?.join("Library/Application Support/OpenAgora"),
            Some(Os::Windows) => {
                PathBuf::from(env::var_os("APPDATA").context("APPDATA is not set")?)
                    .join("OpenAgora")
            }
            _ => match env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
                Some(data) => PathBuf::from(data).join("openagora"),
                None => home()?.join(".local/share/openagora"),
            },
        };
        Ok(Paths { root })
    }

    pub fn app_dir(&self, id: &str) -> PathBuf {
        self.root.join("apps").join(id)
    }

    pub fn state_file(&self, id: &str) -> PathBuf {
        self.root.join("state").join(format!("{id}.toml"))
    }

    pub fn run_file(&self, id: &str) -> PathBuf {
        self.root.join("run").join(format!("{id}.toml"))
    }

    pub fn log_file(&self, id: &str) -> PathBuf {
        self.root.join("logs").join(format!("{id}.log"))
    }

    pub fn tmp(&self) -> PathBuf {
        self.root.join("tmp")
    }
}

pub fn home() -> Result<PathBuf> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .context("cannot find the home directory (HOME is not set)")
}
