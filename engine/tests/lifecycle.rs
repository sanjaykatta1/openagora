//! End-to-end tests of install / start / stop / uninstall, driving the real
//! binary against a temporary catalog, data folder and a fake GitHub API.
#![cfg(unix)]

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Arc;
use std::thread;

use openagora::github::sha256_hex;

struct Env {
    _dir: tempfile::TempDir,
    catalog: PathBuf,
    home: PathBuf,
    api: String,
}

/// Serves `routes` (path -> body) over plain HTTP on a free local port.
fn serve(routes: Vec<(String, Vec<u8>)>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let routes = Arc::new(routes);
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let routes = Arc::clone(&routes);
            thread::spawn(move || {
                let mut reader = BufReader::new(&stream);
                let mut request = String::new();
                if reader.read_line(&mut request).is_err() {
                    return;
                }
                let path = request.split_whitespace().nth(1).unwrap_or("").to_string();
                let mut line = String::new();
                while reader.read_line(&mut line).map(|n| n > 2).unwrap_or(false) {
                    line.clear();
                }
                let mut out = &stream;
                match routes.iter().find(|(p, _)| *p == path) {
                    Some((_, body)) => {
                        let _ = write!(
                            out,
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = out.write_all(body);
                    }
                    None => {
                        let _ = write!(
                            out,
                            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        );
                    }
                }
            });
        }
    });
    base
}

/// A catalog of test apps and a fake GitHub release for `dl`.
fn setup(digest_override: Option<&str>) -> Env {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    // The release asset: a tarball containing an executable `tool` script.
    let payload = root.join("payload");
    fs::create_dir_all(&payload).unwrap();
    fs::write(payload.join("tool"), "#!/bin/sh\necho tool ran\n").unwrap();
    fs::set_permissions(payload.join("tool"), fs::Permissions::from_mode(0o755)).unwrap();
    let tarball = root.join("dl.tar.gz");
    let ok = Command::new("tar")
        .args(["-czf"])
        .arg(&tarball)
        .args(["-C"])
        .arg(&payload)
        .arg("tool")
        .status()
        .unwrap();
    assert!(ok.success());
    let bytes = fs::read(&tarball).unwrap();
    let digest = digest_override
        .map(str::to_string)
        .unwrap_or_else(|| format!("sha256:{}", sha256_hex(&bytes)));

    let api = serve_release(bytes, digest);

    let catalog = root.join("catalog");
    let os = if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    };
    write_app(
        &catalog,
        "dl",
        &format!(
            r#"
schema = "openagora/v1"
id = "dl"
name = "Downloaded tool"
summary = "Comes from a release"
category = "test"
homepage = "https://example.com"
source = "https://github.com/example/dl"
license = "MIT"
platforms = ["{os}"]

[install]
provides = "{{app_dir}}/tool"

[[install.steps]]
os = ["{os}"]
shell = "sh"
download = {{ github = "example/dl", asset = "dl-*.tar.gz" }}
run = "tar -xzf '{{download}}' -C '{{app_dir}}'"

[run]
command = "{{app_dir}}/tool"

[run.health]
process = true

[ui]
kind = "background"
"#
        ),
    );
    write_app(
        &catalog,
        "web",
        &format!(
            r#"
schema = "openagora/v1"
id = "web"
name = "Web demo"
summary = "A tiny web server"
category = "test"
homepage = "https://example.com"
source = "https://github.com/example/web"
license = "MIT"
platforms = ["{os}"]

[install]
provides = "{{app_dir}}/index.html"

[[install.steps]]
os = ["{os}"]
shell = "sh"
run = "echo hello-openagora > '{{app_dir}}/index.html'"

[run]
command = "python3"
args = ["-m", "http.server", "{{port}}", "--bind", "127.0.0.1"]
port = "auto"

[run.health]
tcp = true

[ui]
kind = "web"
url = "http://127.0.0.1:{{port}}/"
"#
        ),
    );
    // A web app with an MCP server of its own, as T3 Code has.
    write_app(
        &catalog,
        "mcpweb",
        &format!(
            r#"
schema = "openagora/v1"
id = "mcpweb"
name = "MCP web demo"
summary = "A tiny web server with an MCP server"
category = "test"
homepage = "https://example.com"
source = "https://github.com/example/mcpweb"
license = "MIT"
platforms = ["{os}"]

[install]
provides = "{{app_dir}}/index.html"

[[install.steps]]
os = ["{os}"]
shell = "sh"
run = "echo hello > '{{app_dir}}/index.html'"

[run]
command = "python3"
args = ["-m", "http.server", "{{port}}", "--bind", "127.0.0.1"]
port = "auto"

[run.health]
tcp = true

[ui]
kind = "web"
url = "http://127.0.0.1:{{port}}/"

[agent.mcp]
url = "http://127.0.0.1:{{port}}/mcp"
auth = "oauth"
"#
        ),
    );
    // Two web apps that print their own address at startup, the way T3 Code
    // prints a one-time pairing link: one local, one pointing elsewhere.
    for (id, printed) in [
        ("paired", "http://127.0.0.1:{{port}}/pair#token=abc123"),
        ("sneaky", "https://example.com/phish"),
    ] {
        write_app(
            &catalog,
            id,
            &format!(
                r#"
schema = "openagora/v1"
id = "{id}"
name = "{id}"
summary = "Prints its address at startup"
category = "test"
homepage = "https://example.com"
source = "https://github.com/example/{id}"
license = "MIT"
platforms = ["{os}"]

[install]
provides = "{{app_dir}}/index.html"

[[install.steps]]
os = ["{os}"]
shell = "sh"
run = "echo hello > '{{app_dir}}/index.html'"

[run]
command = "sh"
args = ["-c", "echo 'Pairing URL: {printed}'; exec python3 -m http.server {{port}} --bind 127.0.0.1"]
port = "auto"

[run.health]
tcp = true

[ui]
kind = "web"
url = "http://127.0.0.1:{{port}}/"
url_from_log_after = "Pairing URL: "
"#
            ),
        );
    }
    Env {
        catalog,
        home: root.join("home"),
        api,
        _dir: dir,
    }
}

/// A fake GitHub API whose latest release of example/dl holds `bytes`, plus a
/// `.sig` decoy the listing's `dl-*.tar.gz` pattern must not match.
fn serve_release(bytes: Vec<u8>, digest: String) -> String {
    let asset_base = serve(vec![("/dl-1.2.3.tar.gz".into(), bytes)]);
    let release = format!(
        r#"{{"tag_name":"v1.2.3","assets":[
            {{"name":"dl-1.2.3.tar.gz","browser_download_url":"{asset_base}/dl-1.2.3.tar.gz","digest":"{digest}"}},
            {{"name":"dl-1.2.3.tar.gz.sig","browser_download_url":"{asset_base}/x","digest":null}}
        ]}}"#
    );
    serve(vec![(
        "/repos/example/dl/releases/latest".into(),
        release.into_bytes(),
    )])
}

fn write_app(catalog: &Path, id: &str, text: &str) {
    let dir = catalog.join("apps").join(id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("app.toml"), text).unwrap();
}

fn run(env: &Env, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_openagora"))
        .arg("--catalog")
        .arg(&env.catalog)
        .args(args)
        .env("OPENAGORA_HOME", &env.home)
        .env("OPENAGORA_GITHUB_API", &env.api)
        .output()
        .unwrap()
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

#[test]
fn install_downloads_verifies_and_records() {
    let env = setup(None);
    let out = run(&env, &["install", "dl", "--yes"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let tool = env.home.join("apps/dl/tool");
    assert!(tool.is_file());
    let state = fs::read_to_string(env.home.join("state/dl.toml")).unwrap();
    assert!(state.contains("dl-1.2.3.tar.gz@v1.2.3"), "{state}");
    assert!(stdout(&run(&env, &["ps"])).contains("dl         stopped"));

    let again = run(&env, &["install", "dl", "--yes"]);
    assert!(!again.status.success());
    assert!(stderr(&again).contains("already installed"));
}

#[test]
fn bad_checksum_installs_nothing() {
    let env = setup(Some(
        "sha256:0000000000000000000000000000000000000000000000000000000000000000",
    ));
    let out = run(&env, &["install", "dl", "--yes"]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("checksum"), "{}", stderr(&out));
    assert!(!env.home.join("apps/dl").exists());
    assert!(!env.home.join("state/dl.toml").exists());
}

#[test]
fn install_needs_confirmation_without_a_terminal() {
    let env = setup(None);
    let out = run(&env, &["install", "dl"]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("--yes"));
    assert!(!env.home.join("apps/dl").exists());
}

#[test]
fn web_app_starts_serves_and_stops() {
    if Command::new("python3").arg("--version").output().is_err() {
        eprintln!("python3 not found; skipping");
        return;
    }
    let env = setup(None);
    assert!(run(&env, &["install", "web", "--yes"]).status.success());

    let started = run(&env, &["start", "web"]);
    assert!(started.status.success(), "{}", stderr(&started));
    let text = stdout(&started);
    let url = text
        .split_whitespace()
        .find(|w| w.starts_with("http://127.0.0.1:"))
        .expect("start prints the URL")
        .to_string();
    let page = Command::new("curl").args(["-fsS", &url]).output().unwrap();
    assert!(stdout(&page).contains("hello-openagora"));
    assert!(stdout(&run(&env, &["ps"])).contains("web        running"));

    // Starting again reuses the running instance.
    assert!(stdout(&run(&env, &["start", "web"])).contains(&url));

    assert!(stdout(&run(&env, &["stop", "web"])).contains("Stopped"));
    assert!(stdout(&run(&env, &["ps"])).contains("web        stopped"));
    let gone = Command::new("curl")
        .args(["-fsS", "-m", "2", &url])
        .output()
        .unwrap();
    assert!(!gone.status.success(), "server still answering after stop");

    assert!(run(&env, &["uninstall", "web", "--yes"]).status.success());
    assert!(!env.home.join("apps/web").exists());
    assert!(stdout(&run(&env, &["ps"])).contains("No apps installed"));
}

#[test]
fn app_that_exits_immediately_fails_to_start() {
    let env = setup(None);
    assert!(run(&env, &["install", "dl", "--yes"]).status.success());
    let out = run(&env, &["start", "dl"]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("exited right after starting"), "{err}");
    assert!(err.contains("tool ran"), "log tail should be shown: {err}");
    assert!(stdout(&run(&env, &["ps"])).contains("dl         stopped"));
}

#[test]
fn web_app_opens_at_the_address_it_prints() {
    if Command::new("python3").arg("--version").output().is_err() {
        eprintln!("python3 not found; skipping");
        return;
    }
    let env = setup(None);
    for id in ["paired", "sneaky"] {
        assert!(run(&env, &["install", id, "--yes"]).status.success());
        let started = run(&env, &["start", id]);
        assert!(started.status.success(), "{}", stderr(&started));
        let text = stdout(&started);
        let url = text
            .split_whitespace()
            .find(|w| w.starts_with("http"))
            .expect("start prints the URL");
        if id == "paired" {
            assert!(
                url.starts_with("http://127.0.0.1:") && url.ends_with("/pair#token=abc123"),
                "{url}"
            );
        } else {
            // A non-local address is ignored; the listing's own URL is used.
            assert!(
                url.starts_with("http://127.0.0.1:") && url.ends_with('/'),
                "{url}"
            );
            assert!(stderr(&started).contains("did not print its address"));
        }
        assert!(run(&env, &["stop", id]).status.success());
    }
}

#[test]
fn an_app_with_an_mcp_server_keeps_its_port() {
    if Command::new("python3").arg("--version").output().is_err() {
        eprintln!("python3 not found; skipping");
        return;
    }
    let env = setup(None);
    assert!(run(&env, &["install", "mcpweb", "--yes"]).status.success());
    let port_of = |out: &Output| -> u16 {
        let text = stdout(out);
        let url = text
            .split_whitespace()
            .find(|w| w.starts_with("http://127.0.0.1:"))
            .expect("start prints the URL");
        url.trim_start_matches("http://127.0.0.1:")
            .trim_end_matches('/')
            .parse()
            .unwrap()
    };
    let first = run(&env, &["start", "mcpweb"]);
    assert!(first.status.success(), "{}", stderr(&first));
    let port = port_of(&first);
    assert!(run(&env, &["stop", "mcpweb"]).status.success());
    // The same port next time: the app's OAuth grants name its address.
    let second = run(&env, &["start", "mcpweb"]);
    assert_eq!(port_of(&second), port);
    assert!(run(&env, &["stop", "mcpweb"]).status.success());
    // Only when something else holds it does the app move.
    let squatter = TcpListener::bind(("127.0.0.1", port)).unwrap();
    let third = run(&env, &["start", "mcpweb"]);
    assert!(third.status.success(), "{}", stderr(&third));
    let moved = port_of(&third);
    assert_ne!(moved, port);
    drop(squatter);
    assert!(run(&env, &["stop", "mcpweb"]).status.success());
    let fourth = run(&env, &["start", "mcpweb"]);
    assert_eq!(port_of(&fourth), moved);
    assert!(run(&env, &["stop", "mcpweb"]).status.success());
}

#[test]
fn native_open_raises_the_existing_window_and_preserves_its_process() {
    let env = setup(None);
    let listing = env.catalog.join("apps/dl/app.toml");
    fs::write(
        &listing,
        fs::read_to_string(&listing)
            .unwrap()
            .replace(
                "command = \"{app_dir}/tool\"",
                "command = \"{app_dir}/tool\"\nargs = [\"--start-hidden\"]",
            )
            .replace(
                "kind = \"background\"",
                "kind = \"background\"\nshow_args = []",
            ),
    )
    .unwrap();
    assert!(run(&env, &["install", "dl", "--yes"]).status.success());
    let dir = env.home.join("apps/dl");
    fs::write(
        dir.join("tool"),
        r#"#!/bin/sh
if [ -f instance ] && kill -0 "$(cat instance)" 2>/dev/null; then
  [ -f fail-open ] && exit 7
  printf 'shown\n' >> windows
  exit 0
fi
printf '%s' "$$" > instance
printf '%s' "$*" > launch-args
trap 'rm -f instance; exit 0' TERM INT
while :; do sleep 1; done
"#,
    )
    .unwrap();
    let opened = run(&env, &["open", "dl"]);
    assert!(opened.status.success(), "{}", stderr(&opened));
    // Even a saved start-hidden preference must be overridden by activation.
    assert_eq!(
        fs::read_to_string(dir.join("launch-args")).unwrap(),
        "--start-hidden"
    );
    let pid = fs::read_to_string(dir.join("instance")).unwrap();
    let opened_again = run(&env, &["open", "dl"]);
    let windows = fs::read_to_string(dir.join("windows")).unwrap();
    let same_pid = fs::read_to_string(dir.join("instance")).unwrap();
    fs::write(dir.join("fail-open"), "").unwrap();
    let rejected = run(&env, &["open", "dl"]);
    let stopped = run(&env, &["stop", "dl"]);
    assert!(stopped.status.success());
    assert!(opened_again.status.success(), "{}", stderr(&opened_again));
    assert_eq!(windows.lines().count(), 2);
    assert_eq!(pid, same_pid);
    assert!(!rejected.status.success());
    assert!(stderr(&rejected).contains("could not show its window"));
}

#[test]
fn missing_managed_app_files_can_be_reinstalled() {
    let env = setup(None);
    assert!(run(&env, &["install", "dl", "--yes"]).status.success());
    fs::remove_dir_all(env.home.join("apps/dl")).unwrap();
    let listing: serde_json::Value =
        serde_json::from_str(&stdout(&run(&env, &["catalog", "--json"]))).unwrap();
    let listing = listing
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == "dl")
        .unwrap();
    assert_eq!(listing["installed"], true);
    assert_eq!(listing["present"], false);
    let reinstalled = run(&env, &["install", "dl", "--yes"]);
    assert!(reinstalled.status.success(), "{}", stderr(&reinstalled));
    assert!(env.home.join("apps/dl/tool").exists());
}

#[test]
fn an_installer_that_exits_successfully_but_leaves_no_app_is_not_recorded() {
    let env = setup(None);
    let listing = env.catalog.join("apps/dl/app.toml");
    let text = fs::read_to_string(&listing).unwrap().replace(
        "provides = \"{app_dir}/tool\"",
        "provides = \"{app_dir}/missing\"",
    );
    fs::write(listing, text).unwrap();
    let out = run(&env, &["install", "dl", "--yes"]);
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("install finished but"),
        "{}",
        stderr(&out)
    );
    assert!(!env.home.join("state/dl.toml").exists());
    assert!(!env.home.join("apps/dl").exists());
}
