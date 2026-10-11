//! The engine daemon's local socket: a Unix-domain socket in OpenAgora's data
//! folder, readable only by this user, or a named pipe on Windows. Messages
//! are JSON, one per line.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use interprocess::local_socket::prelude::*;
use interprocess::local_socket::{ConnectOptions, ListenerOptions, Name, Stream};

use crate::github::sha256_hex;
use crate::paths::Paths;
use crate::process;

/// Where clients reach the daemon: a socket path, or a pipe path on Windows.
pub fn address(paths: &Paths) -> String {
    if cfg!(windows) {
        format!(r"\\.\pipe\{}", pipe_name(paths))
    } else {
        socket_path(paths).display().to_string()
    }
}

/// Unix socket paths must stay short (about 100 bytes on macOS), so a long data
/// folder uses a private folder under /tmp instead.
fn socket_path(paths: &Paths) -> PathBuf {
    let preferred = paths.root.join("run").join("engine.sock");
    if preferred.as_os_str().len() <= 100 {
        return preferred;
    }
    let digest = sha256_hex(paths.root.display().to_string().as_bytes());
    PathBuf::from(format!("/tmp/openagora-{}", user_id())).join(format!("{}.sock", &digest[..16]))
}

fn pipe_name(paths: &Paths) -> String {
    let digest = sha256_hex(paths.root.display().to_string().to_lowercase().as_bytes());
    format!("openagora-{}", &digest[..16])
}

#[cfg(unix)]
fn user_id() -> u32 {
    // SAFETY: getuid has no preconditions and cannot fail.
    unsafe { libc::getuid() }
}

#[cfg(not(unix))]
fn user_id() -> u32 {
    0
}

fn name(paths: &Paths) -> Result<Name<'static>> {
    #[cfg(windows)]
    {
        use interprocess::local_socket::GenericNamespaced;
        Ok(pipe_name(paths)
            .to_ns_name::<GenericNamespaced>()?
            .into_owned())
    }
    #[cfg(not(windows))]
    {
        use interprocess::local_socket::GenericFilePath;
        Ok(socket_path(paths)
            .to_fs_name::<GenericFilePath>()?
            .into_owned())
    }
}

/// Starts listening. Fails if another daemon already answers on the address.
pub fn listen(paths: &Paths) -> Result<interprocess::local_socket::Listener> {
    if connect(paths).is_ok() {
        bail!(
            "the OpenAgora engine is already running ({})",
            address(paths)
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let path = socket_path(paths);
        let dir = path.parent().context("socket path has no folder")?;
        std::fs::create_dir_all(dir)?;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
        // A socket file left by a daemon that died would block the address.
        let _ = std::fs::remove_file(&path);
    }
    // On Unix the socket's folder is private to this user (0700), which works
    // on every Unix, unlike a mode on the socket file itself.
    ListenerOptions::new()
        .name(name(paths)?)
        .create_sync()
        .with_context(|| format!("listening on {}", address(paths)))
}

/// Removes the socket file a stopped daemon leaves behind (Unix).
pub fn remove_address(paths: &Paths) {
    if cfg!(unix) {
        let _ = std::fs::remove_file(socket_path(paths));
    }
}

pub fn connect(paths: &Paths) -> Result<Stream> {
    ConnectOptions::new()
        .name(name(paths)?)
        .connect_sync()
        .with_context(|| format!("connecting to the OpenAgora engine at {}", address(paths)))
}

/// Connects to the daemon, starting it first if it isn't running.
pub fn connect_or_start(paths: &Paths, catalog: Option<&Path>) -> Result<Stream> {
    if let Ok(stream) = connect(paths) {
        return Ok(stream);
    }
    start_daemon(paths, catalog)?;
    connect(paths)
}

/// Starts `openagora serve` in the background and waits until it answers.
pub fn start_daemon(paths: &Paths, catalog: Option<&Path>) -> Result<()> {
    if connect(paths).is_ok() {
        return Ok(());
    }
    // A record left by a daemon that died must not count as the new one.
    let _ = std::fs::remove_file(paths.daemon_file());
    let exe = std::env::current_exe().context("finding the openagora executable")?;
    let log = paths.log_file("engine");
    if let Some(dir) = log.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut args = Vec::new();
    if let Some(dir) = catalog {
        args.push("--catalog".to_string());
        args.push(dir.display().to_string());
    }
    args.push("serve".to_string());
    let child = process::spawn(&exe, &args, &paths.root, &log)?;
    let pid = child.id();
    // Not waited on: the daemon outlives this command by design.
    drop(child);
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        // Up once it answers and has recorded itself.
        if paths.daemon_file().exists() && connect(paths).is_ok() {
            return Ok(());
        }
        if !process::alive(pid) {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    bail!(
        "the OpenAgora engine did not start; see {}",
        paths.log_file("engine").display()
    )
}

/// Sends the opening message that says what kind of client this is.
pub fn hello(stream: &mut Stream, role: &str, client: &str) -> Result<()> {
    let line = serde_json::json!({ "hello": { "role": role, "client": client } });
    writeln!(stream, "{line}")?;
    stream.flush()?;
    Ok(())
}

/// Reads one JSON message. None at end of stream.
pub fn read_message(reader: &mut impl BufRead) -> Result<Option<serde_json::Value>> {
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        if !line.trim().is_empty() {
            return Ok(Some(serde_json::from_str(line.trim())?));
        }
    }
}
