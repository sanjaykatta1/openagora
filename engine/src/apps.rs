//! Installing, starting and stopping apps from their listings.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, bail};

use crate::github;
use crate::manifest::{Arch, Health, Manifest, Os, Port, Role, Shell};
use crate::paths::{self, Paths};
use crate::process;
use crate::state::{self, Installed, Running};

/// How long a web app gets to start listening.
const START_TIMEOUT: Duration = Duration::from_secs(90);
/// How long a background app must stay up to count as started.
const SETTLE: Duration = Duration::from_secs(2);
const STOP_GRACE: Duration = Duration::from_secs(10);
/// How long to wait, once the app is up, for it to print its address.
const LOGGED_URL_TIMEOUT: Duration = Duration::from_secs(15);

pub struct Engine {
    pub paths: Paths,
    pub os: Os,
    pub arch: Option<Arch>,
}

#[derive(Debug)]
pub enum Status {
    Running(Running),
    Stopped,
}

impl Engine {
    pub fn new() -> Result<Engine> {
        Ok(Engine {
            paths: Paths::from_env()?,
            os: Os::current().context("this operating system is not supported")?,
            arch: Arch::current(),
        })
    }

    pub fn installed(&self, id: &str) -> Result<Option<Installed>> {
        state::read(&self.paths.state_file(id))
    }

    pub fn install(&self, m: &Manifest) -> Result<Installed> {
        if !m.platforms.contains(&self.os) {
            bail!("{} is not available on {}", m.name, self.os);
        }
        if self.installed(&m.id)?.is_some() {
            bail!("{} is already installed", m.name);
        }
        let app_dir = self.paths.app_dir(&m.id);
        // Leftovers from an interrupted install would confuse the new one.
        if app_dir.exists() {
            fs::remove_dir_all(&app_dir)
                .with_context(|| format!("clearing {}", app_dir.display()))?;
        }
        fs::create_dir_all(&app_dir).with_context(|| format!("creating {}", app_dir.display()))?;
        let tmp = self.paths.tmp().join(&m.id);
        let result = self.run_install_steps(m, &app_dir, &tmp);
        let _ = fs::remove_dir_all(&tmp);
        match result {
            Ok(assets) => {
                let installed = Installed {
                    id: m.id.clone(),
                    installed_at: state::now(),
                    assets,
                };
                state::write(&self.paths.state_file(&m.id), &installed)?;
                Ok(installed)
            }
            Err(e) => {
                let _ = fs::remove_dir_all(&app_dir);
                Err(e.context(format!("installing {} failed; nothing was kept", m.name)))
            }
        }
    }

    fn run_install_steps(&self, m: &Manifest, app_dir: &Path, tmp: &Path) -> Result<Vec<String>> {
        let mut assets = Vec::new();
        for step in m.steps_for(self.os) {
            let mut vars = vars(&[("app_dir", app_dir)]);
            if let Some(download) = &step.download {
                let arch = self.arch.context("this CPU is not supported")?;
                let pattern = download
                    .asset
                    .for_arch(arch)
                    .with_context(|| format!("{} has no build for this CPU", m.name))?;
                let release = github::latest_release(&download.github)?;
                let asset = github::pick(&release, pattern)?;
                fs::create_dir_all(tmp)?;
                let dest = tmp.join(&asset.name);
                println!("downloading {} ({})", asset.name, release.tag);
                github::download(asset, &dest)?;
                assets.push(format!("{}@{}", asset.name, release.tag));
                vars.insert("download", dest.display().to_string());
            }
            let script = fill(&step.run, &vars);
            run_shell(step.shell, &script, app_dir)?;
        }
        let provides = m
            .install
            .provides
            .for_os(self.os)
            .context("listing has no provides for this OS")?;
        let provides = fill(provides, &vars(&[("app_dir", app_dir)]));
        if resolve(&provides).is_none() {
            bail!("the install finished but {provides} was not found");
        }
        Ok(assets)
    }

    pub fn uninstall(&self, m: &Manifest) -> Result<()> {
        if m.role == Role::Agent {
            bail!(
                "{} is OpenAgora's built-in agent and cannot be removed",
                m.name
            );
        }
        if self.installed(&m.id)?.is_none() {
            bail!("{} is not installed", m.name);
        }
        if let Status::Running(_) = self.status(&m.id)? {
            self.stop(m)?;
        }
        let app_dir = self.paths.app_dir(&m.id);
        if let Some(cmd) = &m.uninstall {
            let shell = if self.os == Os::Windows {
                Shell::Powershell
            } else {
                Shell::Sh
            };
            run_shell(
                shell,
                &fill(&cmd.run, &vars(&[("app_dir", &app_dir)])),
                &app_dir,
            )?;
        }
        if app_dir.exists() {
            fs::remove_dir_all(&app_dir)
                .with_context(|| format!("removing {}", app_dir.display()))?;
        }
        state::remove(&self.paths.state_file(&m.id))?;
        state::remove(&self.paths.run_file(&m.id))?;
        Ok(())
    }

    /// Running only if the recorded process is still alive; clears stale records.
    pub fn status(&self, id: &str) -> Result<Status> {
        let run_file = self.paths.run_file(id);
        match state::read::<Running>(&run_file)? {
            Some(running) if process::alive(running.pid) => Ok(Status::Running(running)),
            Some(_) => {
                state::remove(&run_file)?;
                Ok(Status::Stopped)
            }
            None => Ok(Status::Stopped),
        }
    }

    pub fn start(&self, m: &Manifest) -> Result<Running> {
        if self.installed(&m.id)?.is_none() {
            bail!("{} is not installed (openagora install {})", m.name, m.id);
        }
        if let Status::Running(running) = self.status(&m.id)? {
            return Ok(running);
        }
        let app_dir = self.paths.app_dir(&m.id);
        let port = match &m.run.port {
            Some(Port::Fixed(port)) => Some(*port),
            Some(Port::Named(_)) => Some(process::free_port()?),
            None => None,
        };
        let mut vars = vars(&[("app_dir", &app_dir)]);
        if let Some(port) = port {
            vars.insert("port", port.to_string());
        }
        let command = m
            .run
            .command
            .for_os(self.os)
            .context("listing has no run command for this OS")?;
        let command = fill(command, &vars);
        let program = resolve(&command).with_context(|| format!("{command} was not found"))?;
        let args: Vec<String> = m.run.args.iter().map(|a| fill(a, &vars)).collect();
        let log = self.paths.log_file(&m.id);
        // Only output from this start counts when looking for the app's address.
        let log_start = fs::metadata(&log).map(|meta| meta.len()).unwrap_or(0);
        let mut child = process::spawn(&program, &args, &app_dir, &log)?;
        let pid = child.id();
        let mut running = Running {
            pid,
            port,
            url: m.ui.url.as_ref().map(|u| fill(u, &vars)),
            started_at: state::now(),
        };
        state::write(&self.paths.run_file(&m.id), &running)?;

        let ready = match (&m.run.health, port) {
            (Health { tcp: true, .. }, Some(port)) => {
                process::wait_healthy(&mut child, START_TIMEOUT, || process::port_open(port))
            }
            _ => {
                std::thread::sleep(SETTLE);
                if process::child_running(&mut child) {
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("the app exited right after starting"))
                }
            }
        };
        if let Err(e) = ready {
            let _ = process::stop(pid, STOP_GRACE);
            let _ = child.wait();
            state::remove(&self.paths.run_file(&m.id))?;
            let tail = tail(&log, 15).unwrap_or_default();
            bail!(
                "{} did not start: {e}\nlast lines of {}:\n{tail}",
                m.name,
                log.display()
            );
        }
        if let Some(marker) = &m.ui.url_from_log_after {
            match wait_for_logged_url(&log, log_start, marker, LOGGED_URL_TIMEOUT) {
                Some(url) => {
                    running.url = Some(url);
                    state::write(&self.paths.run_file(&m.id), &running)?;
                }
                None => eprintln!(
                    "warning: {} did not print its address after {marker:?}; using the default address",
                    m.name
                ),
            }
        }
        Ok(running)
    }

    pub fn stop(&self, m: &Manifest) -> Result<bool> {
        match self.status(&m.id)? {
            Status::Running(running) => {
                process::stop(running.pid, STOP_GRACE)?;
                state::remove(&self.paths.run_file(&m.id))?;
                Ok(true)
            }
            Status::Stopped => Ok(false),
        }
    }
}

fn vars(pairs: &[(&'static str, &Path)]) -> BTreeMap<&'static str, String> {
    pairs
        .iter()
        .map(|(k, v)| (*k, v.display().to_string()))
        .collect()
}

/// Replaces `{name}` placeholders. Validation already rejected unknown names.
pub fn fill(text: &str, vars: &BTreeMap<&'static str, String>) -> String {
    vars.iter().fold(text.to_string(), |acc, (k, v)| {
        acc.replace(&format!("{{{k}}}"), v)
    })
}

fn run_shell(shell: Shell, script: &str, cwd: &Path) -> Result<()> {
    let mut cmd = match shell {
        Shell::Sh => {
            let mut c = Command::new("sh");
            c.args(["-c", script]);
            c
        }
        Shell::Powershell => {
            let mut c = Command::new("powershell");
            c.args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                script,
            ]);
            c
        }
    };
    let status = cmd
        .current_dir(cwd)
        .env("OPENAGORA_APP_DIR", cwd)
        .status()
        .context("running an install step")?;
    if !status.success() {
        bail!("install step failed ({status}): {script}");
    }
    Ok(())
}

/// A path that exists, or a command name found on PATH. Official installers
/// often put commands in ~/.local/bin, which may not be on PATH yet.
pub fn resolve(target: &str) -> Option<PathBuf> {
    let path = Path::new(target);
    if path.components().count() > 1 || path.is_absolute() {
        return path.exists().then(|| path.to_path_buf());
    }
    let mut dirs: Vec<PathBuf> = env::var_os("PATH")
        .map(|p| env::split_paths(&p).collect())
        .unwrap_or_default();
    if let Ok(home) = paths::home() {
        dirs.push(home.join(".local/bin"));
    }
    let exts: Vec<String> = if cfg!(windows) {
        env::var("PATHEXT")
            .unwrap_or_else(|_| ".EXE;.CMD;.BAT".into())
            .split(';')
            .map(str::to_lowercase)
            .chain([String::new()])
            .collect()
    } else {
        vec![String::new()]
    };
    dirs.iter()
        .flat_map(|d| exts.iter().map(move |e| d.join(format!("{target}{e}"))))
        .find(|p| p.is_file())
}

/// Finds the first local address printed after `marker` in the log, reading
/// only what was written since `from`. Anything not on 127.0.0.1/localhost is
/// ignored, so an app can't point its tab at another site.
fn wait_for_logged_url(log: &Path, from: u64, marker: &str, timeout: Duration) -> Option<String> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if let Some(url) = fs::read(log).ok().and_then(|bytes| {
            let start = usize::try_from(from).ok()?.min(bytes.len());
            let text = String::from_utf8_lossy(&bytes[start..]).into_owned();
            logged_url(&text, marker)
        }) {
            return Some(url);
        }
        if std::time::Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

pub fn logged_url(text: &str, marker: &str) -> Option<String> {
    text.match_indices(marker).find_map(|(i, _)| {
        let url = text[i + marker.len()..].split_whitespace().next()?;
        ["http://127.0.0.1:", "http://localhost:"]
            .iter()
            .any(|prefix| url.starts_with(prefix))
            .then(|| url.to_string())
    })
}

pub fn tail(path: &Path, lines: usize) -> Result<String> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let all: Vec<&str> = text.lines().collect();
    Ok(all[all.len().saturating_sub(lines)..].join("\n"))
}

#[cfg(test)]
mod tests {
    use super::logged_url;

    #[test]
    fn finds_local_urls_after_the_marker_only() {
        let log = "starting\nPairing URL: http://127.0.0.1:4000/pair#token=x\nready";
        assert_eq!(
            logged_url(log, "Pairing URL: ").as_deref(),
            Some("http://127.0.0.1:4000/pair#token=x")
        );
        assert_eq!(
            logged_url("Pairing URL: https://evil.example/", "Pairing URL: "),
            None
        );
        assert_eq!(logged_url("nothing here", "Pairing URL: "), None);
        // A later local URL is used if an earlier one isn't local.
        let mixed = "Pairing URL: https://x.example/ then Pairing URL: http://localhost:9/a";
        assert_eq!(
            logged_url(mixed, "Pairing URL: ").as_deref(),
            Some("http://localhost:9/a")
        );
    }
}
