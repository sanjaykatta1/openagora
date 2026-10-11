//! The text on another app's window, read through the OS's accessibility API,
//! only when asked: Sunday's `read_window`, or background mode for apps the
//! user allowed. Nothing here is kept; the caller decides. (OpenAgora's own
//! tabs are read by OpenAgora's window from the app's page.)
//!
//! - macOS: the app's accessibility tree (AXUIElement), from a JavaScript for
//!   Automation script run with `osascript`. It needs Accessibility permission
//!   for OpenAgora (System Settings → Privacy & Security → Accessibility).
//!   Chromium and Electron apps build their tree when asked
//!   (`AXManualAccessibility`).
//! - Windows: UI Automation, from a PowerShell script, starting at the window.
//! - Linux: AT-SPI, over D-Bus, for apps that publish their text there (GTK
//!   apps do; browsers and Electron apps when the system's accessibility
//!   support is on). It works on Wayland too, but there OpenAgora can't tell
//!   which window is in front.
//!
//! `OPENAGORA_SCREEN_TEXT_FILE` replaces the OS with a JSON file, for tests:
//! `{"<window id>": {"title": "…", "text": "…"}}`, or `{"unavailable": "why"}`.

use std::path::Path;

use serde_json::Value;

use crate::focus::Window;

/// What a window shows, as text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Read {
    /// The window's title as the accessibility API reports it.
    pub title: Option<String>,
    pub text: String,
}

/// The most text kept from one window, in characters.
pub const MAX_CHARS: usize = 60_000;
/// How many elements of a window's accessibility tree are looked at, at most.
#[cfg_attr(
    not(any(target_os = "linux", windows, target_os = "macos")),
    allow(dead_code)
)]
const MAX_NODES: usize = 4_000;

/// The text on `window`. An error says why it couldn't be read.
pub fn read(window: &Window) -> Result<Read, String> {
    if let Some(file) = std::env::var_os("OPENAGORA_SCREEN_TEXT_FILE").filter(|v| !v.is_empty()) {
        return from_file(Path::new(&file), window);
    }
    let read = os::read(window)?;
    Ok(Read {
        title: read.title.filter(|t| !t.trim().is_empty()),
        text: tidy(&read.text),
    })
}

fn from_file(path: &Path, window: &Window) -> Result<Read, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let value: Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    if let Some(reason) = value["unavailable"].as_str() {
        return Err(reason.to_string());
    }
    let entry = &value[window.id.to_string()];
    if entry.is_null() {
        return Err(format!("{} shares no text", window.app));
    }
    Ok(Read {
        title: entry["title"].as_str().map(str::to_string),
        text: tidy(entry["text"].as_str().unwrap_or_default()),
    })
}

/// One line per piece of text, without blank runs or a line repeated right
/// after itself (a link and its label, say), cut at `MAX_CHARS`.
pub fn tidy(text: &str) -> String {
    let mut out = String::new();
    let mut last = "";
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line == last {
            continue;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(line);
        last = line;
        if out.chars().count() >= MAX_CHARS {
            break;
        }
    }
    match out.char_indices().nth(MAX_CHARS) {
        Some((cut, _)) => out[..cut].to_string(),
        None => out,
    }
}

#[cfg(target_os = "macos")]
mod os {
    use std::process::Command;
    use std::time::Duration;

    use serde_json::Value;

    use super::{MAX_CHARS, MAX_NODES, Read};
    use crate::focus::Window;

    /// Reads one window of the app with process `argv[0]`: the one titled
    /// `argv[1]`, else the app's focused or main window. Visible text only:
    /// static text, text fields and areas, and the titles of buttons, tabs and
    /// the like.
    const SCRIPT: &str = r#"
ObjC.import('ApplicationServices');
ObjC.import('Foundation');
function run(argv) {
  var pid = parseInt(argv[0], 10);
  var wanted = argv.length > 1 ? argv[1] : '';
  var maxNodes = parseInt(argv[2], 10), maxChars = parseInt(argv[3], 10);
  if (!$.AXIsProcessTrusted()) return JSON.stringify({ error: 'accessibility' });
  var app = $.AXUIElementCreateApplication(pid);
  // Chromium and Electron apps build their accessibility tree only when asked.
  try { $.AXUIElementSetAttributeValue(app, 'AXManualAccessibility', $.NSNumber.numberWithBool(true)); } catch (e) {}
  function get(el, name) {
    var ref = Ref();
    try { if ($.AXUIElementCopyAttributeValue(el, name, ref) !== 0) return null; } catch (e) { return null; }
    return ref[0] === undefined ? null : ref[0];
  }
  function str(value) {
    if (value === null) return '';
    try { var s = ObjC.unwrap(ObjC.castRefToObject(value)); return typeof s === 'string' ? s : ''; } catch (e) { return ''; }
  }
  function list(value) {
    if (value === null) return [];
    try {
      var array = ObjC.castRefToObject(value), out = [], n = array.count;
      for (var i = 0; i < n; i++) out.push(array.objectAtIndex(i));
      return out;
    } catch (e) { return []; }
  }
  var windows = list(get(app, 'AXWindows'));
  var win = null;
  for (var i = 0; i < windows.length && !win; i++) if (wanted && str(get(windows[i], 'AXTitle')) === wanted) win = windows[i];
  if (!win) win = get(app, 'AXFocusedWindow') || get(app, 'AXMainWindow') || windows[0] || null;
  if (win === null) return JSON.stringify({ error: 'no-window' });
  var VALUE = { AXStaticText: 1, AXTextField: 1, AXTextArea: 1, AXComboBox: 1 };
  var TITLE = { AXButton: 1, AXMenuButton: 1, AXRadioButton: 1, AXCheckBox: 1, AXPopUpButton: 1, AXTab: 1 };
  var lines = [], chars = 0, nodes = 0;
  function walk(el, depth) {
    if (nodes >= maxNodes || chars >= maxChars || depth > 60) return;
    nodes++;
    var role = str(get(el, 'AXRole'));
    var text = VALUE[role] ? str(get(el, 'AXValue')) : TITLE[role] ? (str(get(el, 'AXTitle')) || str(get(el, 'AXDescription'))) : '';
    if (text) { lines.push(text); chars += text.length; }
    if (role === 'AXTextField' || role === 'AXTextArea') return;
    var kids = list(get(el, 'AXChildren'));
    for (var k = 0; k < kids.length; k++) walk(kids[k], depth + 1);
  }
  walk(win, 0);
  return JSON.stringify({ title: str(get(win, 'AXTitle')), text: lines.join('\n') });
}
"#;

    pub fn read(window: &Window) -> Result<Read, String> {
        let mut cmd = Command::new("/usr/bin/osascript");
        cmd.args(["-l", "JavaScript", "-e", SCRIPT])
            .arg(window.pid.to_string())
            .arg(window.title.as_deref().unwrap_or_default())
            .arg(MAX_NODES.to_string())
            .arg(MAX_CHARS.to_string());
        let out = crate::process::output_within(&mut cmd, Duration::from_secs(15))
            .map_err(|e| format!("couldn't run osascript: {e:#}"))?;
        if !out.status.success() {
            return Err(format!(
                "osascript couldn't read {}'s window: {}",
                window.app,
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        let value: Value = serde_json::from_slice(&out.stdout)
            .map_err(|e| format!("couldn't read osascript's answer: {e}"))?;
        match value["error"].as_str() {
            Some("accessibility") => Err("macOS hasn't given OpenAgora Accessibility permission, which reading other apps' text needs. Allow OpenAgora in System Settings → Privacy & Security → Accessibility.".into()),
            Some("no-window") => Err(format!("{} has no window to read", window.app)),
            Some(other) => Err(other.to_string()),
            None => Ok(Read {
                title: value["title"].as_str().map(str::to_string),
                text: value["text"].as_str().unwrap_or_default().to_string(),
            }),
        }
    }
}

#[cfg(windows)]
mod os {
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    use std::time::Duration;

    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD as BASE64;
    use serde_json::Value;

    use super::{MAX_CHARS, MAX_NODES, Read};
    use crate::focus::Window;

    /// UI Automation from the window down: a document's text, an edit
    /// field's value, and the names of text, buttons, links and the like.
    /// Off-screen parts are skipped.
    const SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$A = [System.Windows.Automation.AutomationElement]
$T = [System.Windows.Automation.ControlType]
$maxNodes = [int]$env:OPENAGORA_MAX_NODES
$maxChars = [int]$env:OPENAGORA_MAX_CHARS
$root = $A::FromHandle([IntPtr]::new([Int64]$env:OPENAGORA_HWND))
$walker = [System.Windows.Automation.TreeWalker]::ControlViewWalker
$watch = [System.Diagnostics.Stopwatch]::StartNew()
$lines = New-Object System.Collections.Generic.List[string]
$script:nodes = 0
$script:chars = 0
$named = @($T::Text, $T::Button, $T::Hyperlink, $T::ListItem, $T::TabItem, $T::MenuItem, $T::Header, $T::HeaderItem, $T::DataItem, $T::TreeItem, $T::CheckBox, $T::RadioButton, $T::TitleBar)
function Add-Text([string]$text) {
  if ($text -and $text.Trim()) { $lines.Add($text); $script:chars += $text.Length }
}
function Walk($el, [int]$depth) {
  if ($script:nodes -ge $maxNodes -or $script:chars -ge $maxChars -or $depth -gt 60 -or $watch.ElapsedMilliseconds -gt 9000) { return }
  $script:nodes++
  try { $cur = $el.Current } catch { return }
  if ($depth -gt 0 -and $cur.IsOffscreen) { return }
  $type = $cur.ControlType
  $pattern = $null
  if ($type -eq $T::Document -and $el.TryGetCurrentPattern([System.Windows.Automation.TextPattern]::Pattern, [ref]$pattern)) {
    Add-Text ($pattern.DocumentRange.GetText($maxChars)); return
  }
  if ($type -eq $T::Edit -and $el.TryGetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern, [ref]$pattern)) {
    Add-Text $pattern.Current.Value; return
  }
  if ($named -contains $type) { Add-Text $cur.Name }
  $child = $walker.GetFirstChild($el)
  while ($child -ne $null) { Walk $child ($depth + 1); $child = $walker.GetNextSibling($child) }
}
$title = $root.Current.Name
Walk $root 0
@{ title = $title; text = ($lines -join "`n") } | ConvertTo-Json -Compress
"#;

    /// Windows' flag for a console program started without a console window.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    pub fn read(window: &Window) -> Result<Read, String> {
        // -EncodedCommand takes the script as base64 of UTF-16LE.
        let utf16: Vec<u8> = SCRIPT.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let mut cmd = Command::new("powershell.exe");
        cmd.args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-EncodedCommand",
        ])
        .arg(BASE64.encode(utf16))
        .env("OPENAGORA_HWND", window.id.to_string())
        .env("OPENAGORA_MAX_NODES", MAX_NODES.to_string())
        .env("OPENAGORA_MAX_CHARS", MAX_CHARS.to_string())
        .creation_flags(CREATE_NO_WINDOW);
        let out = crate::process::output_within(&mut cmd, Duration::from_secs(20))
            .map_err(|e| format!("couldn't run PowerShell: {e:#}"))?;
        if !out.status.success() {
            return Err(format!(
                "UI Automation couldn't read {}'s window: {}",
                window.app,
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        let text = String::from_utf8_lossy(&out.stdout);
        let value: Value = serde_json::from_str(text.trim().trim_start_matches('\u{feff}'))
            .map_err(|e| format!("couldn't read UI Automation's answer: {e}"))?;
        Ok(Read {
            title: value["title"].as_str().map(str::to_string),
            text: value["text"].as_str().unwrap_or_default().to_string(),
        })
    }
}

#[cfg(target_os = "linux")]
mod os {
    use std::time::{Duration, Instant};

    use serde::Serialize;
    use serde::de::DeserializeOwned;
    use zbus::blocking::Connection;
    use zbus::zvariant::{DynamicType, OwnedObjectPath, OwnedValue, Type};

    use super::{MAX_CHARS, MAX_NODES, Read};
    use crate::focus::Window;

    const ACCESSIBLE: &str = "org.a11y.atspi.Accessible";
    const TEXT: &str = "org.a11y.atspi.Text";
    /// AT-SPI's state bits (the first word of a state set).
    const ACTIVE: u32 = 1 << 1;
    const SHOWING: u32 = 1 << 25;
    /// How long one read may take, and one call to an app.
    const WITHIN: Duration = Duration::from_secs(8);
    const CALL_WITHIN: Duration = Duration::from_secs(2);

    /// An accessible object: the app's bus name and the object's path.
    type Object = (String, OwnedObjectPath);

    struct Bus {
        connection: Connection,
        deadline: Instant,
    }

    impl Bus {
        fn call<B, R>(
            &self,
            object: &Object,
            interface: &str,
            method: &str,
            body: &B,
        ) -> Result<R, String>
        where
            B: Serialize + DynamicType,
            R: DeserializeOwned + Type,
        {
            if Instant::now() > self.deadline {
                return Err("reading the window took too long".into());
            }
            let reply = self
                .connection
                .call_method(
                    Some(object.0.as_str()),
                    object.1.as_str(),
                    Some(interface),
                    method,
                    body,
                )
                .map_err(|e| e.to_string())?;
            reply.body().deserialize::<R>().map_err(|e| e.to_string())
        }

        fn children(&self, object: &Object) -> Vec<Object> {
            self.call(object, ACCESSIBLE, "GetChildren", &())
                .unwrap_or_default()
        }

        fn states(&self, object: &Object) -> u32 {
            self.call::<_, Vec<u32>>(object, ACCESSIBLE, "GetState", &())
                .ok()
                .and_then(|words| words.first().copied())
                .unwrap_or(0)
        }

        fn name(&self, object: &Object) -> String {
            self.call::<_, OwnedValue>(
                object,
                "org.freedesktop.DBus.Properties",
                "Get",
                &(ACCESSIBLE, "Name"),
            )
            .ok()
            .and_then(|v| String::try_from(v).ok())
            .unwrap_or_default()
        }

        fn interfaces(&self, object: &Object) -> Vec<String> {
            self.call(object, ACCESSIBLE, "GetInterfaces", &())
                .unwrap_or_default()
        }

        fn text(&self, object: &Object) -> String {
            self.call(object, TEXT, "GetText", &(0i32, -1i32))
                .unwrap_or_default()
        }

        /// The process behind an app's connection to the accessibility bus.
        fn pid(&self, name: &str) -> Option<u32> {
            let bus: Object = (
                "org.freedesktop.DBus".into(),
                OwnedObjectPath::try_from("/org/freedesktop/DBus").ok()?,
            );
            self.call(
                &bus,
                "org.freedesktop.DBus",
                "GetConnectionUnixProcessID",
                &(name,),
            )
            .ok()
        }
    }

    /// The accessibility bus: its own D-Bus, whose address the session bus
    /// gives (or AT_SPI_BUS_ADDRESS).
    fn connect() -> Result<Connection, String> {
        let address = match std::env::var("AT_SPI_BUS_ADDRESS") {
            Ok(address) if !address.is_empty() => address,
            _ => {
                let session = Connection::session().map_err(|e| {
                    format!("there's no D-Bus session to find the accessibility bus on: {e}")
                })?;
                let reply = session
                    .call_method(
                        Some("org.a11y.Bus"),
                        "/org/a11y/bus",
                        Some("org.a11y.Bus"),
                        "GetAddress",
                        &(),
                    )
                    .map_err(|e| {
                        format!("the system's accessibility bus (AT-SPI) isn't running: {e}")
                    })?;
                reply
                    .body()
                    .deserialize::<String>()
                    .map_err(|e| e.to_string())?
            }
        };
        zbus::blocking::connection::Builder::address(address.as_str())
            .map_err(|e| e.to_string())?
            .method_timeout(CALL_WITHIN)
            .build()
            .map_err(|e| format!("couldn't connect to the accessibility bus: {e}"))
    }

    pub fn read(window: &Window) -> Result<Read, String> {
        let bus = Bus {
            connection: connect()?,
            deadline: Instant::now() + WITHIN,
        };
        let root: Object = (
            "org.a11y.atspi.Registry".into(),
            OwnedObjectPath::try_from("/org/a11y/atspi/accessible/root")
                .map_err(|e| e.to_string())?,
        );
        // The app's windows: the registry's children are apps, and theirs
        // are windows.
        let frames: Vec<Object> = bus
            .children(&root)
            .into_iter()
            .filter(|app| bus.pid(&app.0) == Some(window.pid))
            .flat_map(|app| bus.children(&app))
            .collect();
        if frames.is_empty() {
            return Err(format!(
                "{} doesn't share its text through the system's accessibility support (AT-SPI)",
                window.app
            ));
        }
        // The window by its title, else the active one, else the first shown.
        let named: Vec<(Object, String, u32)> = frames
            .into_iter()
            .map(|f| {
                let name = bus.name(&f);
                let states = bus.states(&f);
                (f, name, states)
            })
            .collect();
        let wanted = window.title.as_deref().filter(|t| !t.is_empty());
        let frame = named
            .iter()
            .find(|(_, name, _)| Some(name.as_str()) == wanted)
            .or_else(|| named.iter().find(|(_, _, states)| states & ACTIVE != 0))
            .or_else(|| named.iter().find(|(_, _, states)| states & SHOWING != 0))
            .ok_or_else(|| format!("{} shows no window to read", window.app))?;
        let mut lines = Vec::new();
        let mut walk = Walk {
            bus: &bus,
            nodes: 0,
            chars: 0,
            lines: &mut lines,
        };
        walk.visit(&frame.0, 0, true);
        Ok(Read {
            title: Some(frame.1.clone()),
            text: lines.join("\n"),
        })
    }

    struct Walk<'a> {
        bus: &'a Bus,
        nodes: usize,
        chars: usize,
        lines: &'a mut Vec<String>,
    }

    impl Walk<'_> {
        fn visit(&mut self, object: &Object, depth: usize, root: bool) {
            if self.nodes >= MAX_NODES
                || self.chars >= MAX_CHARS
                || depth > 60
                || Instant::now() > self.bus.deadline
            {
                return;
            }
            self.nodes += 1;
            if !root && self.bus.states(object) & SHOWING == 0 {
                return;
            }
            let children = self.bus.children(object);
            let text = if self.bus.interfaces(object).iter().any(|i| i == TEXT) {
                self.bus.text(object)
            } else if children.is_empty() {
                self.bus.name(object)
            } else {
                String::new()
            };
            if !text.trim().is_empty() {
                self.chars += text.len();
                self.lines.push(text);
            }
            for child in &children {
                self.visit(child, depth + 1, false);
            }
        }
    }
}

#[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
mod os {
    use super::Read;
    use crate::focus::Window;

    pub fn read(_window: &Window) -> Result<Read, String> {
        Err("OpenAgora can't read other apps' text on this system".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tidy_drops_blank_and_repeated_lines_and_stops_at_the_limit() {
        assert_eq!(
            tidy("  Docs \n\n\nDocs\nRead me\n Docs"),
            "Docs\nRead me\nDocs"
        );
        let long = "é".repeat(MAX_CHARS + 10);
        assert_eq!(tidy(&long).chars().count(), MAX_CHARS);
    }

    #[test]
    fn a_file_stands_in_for_the_os() {
        let dir = std::env::temp_dir().join(format!("oa-screen-text-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("text.json");
        std::fs::write(
            &file,
            r#"{"31": {"title": "Docs", "text": "Hello\n\nHello\nworld"}}"#,
        )
        .unwrap();
        let window = Window {
            id: 31,
            pid: 7,
            app: "Firefox".into(),
            key: "firefox".into(),
            title: Some("Docs".into()),
            exe: None,
        };
        assert_eq!(
            from_file(&file, &window),
            Ok(Read {
                title: Some("Docs".into()),
                text: "Hello\nworld".into()
            })
        );
        let other = Window {
            id: 32,
            ..window.clone()
        };
        assert!(
            from_file(&file, &other)
                .unwrap_err()
                .contains("shares no text")
        );
        std::fs::write(&file, r#"{"unavailable": "no accessibility bus"}"#).unwrap();
        assert_eq!(
            from_file(&file, &window),
            Err("no accessibility bus".into())
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
