//! Running an app's Linux build in a Linux container, on computers with no
//! native build for it (`[container]` in the listing).
//!
//! OpenAgora drives a container runtime that is already installed (Docker
//! Desktop, OrbStack, Colima or Podman) through its `docker` or `podman`
//! command:
//!
//! - one base image, `openagora-linux`, built locally from Debian;
//! - per app, a volume `openagora-<id>` mounted as the container's home
//!   (`/openagora/home`, which can't clash with a shared home folder), so
//!   whatever the app's installer puts under `~` stays between runs;
//! - the app's folder mounted at `/openagora/app`, which is `{app_dir}` inside;
//! - each install step in a throwaway container, and the app itself in a
//!   container named `openagora-<id>` with its port published on 127.0.0.1.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

use crate::apps::resolve;
use crate::paths;

/// `{app_dir}` inside the container.
pub const APP_DIR: &str = "/openagora/app";
const HOME: &str = "/openagora/home";
const PATH: &str =
    "/openagora/home/.local/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
/// Bump the tag when the Dockerfile changes, so existing installs rebuild it.
const IMAGE: &str = "openagora-linux:1";
/// What the base image starts from. `OPENAGORA_CONTAINER_FROM` points it at
/// a mirror.
const FROM: &str = "debian:trixie-slim";
/// What official Linux installers and builds expect (curl, git, CA
/// certificates, xz; T3 Code's `t3` links libatomic), socat to forward the
/// published port to the app, and the libraries headless Chromium links
/// against, for agents' browser tools (Hermes's list, for Debian 13).
const DOCKERFILE: &str = "RUN apt-get update \\
 && apt-get install -y --no-install-recommends \\
    ca-certificates curl git libatomic1 procps socat xz-utils \\
    libasound2t64 libatk-bridge2.0-0t64 libatk1.0-0t64 libatspi2.0-0t64 libcairo2 \\
    libcups2t64 libdbus-1-3 libgbm1 libglib2.0-0t64 libnspr4 libnss3 libpango-1.0-0 \\
    libx11-6 libxcb1 libxcomposite1 libxdamage1 libxext6 libxfixes3 libxkbcommon0 libxrandr2 \\
 && rm -rf /var/lib/apt/lists/*
";

/// Apps listen on 127.0.0.1 inside the container, where a published port
/// can't reach them. Once the app is listening, socat accepts on the
/// container's own address and the same port and forwards to it, so the
/// app's address (and any sign-in link it prints) is the same on the host.
const FORWARD: &str = r#"port="$OPENAGORA_PORT"
(
  until (exec 3<>"/dev/tcp/127.0.0.1/$port") 2>/dev/null; do sleep 0.2; done
  read -r ip _ < <(getent ahostsv4 "$HOSTNAME")
  exec socat "TCP4-LISTEN:$port,bind=$ip,fork,reuseaddr" "TCP4:127.0.0.1:$port"
) &
exec "$@""#;

pub struct Runtime {
    cli: PathBuf,
}

impl Runtime {
    /// Finds the container runtime and checks that it is running. `why` says
    /// what needs it, e.g. "T3 Code runs in a Linux container on Intel Macs".
    pub fn find(why: &str) -> Result<Runtime> {
        let Some(cli) = find_cli() else {
            bail!(
                "{why}. Install a container runtime (Docker Desktop, OrbStack, Colima or Podman), start it, and try again."
            );
        };
        let runtime = Runtime { cli };
        // Plain `info` (docker and podman name its fields differently) fails
        // when the CLI can't reach a running daemon or VM.
        let info = runtime
            .cmd()
            .arg("info")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .output()
            .with_context(|| format!("running {}", runtime.cli.display()))?;
        if !info.status.success() {
            let err = String::from_utf8_lossy(&info.stderr);
            bail!(
                "{why}, but the container runtime is not running. Start Docker Desktop, OrbStack, Colima or Podman, and try again.\n{}",
                err.trim()
            );
        }
        Ok(runtime)
    }

    fn cmd(&self) -> Command {
        let mut cmd = Command::new(&self.cli);
        // Credential helpers (docker-credential-desktop, -osxkeychain) sit next
        // to the CLI, which a desktop app's minimal PATH may not include.
        if let Some(dir) = self.cli.parent() {
            let mut dirs = vec![dir.to_path_buf()];
            dirs.extend(
                std::env::var_os("PATH")
                    .map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
                    .unwrap_or_default(),
            );
            if let Ok(path) = std::env::join_paths(dirs) {
                cmd.env("PATH", path);
            }
        }
        cmd
    }

    /// Builds the base image unless it is already there. `scratch` is an
    /// empty folder to use as the build context.
    pub fn ensure_image(&self, scratch: &Path) -> Result<()> {
        let present = self
            .cmd()
            .args(["image", "inspect", IMAGE])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        if present {
            return Ok(());
        }
        println!("preparing the Linux container (first time only)");
        let _ = fs::remove_dir_all(scratch);
        fs::create_dir_all(scratch)?;
        let from = std::env::var("OPENAGORA_CONTAINER_FROM")
            .ok()
            .filter(|f| !f.trim().is_empty())
            .unwrap_or_else(|| FROM.into());
        let mut child = self
            .cmd()
            .args(["build", "-t", IMAGE, "-f", "-"])
            .arg(scratch)
            .stdin(Stdio::piped())
            .spawn()
            .context("building the Linux container image")?;
        child
            .stdin
            .take()
            .context("no stdin for the image build")?
            .write_all(format!("FROM {from}\n{DOCKERFILE}").as_bytes())?;
        if !child.wait()?.success() {
            bail!("building the Linux container image failed");
        }
        Ok(())
    }

    /// Runs one install step in a throwaway container.
    pub fn run_step(&self, app: &App, script: &str) -> Result<()> {
        let status = self
            .cmd()
            .args(["run", "--rm"])
            .args(app.mounts())
            .args([IMAGE, "sh", "-c", script])
            .stdin(Stdio::null())
            .status()
            .context("running an install step in the container")?;
        if !status.success() {
            bail!("install step failed in the container ({status}): {script}");
        }
        Ok(())
    }

    /// Whether `target` (a command on PATH or a path) exists in the container.
    pub fn provides(&self, app: &App, target: &str) -> Result<bool> {
        let status = self
            .cmd()
            .args(["run", "--rm"])
            .args(app.mounts())
            .args([
                IMAGE,
                "sh",
                "-c",
                r#"command -v "$1" >/dev/null 2>&1 || [ -e "$1" ]"#,
                "sh",
                target,
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .status()
            .context("checking the install in the container")?;
        Ok(status.success())
    }

    /// The program and arguments that run the app in its container, in the
    /// foreground, until it exits or is stopped.
    pub fn run_command(
        &self,
        app: &App,
        port: u16,
        command: &str,
        args: &[String],
    ) -> (PathBuf, Vec<String>) {
        let mut argv: Vec<String> = ["run", "--rm", "--init", "--name", &app.name()]
            .map(String::from)
            .into();
        argv.extend(["-p".into(), format!("127.0.0.1:{port}:{port}")]);
        argv.extend(["-e".into(), format!("OPENAGORA_PORT={port}")]);
        argv.extend(app.mounts());
        argv.extend([IMAGE, "bash", "-c", FORWARD, "bash", command].map(String::from));
        argv.extend(args.iter().cloned());
        (self.cli.clone(), argv)
    }

    /// Removes the app's container if one is left over. Never fails.
    pub fn remove_container(&self, app: &App) {
        let _ = self.quiet(&["rm", "-f", &app.name()]);
    }

    /// Removes the app's home volume, and everything installed in it.
    pub fn remove_volume(&self, app: &App) -> Result<()> {
        // Podman may fail on a volume that doesn't exist, even with -f.
        if !self.quiet(&["volume", "inspect", &app.name()])? {
            return Ok(());
        }
        if !self.quiet(&["volume", "rm", "-f", &app.name()])? {
            bail!("could not remove the container volume {}", app.name());
        }
        Ok(())
    }

    fn quiet(&self, args: &[&str]) -> Result<bool> {
        Ok(self
            .cmd()
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?
            .success())
    }
}

/// One app's container setup.
pub struct App<'a> {
    pub id: &'a str,
    pub app_dir: &'a Path,
    pub share_home: bool,
}

impl App<'_> {
    fn name(&self) -> String {
        format!("openagora-{}", self.id)
    }

    fn mounts(&self) -> Vec<String> {
        let mut out = vec![
            "-v".into(),
            format!("{}:{HOME}", self.name()),
            "-v".into(),
            format!("{}:{APP_DIR}", self.app_dir.display()),
            "-w".into(),
            APP_DIR.into(),
            "-e".into(),
            format!("HOME={HOME}"),
            "-e".into(),
            format!("PATH={PATH}"),
            "-e".into(),
            format!("OPENAGORA_APP_DIR={APP_DIR}"),
        ];
        // Same path inside, so projects open by the paths you know. Only a
        // Unix-style home can be mirrored into Linux.
        if self.share_home {
            if let Ok(home) = paths::home() {
                if home.is_absolute() && home.to_string_lossy().starts_with('/') {
                    out.extend(["-v".into(), format!("{0}:{0}", home.display())]);
                }
            }
        }
        out
    }
}

/// `OPENAGORA_CONTAINER_CLI`, else `docker`, else `podman`, looking on PATH
/// and where Docker Desktop, OrbStack and Homebrew put them.
fn find_cli() -> Option<PathBuf> {
    if let Some(cli) = std::env::var_os("OPENAGORA_CONTAINER_CLI").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(cli));
    }
    let mut extra = vec![
        PathBuf::from("/Applications/Docker.app/Contents/Resources/bin"),
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/opt/podman/bin"),
    ];
    if let Ok(home) = paths::home() {
        extra.extend([home.join(".docker/bin"), home.join(".orbstack/bin")]);
    }
    ["docker", "podman"].iter().find_map(|name| {
        resolve(name).or_else(|| extra.iter().map(|d| d.join(name)).find(|p| p.is_file()))
    })
}
