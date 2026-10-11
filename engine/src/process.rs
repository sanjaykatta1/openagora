//! Starting, checking and stopping app processes.
//!
//! Apps are started detached, in their own process group on Unix, so they keep
//! running after `openagora` exits and `stop` can end the app together with any
//! helper processes it started.

use std::fs::{self, OpenOptions};
use std::io::Read;
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::{self, sleep};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

/// Keeps file descriptors this process inherited, besides stdin, stdout and
/// stderr, from passing on to anything it starts. OpenAgora's window leaks
/// some of its own (pipes, sockets, open files) into the engine; without this
/// the engine service and every app it starts would hold them open for as
/// long as they run.
#[cfg(unix)]
pub fn stop_fd_inheritance() {
    // /dev/fd lists this process's open descriptors on Linux and macOS.
    let Ok(entries) = fs::read_dir("/dev/fd") else {
        return;
    };
    let fds: Vec<i32> = entries
        .flatten()
        .filter_map(|e| e.file_name().to_str().and_then(|n| n.parse().ok()))
        .filter(|fd| *fd > 2)
        .collect();
    for fd in fds {
        // SAFETY: only reads and sets the descriptor's close-on-exec flag; a
        // descriptor that is already closed makes fcntl fail harmlessly.
        unsafe {
            let flags = libc::fcntl(fd, libc::F_GETFD);
            if flags >= 0 && flags & libc::FD_CLOEXEC == 0 {
                libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC);
            }
        }
    }
}

#[cfg(not(unix))]
pub fn stop_fd_inheritance() {}

/// Runs `cmd` to completion with no input, capturing its output, and kills it
/// if it takes longer than `timeout`.
pub fn output_within(cmd: &mut Command, timeout: Duration) -> Result<Output> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    // Read into shared buffers: a helper the command leaves running in the
    // background can keep a pipe open long after the command itself exits.
    let drain = |pipe: Option<Box<dyn Read + Send>>| {
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&buffer);
        let reader = thread::spawn(move || {
            let Some(mut pipe) = pipe else { return };
            let mut chunk = [0u8; 8192];
            while let Ok(n) = pipe.read(&mut chunk) {
                if n == 0 {
                    break;
                }
                sink.lock().unwrap().extend_from_slice(&chunk[..n]);
            }
        });
        (buffer, reader)
    };
    let stdout = drain(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let stderr = drain(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            bail!("timed out after {}s", timeout.as_secs());
        }
        sleep(Duration::from_millis(50));
    };
    let settle = Instant::now() + Duration::from_secs(2);
    while !(stdout.1.is_finished() && stderr.1.is_finished()) && Instant::now() < settle {
        sleep(Duration::from_millis(20));
    }
    let take = |buffer: &Arc<Mutex<Vec<u8>>>| std::mem::take(&mut *buffer.lock().unwrap());
    Ok(Output {
        status,
        stdout: take(&stdout.0),
        stderr: take(&stderr.0),
    })
}

pub fn spawn(program: &Path, args: &[String], cwd: &Path, log: &Path) -> Result<Child> {
    if let Some(dir) = log.parent() {
        fs::create_dir_all(dir)?;
    }
    let out = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
        .with_context(|| format!("opening {}", log.display()))?;
    let mut cmd = Command::new(program);
    cmd.args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(out.try_clone()?)
        .stderr(out);
    detach(&mut cmd);
    cmd.spawn()
        .with_context(|| format!("starting {}", program.display()))
}

/// Starts `cmd` detached, as apps are started: it keeps running after
/// `openagora` exits.
pub fn spawn_detached(cmd: &mut Command) -> Result<Child> {
    detach(cmd);
    Ok(cmd.spawn()?)
}

#[cfg(unix)]
fn detach(cmd: &mut Command) {
    use std::os::unix::process::CommandExt;
    cmd.process_group(0);
}

#[cfg(windows)]
fn detach(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
}

#[cfg(unix)]
pub fn alive(pid: u32) -> bool {
    let Ok(raw) = libc::pid_t::try_from(pid) else {
        return false;
    };
    // SAFETY: signal 0 only checks that the process exists.
    let rc = unsafe { libc::kill(raw, 0) };
    let exists = rc == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM);
    exists && !zombie(pid)
}

/// An exited process its parent has not collected yet still "exists" to
/// kill(0); it is not running.
#[cfg(target_os = "linux")]
fn zombie(pid: u32) -> bool {
    fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .and_then(|stat| {
            let state = stat.rsplit_once(')')?.1.trim_start().chars().next()?;
            Some(state == 'Z' || state == 'X')
        })
        .unwrap_or(false)
}

#[cfg(all(unix, not(target_os = "linux")))]
fn zombie(pid: u32) -> bool {
    Command::new("ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .trim_start()
                .starts_with('Z')
        })
        .unwrap_or(false)
}

#[cfg(windows)]
pub fn alive(pid: u32) -> bool {
    Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains(&format!("\"{pid}\"")))
        .unwrap_or(false)
}

/// Asks the app to exit, then forces it after `grace`.
#[cfg(unix)]
pub fn stop(pid: u32, grace: Duration) -> Result<()> {
    let group = -libc::pid_t::try_from(pid).context("pid out of range")?;
    // SAFETY: sends a signal to the app's own process group.
    unsafe { libc::kill(group, libc::SIGTERM) };
    if wait_exit(pid, grace) {
        return Ok(());
    }
    // SAFETY: as above.
    unsafe { libc::kill(group, libc::SIGKILL) };
    if wait_exit(pid, Duration::from_secs(5)) {
        Ok(())
    } else {
        bail!("process {pid} did not exit")
    }
}

#[cfg(windows)]
pub fn stop(pid: u32, grace: Duration) -> Result<()> {
    let pid_s = pid.to_string();
    let _ = Command::new("taskkill")
        .args(["/PID", &pid_s, "/T"])
        .output();
    if wait_exit(pid, grace) {
        return Ok(());
    }
    let _ = Command::new("taskkill")
        .args(["/PID", &pid_s, "/T", "/F"])
        .output();
    if wait_exit(pid, Duration::from_secs(5)) {
        Ok(())
    } else {
        bail!("process {pid} did not exit")
    }
}

fn wait_exit(pid: u32, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if !alive(pid) {
            return true;
        }
        sleep(Duration::from_millis(100));
    }
    !alive(pid)
}

/// A port nothing is listening on right now.
pub fn free_port() -> Result<u16> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).context("finding a free port")?;
    Ok(listener.local_addr()?.port())
}

pub fn port_open(port: u16) -> bool {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    TcpStream::connect_timeout(&addr, Duration::from_millis(300)).is_ok()
}

/// True while the child we started is still running (and reaps it if not).
pub fn child_running(child: &mut Child) -> bool {
    matches!(child.try_wait(), Ok(None))
}

/// Waits until `check` passes, failing early if the process exits.
pub fn wait_healthy(child: &mut Child, timeout: Duration, check: impl Fn() -> bool) -> Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        if check() {
            return Ok(());
        }
        if !child_running(child) {
            bail!("the app exited during startup");
        }
        if Instant::now() >= deadline {
            bail!("the app did not become ready within {}s", timeout.as_secs());
        }
        sleep(Duration::from_millis(250));
    }
}
