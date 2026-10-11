//! Which window is in front, on X11: the engine's query against a real X
//! server (Xvfb), with the hints a window manager keeps set by hand. Skipped
//! where Xvfb isn't installed.
#![cfg(target_os = "linux")]

use std::path::Path;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

use serde_json::Value;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ConnectionExt as _, CreateWindowAux, PropMode, Window, WindowClass,
};
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

/// A private X server for one test.
struct Xvfb {
    child: Child,
    display: String,
}

impl Xvfb {
    fn start() -> Option<Xvfb> {
        Command::new("Xvfb").arg("-help").output().ok()?;
        let number = (91..140).find(|n| !Path::new(&format!("/tmp/.X11-unix/X{n}")).exists())?;
        let display = format!(":{number}");
        let child = Command::new("Xvfb")
            .args([&display, "-nolisten", "tcp", "-screen", "0", "800x600x24"])
            .spawn()
            .ok()?;
        let server = Xvfb { child, display };
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if x11rb::connect(Some(&server.display)).is_ok() {
                return Some(server);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        None
    }
}

impl Drop for Xvfb {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn atom(conn: &RustConnection, name: &str) -> u32 {
    conn.intern_atom(false, name.as_bytes())
        .unwrap()
        .reply()
        .unwrap()
        .atom
}

/// A top-level window with the properties apps set on themselves.
fn app_window(conn: &RustConnection, root: Window, class: &[u8], title: &str, pid: u32) -> Window {
    let window = conn.generate_id().unwrap();
    conn.create_window(
        x11rb::COPY_DEPTH_FROM_PARENT,
        window,
        root,
        0,
        0,
        200,
        100,
        0,
        WindowClass::INPUT_OUTPUT,
        0,
        &CreateWindowAux::new(),
    )
    .unwrap();
    conn.change_property8(
        PropMode::REPLACE,
        window,
        AtomEnum::WM_CLASS,
        AtomEnum::STRING,
        class,
    )
    .unwrap();
    let utf8 = atom(conn, "UTF8_STRING");
    conn.change_property8(
        PropMode::REPLACE,
        window,
        atom(conn, "_NET_WM_NAME"),
        utf8,
        title.as_bytes(),
    )
    .unwrap();
    conn.change_property32(
        PropMode::REPLACE,
        window,
        atom(conn, "_NET_WM_PID"),
        AtomEnum::CARDINAL,
        &[pid],
    )
    .unwrap();
    conn.map_window(window).unwrap();
    window
}

fn focus(display: &str, data: &Path, wayland: bool) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_openagora"));
    cmd.args(["focus", "--json"])
        .env("DISPLAY", display)
        .env("HOME", data)
        .env("OPENAGORA_HOME", data.join("openagora"))
        .env("XDG_DATA_HOME", data.join("share"))
        .env("XDG_DATA_DIRS", data.join("system"))
        .env_remove("XDG_SESSION_TYPE")
        .env_remove("OPENAGORA_FOCUS_FILE");
    if wayland {
        cmd.env("WAYLAND_DISPLAY", "wayland-0");
    } else {
        cmd.env_remove("WAYLAND_DISPLAY");
    }
    cmd.output().unwrap()
}

#[test]
fn the_front_window_and_the_ones_behind_come_from_the_window_manager() {
    let Some(server) = Xvfb::start() else {
        eprintln!("Xvfb not found; skipping");
        return;
    };
    let data = tempfile::tempdir().unwrap();
    // The editor's desktop entry gives its name for people.
    let apps = data.path().join("share/applications");
    std::fs::create_dir_all(&apps).unwrap();
    std::fs::write(
        apps.join("code.desktop"),
        "[Desktop Entry]\nName=Visual Studio Code\nStartupWMClass=Code\nExec=code\n",
    )
    .unwrap();

    let (conn, screen) = x11rb::connect(Some(&server.display)).unwrap();
    let root = conn.setup().roots[screen].root;
    let editor = app_window(
        &conn,
        root,
        b"code\0Code\0",
        "main.rs \u{2014} Visual Studio Code",
        111,
    );
    let browser = app_window(&conn, root, b"Navigator\0testbrowser\0", "Docs", 222);
    let minimized = app_window(&conn, root, b"music\0Music\0", "Song", 333);
    let dock = app_window(&conn, root, b"panel\0Panel\0", "Panel", 444);
    conn.change_property32(
        PropMode::REPLACE,
        dock,
        atom(&conn, "_NET_WM_WINDOW_TYPE"),
        AtomEnum::ATOM,
        &[atom(&conn, "_NET_WM_WINDOW_TYPE_DOCK")],
    )
    .unwrap();
    conn.change_property32(
        PropMode::REPLACE,
        minimized,
        atom(&conn, "_NET_WM_STATE"),
        AtomEnum::ATOM,
        &[atom(&conn, "_NET_WM_STATE_HIDDEN")],
    )
    .unwrap();
    // What a window manager keeps on the root window: the stacking order,
    // bottom to top, and the window with the focus.
    conn.change_property32(
        PropMode::REPLACE,
        root,
        atom(&conn, "_NET_CLIENT_LIST_STACKING"),
        AtomEnum::WINDOW,
        &[minimized, dock, editor, browser],
    )
    .unwrap();
    conn.change_property32(
        PropMode::REPLACE,
        root,
        atom(&conn, "_NET_ACTIVE_WINDOW"),
        AtomEnum::WINDOW,
        &[editor],
    )
    .unwrap();
    conn.flush().unwrap();

    let out = focus(&server.display, data.path(), false);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let windows: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    // The focused window first, then the rest top to bottom; docks and
    // minimized windows aren't windows the user is working in.
    assert_eq!(windows.len(), 2, "{windows:?}");
    assert_eq!(windows[0]["id"], editor);
    assert_eq!(windows[0]["key"], "code");
    assert_eq!(windows[0]["app"], "Visual Studio Code");
    assert_eq!(windows[0]["title"], "main.rs \u{2014} Visual Studio Code");
    assert_eq!(windows[0]["pid"], 111);
    assert_eq!(windows[1]["id"], browser);
    assert_eq!(windows[1]["key"], "testbrowser");
    assert_eq!(windows[1]["app"], "testbrowser");

    // Under Wayland, other apps' windows are out of sight.
    let out = focus(&server.display, data.path(), true);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("Wayland"));
}
