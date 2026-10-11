//! Shared by the Sunday tests: a temporary home with a fake Hermes, and a
//! catalog whose agent listing installs that fake instead of the real one.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Output, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};

use openagora::github::sha256_hex;

pub const FAKE_HERMES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fakes/hermes");

pub struct Env {
    _dir: tempfile::TempDir,
    pub catalog: PathBuf,
    /// The user's home folder (HOME).
    pub home: PathBuf,
    /// OpenAgora's data folder (OPENAGORA_HOME).
    pub data: PathBuf,
    /// Every call the fake Hermes received, one per line.
    pub calls: PathBuf,
    extra_env: Vec<(String, String)>,
}

impl Env {
    pub fn new() -> Env {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let home = root.join("home");
        fs::create_dir_all(&home).unwrap();
        let catalog = root.join("catalog");
        write_agent_listing(&catalog);
        Env {
            catalog,
            data: root.join("data"),
            calls: root.join("hermes-calls.log"),
            home,
            extra_env: Vec::new(),
            _dir: dir,
        }
    }

    pub fn hermes_root(&self) -> PathBuf {
        self.home.join(".hermes")
    }

    pub fn sunday_dir(&self) -> PathBuf {
        self.hermes_root().join("profiles/sunday")
    }

    /// Puts the fake Hermes on this computer, as if the user installed it.
    pub fn install_hermes_by_hand(&self) {
        let bin = self.home.join(".local/bin");
        fs::create_dir_all(&bin).unwrap();
        fs::copy(FAKE_HERMES, bin.join("hermes")).unwrap();
        fs::set_permissions(bin.join("hermes"), fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// A Hermes the user has been using: a default profile with a persona,
    /// a Telegram bot and memory, plus a second profile of their own.
    pub fn existing_hermes_setup(&self) {
        let root = self.hermes_root();
        fs::create_dir_all(root.join("memories")).unwrap();
        fs::write(root.join("config.yaml"), "model:\n  default: some/model\n").unwrap();
        fs::write(
            root.join(".env"),
            "OPENROUTER_API_KEY=sk-test\nTELEGRAM_BOT_TOKEN=123:abc\n",
        )
        .unwrap();
        fs::write(
            root.join("SOUL.md"),
            "You are Jarvis. Speak like a butler.\n",
        )
        .unwrap();
        fs::write(root.join("memories/MEMORY.md"), "The user prefers tea.\n").unwrap();
        let work = root.join("profiles/work");
        fs::create_dir_all(&work).unwrap();
        fs::write(work.join("config.yaml"), "model:\n  default: other/model\n").unwrap();
        fs::write(work.join(".env"), "DISCORD_BOT_TOKEN=xyz\n").unwrap();
    }

    pub fn set_hermes_version(&self, version: &str) {
        fs::create_dir_all(self.hermes_root()).unwrap();
        fs::write(self.hermes_root().join(".fake-version"), version).unwrap();
    }

    pub fn env(&mut self, key: &str, value: &str) {
        self.extra_env.push((key.into(), value.into()));
    }

    pub fn run(&self, args: &[&str]) -> Output {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_openagora"));
        cmd.arg("--catalog")
            .arg(&self.catalog)
            .args(args)
            .env("HOME", &self.home)
            .env("OPENAGORA_HOME", &self.data)
            .env("HERMES_CALLS", &self.calls)
            .env("FAKE_HERMES", FAKE_HERMES)
            .env("PATH", "/usr/bin:/bin")
            .env_remove("HERMES_HOME");
        for (k, v) in &self.extra_env {
            cmd.env(k, v);
        }
        cmd.output().unwrap()
    }

    /// Runs and insists on success, returning stdout.
    pub fn ok(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "openagora {args:?} failed:\n{}{}",
            stdout(&out),
            stderr(&out)
        );
        stdout(&out)
    }

    pub fn json(&self, args: &[&str]) -> serde_json::Value {
        serde_json::from_str(&self.ok(args)).unwrap()
    }

    pub fn hermes_calls(&self) -> Vec<String> {
        fs::read_to_string(&self.calls)
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// Every Hermes file outside Sunday's profile, with a hash of its contents.
    pub fn snapshot_outside_sunday(&self) -> BTreeMap<PathBuf, String> {
        let mut files = BTreeMap::new();
        walk(&self.hermes_root(), &self.hermes_root(), &mut files);
        files.retain(|path, _| !path.starts_with("profiles/sunday"));
        files
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        // A test may have started the engine service; don't leave it running.
        let _ = self.run(&["daemon", "stop"]);
    }
}

impl Env {
    /// Adds a small web app (python's http.server) to the catalog.
    pub fn add_web_app(&self, id: &str) {
        let os = this_os();
        write_listing(
            &self.catalog,
            id,
            &format!(
                r#"
schema = "openagora/v1"
id = "{id}"
name = "Web {id}"
summary = "A tiny web server"
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
run = "echo hello-{id} > '{{app_dir}}/index.html'"

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
    }

    /// The engine service's socket, as the running service recorded it.
    pub fn socket(&self) -> PathBuf {
        let record = fs::read_to_string(self.data.join("run/daemon.toml"))
            .expect("the engine service isn't running");
        let record: toml::Value = toml::from_str(&record).unwrap();
        PathBuf::from(record["address"].as_str().unwrap())
    }

    pub fn audit(&self) -> Vec<Value> {
        let out = self.json(&["audit", "--json"]);
        out.as_array().cloned().unwrap_or_default()
    }
}

fn this_os() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

fn write_listing(catalog: &Path, id: &str, text: &str) {
    let dir = catalog.join("apps").join(id);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("app.toml"), text).unwrap();
}

/// An MCP client talking to `openagora mcp --client <id>`, as Hermes would.
pub struct McpClient {
    child: Child,
    stdin: ChildStdin,
    incoming: mpsc::Receiver<Value>,
    next_id: u64,
}

impl McpClient {
    pub fn start(env: &Env, client: &str, elicitation: bool) -> McpClient {
        let mut child = Command::new(env!("CARGO_BIN_EXE_openagora"))
            .arg("--catalog")
            .arg(&env.catalog)
            .args(["mcp", "--client", client])
            .env("HOME", &env.home)
            .env("OPENAGORA_HOME", &env.data)
            .env("HERMES_CALLS", &env.calls)
            .env("PATH", "/usr/bin:/bin")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, incoming) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if let Ok(message) = serde_json::from_str(&line) {
                    if tx.send(message).is_err() {
                        break;
                    }
                }
            }
        });
        let mut mcp = McpClient {
            child,
            stdin,
            incoming,
            next_id: 1,
        };
        let capabilities = if elicitation {
            json!({"elicitation": {}})
        } else {
            json!({})
        };
        let init = mcp.request(
            "initialize",
            json!({"protocolVersion": "2025-06-18", "capabilities": capabilities, "clientInfo": {"name": "test"}}),
            &mut |_| panic!("unexpected server request during initialize"),
        );
        assert_eq!(init["result"]["serverInfo"]["name"], "openagora", "{init}");
        mcp.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
        mcp
    }

    fn send(&mut self, message: &Value) {
        writeln!(self.stdin, "{message}").unwrap();
        self.stdin.flush().unwrap();
    }

    /// Sends a request and waits for its reply. Requests the server sends in
    /// the meantime (elicitation) are answered by `answer`.
    pub fn request(
        &mut self,
        method: &str,
        params: Value,
        answer: &mut dyn FnMut(&Value) -> Value,
    ) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        loop {
            let message = self
                .incoming
                .recv_timeout(Duration::from_secs(60))
                .expect("no reply from openagora mcp");
            if message["id"] == id && message.get("method").is_none() {
                return message;
            }
            if message.get("method").is_some() && message.get("id").is_some() {
                let result = answer(&message);
                // `{"__error": "…"}` answers with a JSON-RPC error instead.
                let reply = match result.get("__error") {
                    Some(error) => {
                        json!({"jsonrpc": "2.0", "id": message["id"], "error": {"code": -32601, "message": error}})
                    }
                    None => json!({"jsonrpc": "2.0", "id": message["id"], "result": result}),
                };
                self.send(&reply);
            }
        }
    }

    pub fn tools(&mut self) -> Vec<String> {
        let reply = self.request("tools/list", json!({}), &mut |_| json!({}));
        reply["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect()
    }

    /// Calls a tool; returns (text, is_error).
    pub fn call(
        &mut self,
        tool: &str,
        arguments: Value,
        answer: &mut dyn FnMut(&Value) -> Value,
    ) -> (String, bool) {
        let reply = self.request(
            "tools/call",
            json!({"name": tool, "arguments": arguments}),
            answer,
        );
        let result = &reply["result"];
        let text = result["content"]
            .as_array()
            .map(|c| {
                c.iter()
                    .filter_map(|b| b["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_else(|| reply.to_string());
        (text, result["isError"] == true)
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<PathBuf, String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(root, &path, out);
        } else {
            let rel = path.strip_prefix(root).unwrap().to_path_buf();
            out.insert(rel, sha256_hex(&fs::read(&path).unwrap()));
        }
    }
}

pub fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

pub fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn write_agent_listing(catalog: &Path) {
    let os = if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    };
    let dir = catalog.join("apps/hermes");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("app.toml"),
        format!(
            r#"
schema = "openagora/v1"
id = "hermes"
name = "Hermes"
summary = "Stands in for Hermes"
category = "agents"
homepage = "https://example.com"
source = "https://github.com/example/hermes"
license = "MIT"
platforms = ["{os}"]
role = "agent"
profile = "sunday"

[install]
provides = "hermes"

[[install.steps]]
os = ["{os}"]
shell = "sh"
run = 'mkdir -p "$HOME/.local/bin" && cp "$FAKE_HERMES" "$HOME/.local/bin/hermes" && chmod +x "$HOME/.local/bin/hermes" && touch "$HOME/installer-ran"'

[update]
run = "hermes update"

[run]
command = "hermes"
args = ["-p", "{{profile}}", "dashboard", "--isolated", "--host", "127.0.0.1", "--port", "{{port}}", "--no-open"]
port = "auto"

[run.health]
tcp = true

[ui]
kind = "web"
url = "http://127.0.0.1:{{port}}/"
"#
        ),
    )
    .unwrap();
}
