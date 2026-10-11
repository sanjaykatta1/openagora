//! Reading another app's text on Linux: Sunday's `read_window` against a real
//! GTK window, through the real accessibility bus (AT-SPI), on a private X
//! server and D-Bus session. Only the OS's window list comes from a file.
//! Skipped where Xvfb, D-Bus, AT-SPI or Python's GTK bindings aren't
//! installed.
#![cfg(target_os = "linux")]

mod common;

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use common::{Env, McpClient};
use serde_json::{Value, json};

/// Processes this test started, stopped when it ends.
struct Started(Vec<Child>);

impl Drop for Started {
    fn drop(&mut self) {
        for child in self.0.iter_mut().rev() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// A Python that has GTK 3's bindings (the system's `python3` may be newer
/// than the bindings it ships).
fn python_with_gtk() -> Option<String> {
    ["python3", "python3.12", "python3.11"]
        .into_iter()
        .find_map(|python| {
            Command::new(python)
                .args([
                    "-I",
                    "-c",
                    "import gi; gi.require_version('Gtk', '3.0'); from gi.repository import Gtk",
                ])
                .stderr(Stdio::null())
                .status()
                .ok()?
                .success()
                .then(|| python.to_string())
        })
}

const GTK_APP: &str = r#"
import os, sys
import gi
gi.require_version('Gtk', '3.0')
gi.require_version('GdkX11', '3.0')
from gi.repository import Gtk, GLib, GdkX11
window = Gtk.Window(title='Notes — Text Editor')
box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
box.pack_start(Gtk.Label(label='Release plan'), False, False, 0)
view = Gtk.TextView()
view.get_buffer().set_text('Version 2 ships on Friday.\nAsk Dana about the changelog.')
box.pack_start(view, True, True, 0)
box.pack_start(Gtk.Button(label='Save'), False, False, 0)
window.add(box)
window.set_default_size(400, 300)
window.show_all()
def ready():
    print('ready', window.get_window().get_xid(), os.getpid(), flush=True)
    return False
GLib.timeout_add(1500, ready)
Gtk.main()
"#;

#[test]
fn sunday_reads_a_gtk_windows_text_through_at_spi() {
    let Some(python) = python_with_gtk() else {
        eprintln!("skipped: no Python with GTK 3's bindings");
        return;
    };
    let launcher = [
        "/usr/libexec/at-spi-bus-launcher",
        "/usr/lib/at-spi2-core/at-spi-bus-launcher",
    ];
    if Command::new("Xvfb")
        .arg("-help")
        .stderr(Stdio::null())
        .output()
        .is_err()
        || Command::new("dbus-daemon")
            .arg("--version")
            .output()
            .is_err()
        || !launcher.iter().any(|p| Path::new(p).exists())
    {
        eprintln!("skipped: needs Xvfb, dbus-daemon and AT-SPI");
        return;
    }
    let mut started = Started(Vec::new());
    let number = (141..190)
        .find(|n| !Path::new(&format!("/tmp/.X11-unix/X{n}")).exists())
        .unwrap();
    let display = format!(":{number}");
    started.0.push(
        Command::new("Xvfb")
            .args([&display, "-nolisten", "tcp", "-screen", "0", "800x600x24"])
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let mut bus = Command::new("dbus-daemon")
        .args(["--session", "--nofork", "--nopidfile", "--print-address=1"])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut address = String::new();
    BufReader::new(bus.stdout.take().unwrap())
        .read_line(&mut address)
        .unwrap();
    started.0.push(bus);
    let address = address.trim().to_string();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !Path::new(&format!("/tmp/.X11-unix/X{number}")).exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }

    // A GTK app's window, with a label, some text and a button.
    let mut app = Command::new(&python)
        .args(["-I", "-c", GTK_APP])
        .env("DISPLAY", &display)
        .env("DBUS_SESSION_BUS_ADDRESS", &address)
        .env_remove("NO_AT_BRIDGE")
        .env_remove("AT_SPI_BUS_ADDRESS")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(app.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    started.0.push(app);
    let words: Vec<&str> = line.split_whitespace().collect();
    assert_eq!(words.first(), Some(&"ready"), "the GTK app said {line:?}");
    let (xid, pid): (u64, u32) = (words[1].parse().unwrap(), words[2].parse().unwrap());

    let mut env = Env::new();
    let focus = env.data.with_file_name("focus.json");
    std::fs::write(
        &focus,
        json!([{"id": xid, "pid": pid, "app": "Text Editor", "key": "text-editor", "title": "Notes — Text Editor"}]).to_string(),
    )
    .unwrap();
    env.env("OPENAGORA_FOCUS_FILE", focus.to_str().unwrap());
    env.env("DISPLAY", &display);
    env.env("DBUS_SESSION_BUS_ADDRESS", &address);
    env.env("AT_SPI_BUS_ADDRESS", "");
    env.ok(&["grants", "allow", "text-editor", "--name", "Text Editor"]);

    let mut sunday = McpClient::start(&env, "sunday", true);
    let (text, error) = sunday.call("read_window", json!({}), &mut |r: &Value| {
        panic!("unexpected question {r}")
    });
    assert!(!error, "{text}");
    for expected in [
        "Release plan",
        "Version 2 ships on Friday.",
        "Ask Dana about the changelog.",
        "Save",
    ] {
        assert!(text.contains(expected), "{expected:?} missing from {text}");
    }
    assert!(
        text.contains("window \u{201c}Notes — Text Editor\u{201d}"),
        "{text}"
    );
    let desk = env.json(&["desk", "list", "--json"]);
    assert_eq!(desk[0]["kind"], "text", "{desk}");
    assert_eq!(desk[0]["source"]["app"], "Text Editor");
    let audit = env.audit();
    assert!(
        audit
            .iter()
            .any(|e| e["action"] == "read_window" && e["decision"] == "allowed"),
        "{audit:?}"
    );
}
