//! Where OpenAgora keeps its data on each OS.
//!
//! ```text
//! <root>/apps/<id>/      the app's own folder ({app_dir})
//! <root>/state/<id>.toml what was installed
//! <root>/state/<id>.profile.toml  the personal agent's own profile
//! <root>/state/<id>.port.toml     the port an app with an MCP server keeps
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

    /// The personal agent's profile record (see `state::AgentProfile`).
    pub fn agent_profile_file(&self, id: &str) -> PathBuf {
        self.root.join("state").join(format!("{id}.profile.toml"))
    }

    /// The port an app with an MCP server keeps (see `state::StickyPort`).
    pub fn port_file(&self, id: &str) -> PathBuf {
        self.root.join("state").join(format!("{id}.port.toml"))
    }

    pub fn log_file(&self, id: &str) -> PathBuf {
        self.root.join("logs").join(format!("{id}.log"))
    }

    pub fn tmp(&self) -> PathBuf {
        self.root.join("tmp")
    }

    /// A copy of the engine at a fixed place, for other tools' configs (Sunday's
    /// MCP server and hook). The desktop app's own copy can move: a Linux
    /// AppImage runs from a new folder every time.
    pub fn stable_exe(&self) -> PathBuf {
        let name = if cfg!(windows) {
            "openagora.exe"
        } else {
            "openagora"
        };
        self.root.join("bin").join(name)
    }

    /// The daemon's record of itself while it runs.
    pub fn daemon_file(&self) -> PathBuf {
        self.root.join("run").join("daemon.toml")
    }

    /// Skills OpenAgora ships for Sunday (Hermes's `skills.external_dirs`).
    pub fn sunday_skills(&self) -> PathBuf {
        self.root.join("sunday").join("skills")
    }
}

/// Puts a copy of the running engine at `paths.stable_exe()`, unless it is
/// already there and identical. Returns its path.
pub fn install_stable_exe(paths: &Paths) -> Result<PathBuf> {
    let target = paths.stable_exe();
    let current = env::current_exe().context("finding the openagora executable")?;
    let same_file = |a: &PathBuf, b: &PathBuf| matches!((a.canonicalize(), b.canonicalize()), (Ok(a), Ok(b)) if a == b);
    if same_file(&current, &target) {
        return Ok(target);
    }
    let bytes =
        std::fs::read(&current).with_context(|| format!("reading {}", current.display()))?;
    if std::fs::read(&target).is_ok_and(|existing| existing == bytes) {
        return Ok(target);
    }
    let dir = target.parent().context("no folder for the engine copy")?;
    std::fs::create_dir_all(dir)?;
    let fresh = target.with_extension("new");
    std::fs::write(&fresh, &bytes).with_context(|| format!("writing {}", fresh.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fresh, std::fs::Permissions::from_mode(0o755))?;
    }
    if std::fs::rename(&fresh, &target).is_err() {
        // Windows can't replace an executable that is running, but it can
        // rename it out of the way.
        let aside = target.with_extension(format!("old-{}", crate::state::now()));
        std::fs::rename(&target, &aside)?;
        std::fs::rename(&fresh, &target)?;
    }
    // Copies set aside earlier go once nothing runs them any more.
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().contains(".old-") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    Ok(target)
}

pub fn home() -> Result<PathBuf> {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .context("cannot find the home directory (HOME is not set)")
}
