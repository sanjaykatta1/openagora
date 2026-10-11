//! Which windows are in front, system-wide. OpenAgora asks the OS only when it
//! needs to know: when the user presses Sunday's shortcut, when an agent asks
//! for a capture, and before each of Sunday's replies. Nothing is tracked in
//! the background and nothing here is written to disk.
//!
//! - macOS: the on-screen window list, front to back, from a small JavaScript
//!   for Automation script run with `osascript`. App names need no permission;
//!   window titles need Screen Recording permission, which capture needs too.
//! - Windows: the foreground window, then the other top-level windows in
//!   z-order, from the Windows API.
//! - Linux on X11: the window manager's `_NET_ACTIVE_WINDOW` and
//!   `_NET_CLIENT_LIST_STACKING`. Wayland doesn't let apps see other apps'
//!   windows, so there it is unavailable.
//!
//! `OPENAGORA_FOCUS_FILE` replaces the OS with a JSON file, for tests: a list
//! of windows, or `{"unavailable": "why", "pick": true|false}`.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// One top-level window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Window {
    /// The OS's id for the window: a CGWindowID on macOS, an HWND on Windows,
    /// an X11 window id on Linux. Electron's capture source for it is
    /// `window:<id>:0`.
    pub id: u64,
    /// The process that owns the window.
    pub pid: u32,
    /// The app's name, for people: "Google Chrome".
    pub app: String,
    /// A stable id for the app: the bundle id on macOS, the executable's file
    /// name on Windows, the window class on Linux.
    pub key: String,
    /// The window's title, when the OS shares it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The app's executable, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exe: Option<String>,
}

/// How many windows to look at, front to back.
const MAX_WINDOWS: usize = 16;

/// The windows on screen, front to back: the focused one first. An error says
/// why the OS couldn't tell.
pub fn windows() -> Result<Vec<Window>, String> {
    if let Some(file) = std::env::var_os("OPENAGORA_FOCUS_FILE").filter(|v| !v.is_empty()) {
        return from_file(Path::new(&file));
    }
    let mut windows = os::windows()?;
    windows.truncate(MAX_WINDOWS);
    Ok(windows)
}

/// Whether the system lets the user pick a window in its own dialog, which is
/// how OpenAgora captures where it can't see which window is in front:
/// Wayland's screen-cast portal. Anywhere else a "pick" would be a guess, so
/// it isn't offered.
pub fn can_pick() -> bool {
    if let Some(file) = std::env::var_os("OPENAGORA_FOCUS_FILE").filter(|v| !v.is_empty()) {
        return std::fs::read_to_string(&file)
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .is_some_and(|value| value["pick"] == true);
    }
    cfg!(target_os = "linux") && wayland()
}

/// A Wayland session, where other apps' windows are out of sight.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn wayland() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some_and(|v| !v.is_empty())
        || std::env::var("XDG_SESSION_TYPE").is_ok_and(|v| v == "wayland")
}

fn from_file(path: &Path) -> Result<Vec<Window>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    if let Some(reason) = value["unavailable"].as_str() {
        return Err(reason.to_string());
    }
    parse(&value.to_string())
}

/// Reads a window list as JSON (what the macOS script prints).
fn parse(text: &str) -> Result<Vec<Window>, String> {
    serde_json::from_str(text.trim()).map_err(|e| format!("couldn't read the window list: {e}"))
}

/// The file name of an executable path, lowercased: `chrome.exe`.
#[cfg(any(windows, target_os = "linux", test))]
fn file_name(path: &str) -> Option<String> {
    path.rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
        .map(str::to_lowercase)
}

#[cfg(target_os = "macos")]
mod os {
    use std::process::Command;
    use std::time::Duration;

    use super::Window;

    /// Lists layer-0 (normal) on-screen windows, front to back, with each
    /// owner's name, bundle id and executable from NSRunningApplication.
    const SCRIPT: &str = r#"
ObjC.import('AppKit');
ObjC.import('CoreGraphics');
function text(value) {
  try { var s = ObjC.unwrap(value); return typeof s === 'string' ? s : ''; } catch (e) { return ''; }
}
function run() {
  // kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements, kCGNullWindowID
  var list = ObjC.deepUnwrap(ObjC.castRefToObject($.CGWindowListCopyWindowInfo(17, 0))) || [];
  var apps = {};
  var out = [];
  for (var i = 0; i < list.length && out.length < 16; i++) {
    var w = list[i];
    if (w.kCGWindowLayer !== 0) continue;
    if (w.kCGWindowAlpha !== undefined && w.kCGWindowAlpha <= 0) continue;
    var b = w.kCGWindowBounds || {};
    if ((b.Width || 0) < 40 || (b.Height || 0) < 40) continue;
    var pid = w.kCGWindowOwnerPID;
    if (!(pid in apps)) {
      var app = $.NSRunningApplication.runningApplicationWithProcessIdentifier(pid);
      apps[pid] = app.isNil() ? {} : {
        name: text(app.localizedName),
        key: text(app.bundleIdentifier),
        exe: app.executableURL.isNil() ? '' : text(app.executableURL.path)
      };
    }
    var a = apps[pid];
    var owner = w.kCGWindowOwnerName || '';
    out.push({
      id: w.kCGWindowNumber,
      pid: pid,
      app: a.name || owner,
      key: a.key || owner,
      title: typeof w.kCGWindowName === 'string' ? w.kCGWindowName : null,
      exe: a.exe || null
    });
  }
  return JSON.stringify(out);
}
"#;

    pub fn windows() -> Result<Vec<Window>, String> {
        let mut cmd = Command::new("/usr/bin/osascript");
        cmd.args(["-l", "JavaScript", "-e", SCRIPT]);
        let out = crate::process::output_within(&mut cmd, Duration::from_secs(10))
            .map_err(|e| format!("couldn't run osascript: {e:#}"))?;
        if !out.status.success() {
            return Err(format!(
                "osascript couldn't list windows: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        super::parse(&String::from_utf8_lossy(&out.stdout))
    }
}

#[cfg(windows)]
mod os {
    use std::collections::HashMap;
    use std::ffi::c_void;

    use windows_sys::Win32::Foundation::{CloseHandle, HWND, LPARAM};
    use windows_sys::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumChildWindows, EnumWindows, GW_OWNER, GWL_EXSTYLE, GetClassNameW, GetForegroundWindow,
        GetWindow, GetWindowLongW, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
        IsIconic, IsWindowVisible, WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    };
    use windows_sys::core::BOOL;

    use super::Window;

    unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
        // SAFETY: `lparam` is the Vec that `windows` passes to EnumWindows,
        // which calls this only while EnumWindows runs.
        let handles = unsafe { &mut *(lparam as *mut Vec<HWND>) };
        handles.push(hwnd);
        1
    }

    pub fn windows() -> Result<Vec<Window>, String> {
        let mut handles: Vec<HWND> = Vec::new();
        // SAFETY: the callback only pushes to `handles`, which outlives the call.
        unsafe { EnumWindows(Some(collect), &mut handles as *mut Vec<HWND> as LPARAM) };
        // SAFETY: no arguments.
        let foreground = unsafe { GetForegroundWindow() };
        // The focused window first, then the rest front to back (EnumWindows
        // goes in z-order).
        let mut order = Vec::with_capacity(handles.len() + 1);
        if !foreground.is_null() {
            order.push(foreground);
        }
        order.extend(handles.into_iter().filter(|h| *h != foreground));
        let mut names: HashMap<String, String> = HashMap::new();
        let mut out = Vec::new();
        for hwnd in order {
            if out.len() >= super::MAX_WINDOWS {
                break;
            }
            if !is_app_window(hwnd) {
                continue;
            }
            let mut pid = 0u32;
            // SAFETY: `pid` is a valid out pointer.
            unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
            let mut exe = process_path(pid);
            // A Store app's window belongs to ApplicationFrameHost.exe, which
            // frames every Store app; the app is the process behind it. A
            // frame whose app can't be found stays ApplicationFrameHost.exe,
            // which the never-capture list covers: it could be any Store app.
            if exe.as_deref().and_then(super::file_name).as_deref()
                == Some("applicationframehost.exe")
            {
                if let Some(app_pid) = hosted_app(hwnd, pid) {
                    pid = app_pid;
                    exe = process_path(pid);
                }
            }
            let key = exe
                .as_deref()
                .and_then(super::file_name)
                .unwrap_or_else(|| format!("pid:{pid}"));
            let app = match &exe {
                Some(path) => names
                    .entry(path.clone())
                    .or_insert_with(|| {
                        file_description(path)
                            .unwrap_or_else(|| key.trim_end_matches(".exe").to_string())
                    })
                    .clone(),
                None => key.clone(),
            };
            out.push(Window {
                id: hwnd as usize as u64,
                pid,
                app,
                key,
                title: window_text(hwnd),
                exe,
            });
        }
        Ok(out)
    }

    /// The process of the app inside a Store app's frame window: the owner of
    /// a child window that isn't the frame's own.
    fn hosted_app(frame: HWND, frame_pid: u32) -> Option<u32> {
        struct Search {
            frame_pid: u32,
            found: u32,
        }
        unsafe extern "system" fn child(hwnd: HWND, lparam: LPARAM) -> BOOL {
            // SAFETY: `lparam` is the Search that `hosted_app` passes to
            // EnumChildWindows, which calls this only while it runs.
            let search = unsafe { &mut *(lparam as *mut Search) };
            let mut pid = 0u32;
            // SAFETY: `pid` is a valid out pointer.
            unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
            if pid != 0 && pid != search.frame_pid {
                search.found = pid;
                return 0;
            }
            1
        }
        let mut search = Search {
            frame_pid,
            found: 0,
        };
        // SAFETY: the callback only writes to `search`, which outlives the call.
        unsafe { EnumChildWindows(frame, Some(child), &mut search as *mut Search as LPARAM) };
        (search.found != 0).then_some(search.found)
    }

    /// A window the user would see in Alt+Tab.
    fn is_app_window(hwnd: HWND) -> bool {
        // SAFETY: plain queries on a window handle; a handle that went away
        // just makes them fail.
        unsafe {
            if IsWindowVisible(hwnd) == 0 || IsIconic(hwnd) != 0 {
                return false;
            }
            let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
            let app_window = ex & WS_EX_APPWINDOW != 0;
            if !app_window
                && (ex & (WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE) != 0
                    || !GetWindow(hwnd, GW_OWNER).is_null())
            {
                return false;
            }
            let mut cloaked: u32 = 0;
            let size = std::mem::size_of::<u32>() as u32;
            if DwmGetWindowAttribute(
                hwnd,
                DWMWA_CLOAKED as u32,
                &mut cloaked as *mut u32 as *mut c_void,
                size,
            ) == 0
                && cloaked != 0
            {
                return false;
            }
            let mut class = [0u16; 64];
            let n = GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32);
            let class = String::from_utf16_lossy(&class[..n.max(0) as usize]);
            if matches!(
                class.as_str(),
                "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
            ) {
                return false;
            }
            GetWindowTextLengthW(hwnd) > 0
        }
    }

    fn window_text(hwnd: HWND) -> Option<String> {
        // SAFETY: the buffer is as long as we say it is.
        unsafe {
            let len = GetWindowTextLengthW(hwnd);
            if len <= 0 {
                return None;
            }
            let mut buffer = vec![0u16; len as usize + 1];
            let n = GetWindowTextW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32);
            (n > 0).then(|| String::from_utf16_lossy(&buffer[..n as usize]))
        }
    }

    fn process_path(pid: u32) -> Option<String> {
        // SAFETY: the handle is closed before returning; the buffer length is
        // passed in and updated by the call.
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                return None;
            }
            let mut buffer = vec![0u16; 1024];
            let mut len = buffer.len() as u32;
            let ok = QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_WIN32,
                buffer.as_mut_ptr(),
                &mut len,
            );
            CloseHandle(handle);
            (ok != 0).then(|| String::from_utf16_lossy(&buffer[..len as usize]))
        }
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain([0]).collect()
    }

    /// The executable's own description ("Google Chrome"), from its version
    /// resource.
    fn file_description(path: &str) -> Option<String> {
        let file = wide(path);
        // SAFETY: every buffer is sized as the API asks; pointers returned by
        // VerQueryValueW point into `data`, which lives until we return.
        unsafe {
            let mut ignored = 0u32;
            let size = GetFileVersionInfoSizeW(file.as_ptr(), &mut ignored);
            if size == 0 {
                return None;
            }
            let mut data = vec![0u8; size as usize];
            if GetFileVersionInfoW(file.as_ptr(), 0, size, data.as_mut_ptr() as *mut c_void) == 0 {
                return None;
            }
            let block = data.as_ptr() as *const c_void;
            let mut value: *mut c_void = std::ptr::null_mut();
            let mut len = 0u32;
            let translation = wide("\\VarFileInfo\\Translation");
            let (language, code_page) =
                if VerQueryValueW(block, translation.as_ptr(), &mut value, &mut len) != 0
                    && len >= 4
                {
                    let pair = value as *const u16;
                    (pair.read_unaligned(), pair.add(1).read_unaligned())
                } else {
                    (0x0409, 0x04b0)
                };
            let query = wide(&format!(
                "\\StringFileInfo\\{language:04x}{code_page:04x}\\FileDescription"
            ));
            if VerQueryValueW(block, query.as_ptr(), &mut value, &mut len) == 0 || len == 0 {
                return None;
            }
            let chars = std::slice::from_raw_parts(value as *const u16, len as usize);
            let text = String::from_utf16_lossy(chars);
            let text = text.trim_end_matches('\0').trim();
            (!text.is_empty()).then(|| text.to_string())
        }
    }
}

#[cfg(target_os = "linux")]
mod os {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::Mutex;

    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{Atom, AtomEnum, ConnectionExt, Window as XWindow};
    use x11rb::rust_connection::RustConnection;

    use super::Window;

    pub fn windows() -> Result<Vec<Window>, String> {
        if super::wayland() {
            return Err("Wayland doesn't let apps see which window is in front".into());
        }
        let (conn, screen) =
            x11rb::connect(None).map_err(|e| format!("couldn't reach the X server: {e}"))?;
        let root = conn.setup().roots[screen].root;
        let atoms = Atoms::new(&conn)?;
        let active = cardinals(
            &conn,
            root,
            atoms.net_active_window,
            AtomEnum::WINDOW.into(),
        )
        .first()
        .copied()
        .filter(|w| *w != 0);
        let mut stacking = cardinals(
            &conn,
            root,
            atoms.net_client_list_stacking,
            AtomEnum::WINDOW.into(),
        );
        if active.is_none() && stacking.is_empty() {
            return Err("the window manager doesn't say which window is in front".into());
        }
        // Stacking order is bottom to top.
        stacking.reverse();
        let mut order: Vec<XWindow> = active.into_iter().collect();
        order.extend(stacking.into_iter().filter(|w| Some(*w) != active));
        let mut out = Vec::new();
        for window in order {
            if out.len() >= super::MAX_WINDOWS {
                break;
            }
            if let Some(found) = describe(&conn, &atoms, window) {
                out.push(found);
            }
        }
        Ok(out)
    }

    struct Atoms {
        net_active_window: Atom,
        net_client_list_stacking: Atom,
        net_wm_name: Atom,
        net_wm_pid: Atom,
        net_wm_state: Atom,
        net_wm_state_hidden: Atom,
        net_wm_window_type: Atom,
        net_wm_window_type_dock: Atom,
        net_wm_window_type_desktop: Atom,
        utf8_string: Atom,
    }

    impl Atoms {
        fn new(conn: &RustConnection) -> Result<Atoms, String> {
            let atom = |name: &[u8]| -> Result<Atom, String> {
                conn.intern_atom(false, name)
                    .map_err(|e| e.to_string())?
                    .reply()
                    .map(|r| r.atom)
                    .map_err(|e| e.to_string())
            };
            Ok(Atoms {
                net_active_window: atom(b"_NET_ACTIVE_WINDOW")?,
                net_client_list_stacking: atom(b"_NET_CLIENT_LIST_STACKING")?,
                net_wm_name: atom(b"_NET_WM_NAME")?,
                net_wm_pid: atom(b"_NET_WM_PID")?,
                net_wm_state: atom(b"_NET_WM_STATE")?,
                net_wm_state_hidden: atom(b"_NET_WM_STATE_HIDDEN")?,
                net_wm_window_type: atom(b"_NET_WM_WINDOW_TYPE")?,
                net_wm_window_type_dock: atom(b"_NET_WM_WINDOW_TYPE_DOCK")?,
                net_wm_window_type_desktop: atom(b"_NET_WM_WINDOW_TYPE_DESKTOP")?,
                utf8_string: atom(b"UTF8_STRING")?,
            })
        }
    }

    /// A 32-bit list property; empty when it isn't set or can't be read.
    fn cardinals(conn: &RustConnection, window: XWindow, property: Atom, kind: Atom) -> Vec<u32> {
        conn.get_property(false, window, property, kind, 0, 4096)
            .ok()
            .and_then(|cookie| cookie.reply().ok())
            .and_then(|reply| reply.value32().map(|values| values.collect()))
            .unwrap_or_default()
    }

    fn bytes(
        conn: &RustConnection,
        window: XWindow,
        property: Atom,
        kind: Atom,
    ) -> Option<Vec<u8>> {
        let reply = conn
            .get_property(false, window, property, kind, 0, 4096)
            .ok()?
            .reply()
            .ok()?;
        (reply.format == 8 && !reply.value.is_empty()).then_some(reply.value)
    }

    fn describe(conn: &RustConnection, atoms: &Atoms, window: XWindow) -> Option<Window> {
        let types = cardinals(
            conn,
            window,
            atoms.net_wm_window_type,
            AtomEnum::ATOM.into(),
        );
        if types
            .iter()
            .any(|t| *t == atoms.net_wm_window_type_dock || *t == atoms.net_wm_window_type_desktop)
        {
            return None;
        }
        let states = cardinals(conn, window, atoms.net_wm_state, AtomEnum::ATOM.into());
        if states.contains(&atoms.net_wm_state_hidden) {
            return None;
        }
        // WM_CLASS is "instance\0class\0".
        let class = bytes(
            conn,
            window,
            AtomEnum::WM_CLASS.into(),
            AtomEnum::STRING.into(),
        )
        .map(|raw| {
            let text = String::from_utf8_lossy(&raw).into_owned();
            let mut parts = text.split('\0').filter(|p| !p.is_empty());
            let instance = parts.next().unwrap_or_default().to_string();
            parts.next().map(str::to_string).unwrap_or(instance)
        })
        .filter(|c| !c.is_empty());
        let title = bytes(conn, window, atoms.net_wm_name, atoms.utf8_string)
            .map(|raw| String::from_utf8_lossy(&raw).into_owned())
            .or_else(|| {
                bytes(
                    conn,
                    window,
                    AtomEnum::WM_NAME.into(),
                    AtomEnum::STRING.into(),
                )
                .map(|raw| raw.iter().map(|b| *b as char).collect())
            });
        let pid = cardinals(conn, window, atoms.net_wm_pid, AtomEnum::CARDINAL.into())
            .first()
            .copied()
            .unwrap_or(0);
        let exe = (pid != 0)
            .then(|| std::fs::read_link(format!("/proc/{pid}/exe")).ok())
            .flatten()
            .map(|p| p.display().to_string());
        let key = class
            .as_deref()
            .map(str::to_lowercase)
            .or_else(|| exe.as_deref().and_then(super::file_name))
            .unwrap_or_else(|| format!("x11:{window}"));
        let app = class
            .as_deref()
            .and_then(desktop_name)
            .or(class.clone())
            .unwrap_or_else(|| key.clone());
        Some(Window {
            id: window as u64,
            pid,
            app,
            key,
            title,
            exe,
        })
    }

    /// The app's name from its desktop entry, found by window class.
    fn desktop_name(class: &str) -> Option<String> {
        static CACHE: Mutex<Option<HashMap<String, Option<String>>>> = Mutex::new(None);
        let wanted = class.to_lowercase();
        let mut cache = CACHE.lock().unwrap();
        let cache = cache.get_or_insert_with(HashMap::new);
        if let Some(found) = cache.get(&wanted) {
            return found.clone();
        }
        let found = find_desktop_name(&wanted);
        cache.insert(wanted, found.clone());
        found
    }

    fn application_dirs() -> Vec<PathBuf> {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let mut dirs = Vec::new();
        match std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
            Some(data) => dirs.push(PathBuf::from(data).join("applications")),
            None => dirs.extend(home.iter().map(|h| h.join(".local/share/applications"))),
        }
        let system = std::env::var("XDG_DATA_DIRS")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
        dirs.extend(
            system
                .split(':')
                .map(|d| PathBuf::from(d).join("applications")),
        );
        dirs.extend(
            home.iter()
                .map(|h| h.join(".local/share/flatpak/exports/share/applications")),
        );
        dirs.push("/var/lib/flatpak/exports/share/applications".into());
        dirs.push("/var/lib/snapd/desktop/applications".into());
        dirs
    }

    fn find_desktop_name(class: &str) -> Option<String> {
        let mut by_file = None;
        for dir in application_dirs() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_none_or(|e| e != "desktop") {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                let (name, wm_class) = desktop_entry(&text);
                if wm_class.is_some_and(|c| c.to_lowercase() == class) {
                    return name;
                }
                let stem = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                if by_file.is_none() && (stem == class || stem.ends_with(&format!(".{class}"))) {
                    by_file = name;
                }
            }
        }
        by_file
    }

    /// `Name=` and `StartupWMClass=` of a desktop file's main group.
    fn desktop_entry(text: &str) -> (Option<String>, Option<String>) {
        let mut in_entry = false;
        let (mut name, mut class) = (None, None);
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('[') {
                in_entry = line == "[Desktop Entry]";
                continue;
            }
            if !in_entry {
                continue;
            }
            if let Some(v) = line.strip_prefix("Name=") {
                name.get_or_insert_with(|| v.trim().to_string());
            } else if let Some(v) = line.strip_prefix("StartupWMClass=") {
                class.get_or_insert_with(|| v.trim().to_string());
            }
        }
        (name, class)
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn reads_desktop_entries() {
            let (name, class) = super::desktop_entry(
                "[Desktop Entry]\nName=Firefox Web Browser\nName[de]=Firefox\nStartupWMClass=firefox\n[Desktop Action new]\nName=New Window\n",
            );
            assert_eq!(name.as_deref(), Some("Firefox Web Browser"));
            assert_eq!(class.as_deref(), Some("firefox"));
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
mod os {
    pub fn windows() -> Result<Vec<super::Window>, String> {
        Err("OpenAgora can't see windows on this OS".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_macos_script_output() {
        let windows = parse(
            r#"[{"id":4211,"pid":801,"app":"Google Chrome","key":"com.google.Chrome","title":"Docs","exe":"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"},
                {"id":90,"pid":12,"app":"Finder","key":"com.apple.finder","title":null,"exe":null}]"#,
        )
        .unwrap();
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].key, "com.google.Chrome");
        assert_eq!(windows[0].title.as_deref(), Some("Docs"));
        assert_eq!(windows[1].title, None);
        assert!(parse("not json").is_err());
    }

    #[test]
    fn names_executables() {
        assert_eq!(
            file_name(r"C:\Program Files\Google\Chrome\Application\chrome.exe").as_deref(),
            Some("chrome.exe")
        );
        assert_eq!(
            file_name("/usr/lib/firefox/firefox").as_deref(),
            Some("firefox")
        );
        assert_eq!(file_name(""), None);
    }
}
