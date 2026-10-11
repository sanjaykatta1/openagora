//! OpenCode as a push destination (issue #9, B8): the next push adapter after
//! T3 Code. Sunday starts and continues sessions in OpenCode through its own
//! local server API and reports on them; `send_to` still puts things in
//! OpenCode's inbox, as for every MCP agent.
//!
//! - **OpenCode 2:** its shared background service, found through the
//!   registration file OpenCode writes for its clients
//!   (`$XDG_STATE_HOME/opencode/service.json`), started with OpenCode's own
//!   `opencode service start` when it isn't running (the user said yes to the
//!   task first). The user opens a session with `opencode -s <id>`.
//! - **OpenCode 1:** `opencode serve`, run by OpenAgora on 127.0.0.1 with a
//!   random password (`OPENCODE_SERVER_PASSWORD`, in its environment only;
//!   nothing is written to OpenCode's config). Sessions are in OpenCode's own
//!   database, so `opencode <folder> -s <id>` opens them.
//! - Sessions start only in a folder OpenCode already knows (from its own
//!   database), with rules that make OpenCode ask before it edits files, runs
//!   commands, fetches pages or leaves the folder, and never ask questions of
//!   its own. While Sunday follows a task, OpenAgora asks the user about each
//!   of OpenCode's requests in its own window and answers OpenCode: Sunday
//!   never can.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::agents;
use crate::audit::{self, Entry};
use crate::desk::{self, Item, Kind};
use crate::destinations::{self, Destination, Progress, Pull, Task};
use crate::http;
use crate::jsonrpc::Peer;
use crate::paths::Paths;
use crate::process;
use crate::state;

pub const ID: &str = "opencode";
const NAME: &str = "OpenCode";
/// How long the user has to answer one of OpenCode's requests in OpenAgora's
/// window.
const ANSWER_WITHIN: Duration = Duration::from_secs(300);
const CALL_WITHIN: Duration = Duration::from_secs(30);
/// The most text a result returns.
const MAX_RESULT: usize = 20_000;
/// Pictures larger than this aren't sent (OpenCode 2 takes 20 MiB a file).
const MAX_PICTURE: usize = 20 * 1024 * 1024;

/// Which of OpenCode's server APIs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Api {
    V1,
    V2,
}

impl Api {
    fn as_str(self) -> &'static str {
        match self {
            Api::V1 => "1",
            Api::V2 => "2",
        }
    }

    fn of(task: &Task) -> Api {
        if task.remote.get("api").map(String::as_str) == Some("1") {
            Api::V1
        } else {
            Api::V2
        }
    }
}

/// A running OpenCode server on this computer.
#[derive(Debug, Clone)]
struct Server {
    api: Api,
    port: u16,
    password: Option<String>,
}

/// The `opencode serve` OpenAgora runs for OpenCode 1.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Run {
    pid: u32,
    port: u16,
    password: String,
}

/// OpenCode 2's registration file, as OpenCode writes it for its clients.
#[derive(Debug, Deserialize)]
struct Registration {
    url: String,
    #[serde(default)]
    pid: Option<u32>,
    #[serde(default)]
    password: Option<String>,
}

fn run_file(paths: &Paths) -> PathBuf {
    paths.root.join("run").join("opencode.toml")
}

/// Where OpenCode 2 registers its background service.
fn registration_file(home: &Path) -> PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|d| d.is_absolute())
        .unwrap_or_else(|| home.join(".local").join("state"))
        .join("opencode")
        .join("service.json")
}

/// The port of a URL on this computer; None for any other host.
fn local_port(url: &str) -> Option<u16> {
    let rest = url.strip_prefix("http://")?;
    let host_port = rest.split('/').next()?;
    let (host, port) = host_port.rsplit_once(':')?;
    matches!(host, "127.0.0.1" | "localhost")
        .then(|| port.parse().ok())
        .flatten()
}

/// Percent-encodes a query value.
fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

impl Server {
    fn call(&self, method: &str, path: &str, body: Option<&Value>) -> Result<Value> {
        let auth = self
            .password
            .as_ref()
            .map(|p| format!("Basic {}", BASE64.encode(format!("opencode:{p}"))));
        let mut headers: Vec<(&str, &str)> = vec![("Accept", "application/json")];
        if let Some(auth) = &auth {
            headers.push(("Authorization", auth));
        }
        let bytes = body.map(|b| b.to_string().into_bytes());
        if bytes.is_some() {
            headers.push(("Content-Type", "application/json"));
        }
        let response = http::request(
            method,
            self.port,
            path,
            &headers,
            bytes.as_deref(),
            CALL_WITHIN,
        )
        .with_context(|| format!("asking {NAME}'s server"))?;
        match response.status {
            200..=299 if response.body.is_empty() => Ok(Value::Null),
            200..=299 => response.json(),
            401 | 403 => bail!("{NAME}'s server didn't accept OpenAgora's password"),
            404 => {
                bail!("{NAME}'s server doesn't know {path}: is this the right version of {NAME}?")
            }
            code => bail!(
                "{NAME}'s server answered {code}: {}",
                String::from_utf8_lossy(&response.body)
                    .chars()
                    .take(300)
                    .collect::<String>()
            ),
        }
    }

    fn healthy(&self) -> bool {
        let path = match self.api {
            Api::V1 => "/global/health",
            Api::V2 => "/api/info",
        };
        self.call("GET", path, None).is_ok()
    }
}

/// A picture for a prompt.
struct Picture {
    mime: String,
    name: String,
    /// Base64.
    data: String,
}

impl Picture {
    fn uri(&self) -> String {
        format!("data:{};base64,{}", self.mime, self.data)
    }
}

/// OpenCode: its inbox (pull) as for every MCP agent, and sessions it runs
/// for the user (push).
pub struct OpenCode {
    pull: Pull,
    paths: Paths,
    /// OpenAgora's window, where the user answers OpenCode's requests.
    desktop: Option<Arc<Peer>>,
}

pub fn destination(paths: &Paths, desktop: Option<Arc<Peer>>) -> OpenCode {
    OpenCode {
        pull: Pull {
            paths: paths.clone(),
            id: ID.into(),
            name: NAME.into(),
            from: crate::capture::SUNDAY.into(),
        },
        paths: paths.clone(),
        desktop,
    }
}

/// The folders OpenCode already knows, newest first, from its own database
/// (read only).
pub fn projects() -> Vec<String> {
    let Ok(home) = crate::paths::home() else {
        return Vec::new();
    };
    crate::sessions::opencode_projects(&home)
}

impl OpenCode {
    /// OpenCode on this computer: its command and its major version.
    fn installed(&self) -> Result<(PathBuf, Api)> {
        let descriptor = agents::descriptor(ID).context("no OpenCode descriptor")?;
        let found = agents::find(descriptor);
        let exe = found
            .exe
            .clone()
            .ok_or_else(|| anyhow!("{NAME} isn't on this computer"))?;
        let api = if found.major().is_some_and(|m| m >= 2) {
            Api::V2
        } else {
            Api::V1
        };
        Ok((exe, api))
    }

    /// OpenCode's server for `api`; started when `start` (the user said yes
    /// to a task) and it isn't running.
    fn server(&self, api: Api, exe: Option<&Path>, start: bool) -> Result<Server> {
        match api {
            Api::V2 => self.service(exe, start),
            Api::V1 => self.serve(exe, start),
        }
    }

    /// OpenCode 2's background service.
    fn service(&self, exe: Option<&Path>, start: bool) -> Result<Server> {
        let home = crate::paths::home()?;
        let registered = || -> Option<Server> {
            let text = std::fs::read_to_string(registration_file(&home)).ok()?;
            let r: Registration = serde_json::from_str(&text).ok()?;
            if r.pid.is_some_and(|pid| !process::alive(pid)) {
                return None;
            }
            let server = Server {
                api: Api::V2,
                port: local_port(&r.url)?,
                password: r.password.filter(|p| !p.is_empty()),
            };
            server.healthy().then_some(server)
        };
        if let Some(server) = registered() {
            return Ok(server);
        }
        let Some(exe) = exe.filter(|_| start) else {
            bail!(
                "{NAME}'s background service isn't running. Start a task to start it, or the user can run `opencode service start`."
            );
        };
        let mut cmd = std::process::Command::new(exe);
        cmd.args(["service", "start"]);
        let out = process::output_within(&mut cmd, Duration::from_secs(60))
            .with_context(|| format!("running {} service start", exe.display()))?;
        if !out.status.success() {
            bail!(
                "`opencode service start` failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            if let Some(server) = registered() {
                return Ok(server);
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        bail!("{NAME}'s background service didn't come up")
    }

    /// OpenCode 1's `opencode serve`, run by OpenAgora.
    fn serve(&self, exe: Option<&Path>, start: bool) -> Result<Server> {
        let saved: Option<Run> = state::read(&run_file(&self.paths)).ok().flatten();
        if let Some(run) = &saved {
            let server = Server {
                api: Api::V1,
                port: run.port,
                password: Some(run.password.clone()),
            };
            if process::alive(run.pid) && server.healthy() {
                return Ok(server);
            }
        }
        let Some(exe) = exe.filter(|_| start) else {
            bail!("The {NAME} server OpenAgora runs for its tasks isn't running any more.");
        };
        let port = process::free_port()?;
        let mut key = [0u8; 24];
        getrandom::fill(&mut key).map_err(|e| anyhow!("no randomness: {e}"))?;
        let password = crate::github::sha256_hex(&key)[..32].to_string();
        let log = self.paths.root.join("logs").join("opencode-serve.log");
        if let Some(dir) = log.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let out = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log)?;
        let mut cmd = std::process::Command::new(exe);
        cmd.args([
            "serve",
            "--hostname",
            "127.0.0.1",
            "--port",
            &port.to_string(),
        ])
        .env("OPENCODE_SERVER_PASSWORD", &password)
        .current_dir(crate::paths::home()?)
        .stdin(std::process::Stdio::null())
        .stdout(out.try_clone()?)
        .stderr(out);
        let mut child = process::spawn_detached(&mut cmd)
            .with_context(|| format!("starting {} serve", exe.display()))?;
        let server = Server {
            api: Api::V1,
            port,
            password: Some(password.clone()),
        };
        process::wait_healthy(&mut child, Duration::from_secs(30), || server.healthy())
            .with_context(|| format!("{NAME}'s server didn't come up (see {})", log.display()))?;
        state::write_private(
            &run_file(&self.paths),
            &Run {
                pid: child.id(),
                port,
                password,
            },
        )?;
        Ok(server)
    }

    /// The folder a task runs in: one OpenCode already knows, by its path or
    /// its name.
    fn project(&self, asked: Option<&str>) -> Result<String> {
        let known = projects();
        let listed = || {
            if known.is_empty() {
                format!("{NAME} knows no folders yet: the user opens one in {NAME} first.")
            } else {
                format!(
                    "Give project as one of the folders {NAME} knows: {}.",
                    known
                        .iter()
                        .take(20)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        };
        let Some(asked) = asked.map(str::trim).filter(|a| !a.is_empty()) else {
            bail!("Which folder should {NAME} work in? {}", listed());
        };
        let trimmed = asked.trim_end_matches(['/', '\\']);
        let found = known
            .iter()
            .find(|k| k.trim_end_matches(['/', '\\']) == trimmed)
            .or_else(|| {
                let mut named = known.iter().filter(|k| {
                    Path::new(k.as_str())
                        .file_name()
                        .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(trimmed))
                });
                let first = named.next();
                // A name that fits more than one folder is no answer.
                first.filter(|_| named.next().is_none())
            });
        match found {
            Some(dir) if Path::new(dir).is_dir() => Ok(dir.clone()),
            Some(dir) => bail!("{dir} isn't there any more. {}", listed()),
            None => bail!("{NAME} doesn't know the folder {asked:?}. {}", listed()),
        }
    }

    /// The prompt and its pictures, as `(text, [(mime, base64)])`.
    fn compose(&self, message: &str, items: &[Item]) -> Result<(String, Vec<Picture>)> {
        let mut text = message.trim().to_string();
        let mut pictures = Vec::new();
        for item in items {
            let bytes = desk::payload(&self.paths, item)?;
            if matches!(item.kind, Kind::Screenshot | Kind::Image) {
                if bytes.len() > MAX_PICTURE {
                    bail!("{} is too big for {NAME} (20 MB at most)", item.id);
                }
                text.push_str(&format!("\n[Attached: {}.]", item.describe()));
                pictures.push(Picture {
                    mime: item.mime.clone(),
                    name: destinations::file_name(item),
                    data: BASE64.encode(bytes),
                });
            } else {
                let content = String::from_utf8(bytes).map_err(|_| {
                    anyhow!(
                        "{NAME} tasks take pictures and text; {} is neither",
                        item.id
                    )
                })?;
                text.push_str(&format!(
                    "\n\n[{} from the user's apps, sent by Sunday:]\n{content}",
                    item.describe()
                ));
            }
        }
        Ok((text.trim().to_string(), pictures))
    }

    /// Sends a prompt (and pictures) to a session.
    fn prompt(
        &self,
        server: &Server,
        session: &str,
        directory: &str,
        text: &str,
        pictures: &[Picture],
    ) -> Result<()> {
        match server.api {
            Api::V1 => {
                let mut parts = vec![json!({"type": "text", "text": text})];
                for p in pictures {
                    parts.push(
                        json!({"type": "file", "mime": p.mime, "filename": p.name, "url": p.uri()}),
                    );
                }
                server.call(
                    "POST",
                    &format!(
                        "/session/{session}/prompt_async?directory={}",
                        encode(directory)
                    ),
                    Some(&json!({"parts": parts})),
                )?;
            }
            Api::V2 => {
                let files: Vec<Value> = pictures
                    .iter()
                    .map(|p| json!({"uri": p.uri(), "name": p.name}))
                    .collect();
                let mut body = json!({"text": text});
                if !files.is_empty() {
                    body["files"] = json!(files);
                }
                server.call(
                    "POST",
                    &format!("/api/session/{session}/prompt"),
                    Some(&body),
                )?;
            }
        }
        Ok(())
    }

    /// OpenCode's pending requests for a session: (id, what, about).
    fn requests(
        &self,
        server: &Server,
        session: &str,
        directory: &str,
    ) -> Result<Vec<(String, String, Vec<String>)>> {
        let list = match server.api {
            Api::V1 => server.call(
                "GET",
                &format!("/permission?directory={}", encode(directory)),
                None,
            )?,
            Api::V2 => {
                server.call("GET", &format!("/api/session/{session}/permission"), None)?["data"]
                    .take()
            }
        };
        Ok(list
            .as_array()
            .into_iter()
            .flatten()
            .filter(|r| r["sessionID"].as_str().is_none_or(|s| s == session))
            .filter_map(|r| {
                let id = r["id"].as_str()?.to_string();
                let what = r["permission"]
                    .as_str()
                    .or(r["action"].as_str())?
                    .to_string();
                let about = r["patterns"]
                    .as_array()
                    .or(r["resources"].as_array())
                    .into_iter()
                    .flatten()
                    .filter_map(|p| p.as_str().map(str::to_string))
                    .collect();
                Some((id, what, about))
            })
            .collect())
    }

    fn reply(
        &self,
        server: &Server,
        session: &str,
        directory: &str,
        request: &str,
        answer: &str,
    ) -> Result<()> {
        match server.api {
            Api::V1 => server.call(
                "POST",
                &format!(
                    "/permission/{request}/reply?directory={}",
                    encode(directory)
                ),
                Some(&json!({"reply": answer})),
            )?,
            Api::V2 => server.call(
                "POST",
                &format!("/api/session/{session}/permission/{request}/reply"),
                Some(&json!({"decision": answer})),
            )?,
        };
        Ok(())
    }

    /// Asks the user, in OpenAgora's window, about one of OpenCode's
    /// requests. None when there's no window to ask in.
    fn ask(&self, task: &Task, what: &str, about: &[String]) -> Option<&'static str> {
        let desktop = self.desktop.as_ref().filter(|d| !d.is_closed())?;
        let doing = match what {
            "edit" => "edit files",
            "bash" | "shell" => "run commands",
            "webfetch" => "fetch web pages",
            "websearch" => "search the web",
            "external_directory" => "work outside the task's folder",
            other => other,
        };
        let listed: Vec<String> = about.iter().take(8).map(|a| format!("• {a}")).collect();
        let message = format!(
            "In the task \u{201c}{}\u{201d} that Sunday started, {NAME} asks to {doing}{}\n\nOnly you can answer this; Sunday can't.",
            task.title,
            if listed.is_empty() {
                ".".to_string()
            } else {
                format!(":\n{}", listed.join("\n"))
            }
        );
        let asked = desktop.request(
            "confirm/ask",
            json!({
                "client": ID,
                "title": format!("Let {NAME} {doing}?"),
                "message": message,
                "choices": [{"id": "once", "label": "Allow once"}, {"id": "always", "label": "Allow for this task"}],
            }),
            ANSWER_WITHIN,
        );
        let answer = match asked {
            Ok(reply) if reply["allowed"] == true => match reply["choice"].as_str() {
                Some("always") => "always",
                Some("once") => "once",
                _ => "reject",
            },
            _ => "reject",
        };
        let mut entry = Entry::new("opencode", "task_permission", &task.id);
        entry.via = Some("window".into());
        entry.decision = if answer == "reject" {
            "declined"
        } else {
            "confirmed"
        }
        .into();
        entry.outcome = format!("{what} {}: {answer}", about.join(", "));
        let _ = audit::record(&self.paths, &entry);
        Some(answer)
    }

    /// Whether the session is running now.
    fn busy(&self, server: &Server, session: &str, directory: &str) -> Result<bool> {
        let active = match server.api {
            Api::V1 => server.call(
                "GET",
                &format!("/session/status?directory={}", encode(directory)),
                None,
            )?,
            Api::V2 => server.call("GET", "/api/session/active", None)?["data"].take(),
        };
        Ok(active.get(session).is_some())
    }

    /// The session's assistant messages, as (text, failed), oldest first.
    fn answers(
        &self,
        server: &Server,
        session: &str,
        directory: &str,
    ) -> Result<Vec<(String, Option<String>)>> {
        let mut out = Vec::new();
        match server.api {
            Api::V1 => {
                let messages = server.call(
                    "GET",
                    &format!("/session/{session}/message?directory={}", encode(directory)),
                    None,
                )?;
                for m in messages.as_array().into_iter().flatten() {
                    if m["info"]["role"] != "assistant" {
                        continue;
                    }
                    let text: Vec<&str> = m["parts"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter(|p| p["type"] == "text" && p["synthetic"] != true)
                        .filter_map(|p| p["text"].as_str())
                        .collect();
                    let error = m["info"]["error"]["data"]["message"]
                        .as_str()
                        .or(m["info"]["error"]["name"].as_str())
                        .map(str::to_string);
                    out.push((text.join("\n"), error));
                }
            }
            Api::V2 => {
                let messages = server.call(
                    "GET",
                    &format!("/api/session/{session}/message?order=asc&limit=200"),
                    None,
                )?;
                for m in messages["data"].as_array().into_iter().flatten() {
                    match m["type"].as_str() {
                        Some("assistant") => {
                            let text: Vec<&str> = m["content"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter(|c| c["type"] == "text")
                                .filter_map(|c| c["text"].as_str())
                                .collect();
                            let error = m["error"]["message"]
                                .as_str()
                                .or(m["error"]["name"].as_str())
                                .map(str::to_string);
                            out.push((text.join("\n"), error));
                        }
                        Some("idle") if m["outcome"] != "succeeded" => {
                            let why = m["error"]["message"]
                                .as_str()
                                .unwrap_or("it stopped")
                                .to_string();
                            out.push((
                                String::new(),
                                Some(format!(
                                    "{} ({why})",
                                    m["outcome"].as_str().unwrap_or("failed")
                                )),
                            ));
                        }
                        _ => {}
                    }
                }
            }
        }
        Ok(out)
    }

    fn remote<'a>(&self, task: &'a Task) -> Result<(&'a str, &'a str)> {
        let session = task
            .remote
            .get("session")
            .ok_or_else(|| anyhow!("task {} has no {NAME} session", task.id))?;
        let directory = task
            .remote
            .get("directory")
            .map(String::as_str)
            .unwrap_or_default();
        Ok((session, directory))
    }
}

/// What OpenCode may do in a task OpenAgora starts: ask before editing,
/// running commands, fetching or leaving the folder; no questions of its own,
/// since no one may be watching.
fn rules(api: Api) -> Value {
    let ask = [
        "edit",
        if api == Api::V1 { "bash" } else { "shell" },
        "webfetch",
        "websearch",
        "external_directory",
    ];
    let mut rules: Vec<Value> = ask
        .iter()
        .map(|what| match api {
            Api::V1 => json!({"permission": what, "pattern": "*", "action": "ask"}),
            Api::V2 => json!({"action": what, "resource": "*", "effect": "ask"}),
        })
        .collect();
    rules.push(match api {
        Api::V1 => json!({"permission": "question", "pattern": "*", "action": "deny"}),
        Api::V2 => json!({"action": "question", "resource": "*", "effect": "deny"}),
    });
    json!(rules)
}

impl Destination for OpenCode {
    fn id(&self) -> &str {
        ID
    }

    fn name(&self) -> &str {
        NAME
    }

    fn accepts(&self) -> Vec<String> {
        self.pull.accepts()
    }

    fn consequence(&self) -> String {
        self.pull.consequence()
    }

    fn next_step(&self, places: &[String]) -> String {
        self.pull.next_step(places)
    }

    fn check_task(&self, project: Option<&str>) -> Result<()> {
        self.installed()?;
        self.project(project).map(|_| ())
    }

    fn task_note(&self, project: Option<&str>) -> String {
        let folder = self
            .project(project)
            .unwrap_or_else(|_| "the folder Sunday named".into());
        format!(
            "{NAME} works on it in {folder}. It asks before it edits files, runs commands or fetches pages, and OpenAgora asks you about each in its window while Sunday follows the task."
        )
    }

    /// Into OpenCode's inbox, or into a session OpenAgora started.
    fn deliver(&self, item: &Item, task: Option<&Task>, message: Option<&str>) -> Result<String> {
        let Some(task) = task else {
            return self.pull.deliver(item, None, message);
        };
        let (session, directory) = self.remote(task)?;
        let server = self.server(Api::of(task), None, false)?;
        let (text, pictures) = self.compose(
            message.unwrap_or("Sunday sends this for the task."),
            std::slice::from_ref(item),
        )?;
        self.prompt(&server, session, directory, &text, &pictures)?;
        Ok(format!("the {NAME} session \u{201c}{}\u{201d}", task.title))
    }

    fn start_task(
        &self,
        title: &str,
        prompt: &str,
        items: &[Item],
        project: Option<&str>,
    ) -> Result<Task> {
        let directory = self.project(project)?;
        let (exe, api) = self.installed()?;
        let server = self.server(api, Some(&exe), true)?;
        let (text, pictures) = self.compose(prompt, items)?;
        let session = match api {
            Api::V1 => server.call(
                "POST",
                &format!("/session?directory={}", encode(&directory)),
                Some(&json!({"title": title, "permission": rules(api)})),
            )?,
            Api::V2 => server.call(
                "POST",
                "/api/session",
                Some(&json!({"location": {"directory": directory}, "title": title, "permissions": rules(api)})),
            )?["data"]
                .take(),
        };
        let session = session["id"]
            .as_str()
            .ok_or_else(|| anyhow!("{NAME} didn't say which session it started"))?
            .to_string();
        self.prompt(&server, &session, &directory, &text, &pictures)?;
        let mut remote = std::collections::BTreeMap::new();
        remote.insert("session".to_string(), session.clone());
        remote.insert("directory".to_string(), directory.clone());
        remote.insert("api".to_string(), api.as_str().to_string());
        let open_with = match api {
            Api::V2 => format!("opencode -s {session}"),
            Api::V1 => format!("opencode {} -s {session}", shell_word(&directory)),
        };
        Ok(Task {
            id: destinations::new_task_id(&session),
            destination: ID.into(),
            title: title.to_string(),
            created_at: state::now(),
            started_by: String::new(),
            remote,
            items: items.iter().map(|i| i.id.clone()).collect(),
            link: None,
            open_with: Some(open_with),
        })
    }

    /// How the session is going; meanwhile OpenAgora asks the user about
    /// OpenCode's requests, in its own window, and answers them.
    fn status(&self, task: &Task, wait: Duration) -> Result<Progress> {
        let (session, directory) = self.remote(task)?;
        let server = self.server(Api::of(task), None, false)?;
        let deadline = Instant::now() + wait;
        loop {
            let requests = self.requests(&server, session, directory)?;
            let mut unanswered = false;
            for (id, what, about) in &requests {
                match self.ask(task, what, about) {
                    Some(answer) => self.reply(&server, session, directory, id, answer)?,
                    None => unanswered = true,
                }
            }
            if unanswered {
                return Ok(Progress {
                    state: format!(
                        "waiting for the user's OK in {NAME} (OpenAgora's window isn't open to ask)"
                    ),
                    finished: false,
                });
            }
            if requests.is_empty() && !self.busy(&server, session, directory)? {
                let failed = self
                    .answers(&server, session, directory)?
                    .last()
                    .and_then(|(_, error)| error.clone());
                return Ok(Progress {
                    state: match failed {
                        Some(why) => format!("failed: {why}"),
                        None => "completed".into(),
                    },
                    finished: true,
                });
            }
            if Instant::now() >= deadline {
                return Ok(Progress {
                    state: "running".into(),
                    finished: false,
                });
            }
            std::thread::sleep(
                Duration::from_millis(500).min(deadline.saturating_duration_since(Instant::now())),
            );
        }
    }

    fn result(&self, task: &Task) -> Result<String> {
        let (session, directory) = self.remote(task)?;
        let server = self.server(Api::of(task), None, false)?;
        let mut out = String::new();
        for (text, error) in self.answers(&server, session, directory)? {
            if !text.trim().is_empty() {
                out.push_str(text.trim());
                out.push_str("\n\n");
            }
            if let Some(error) = error {
                out.push_str(&format!("[{NAME} reported a problem: {error}]\n\n"));
            }
        }
        if out.len() > MAX_RESULT {
            let cut: String = out.chars().take(MAX_RESULT).collect();
            out = format!("{cut}\n[Shortened: read the rest in {NAME}.]");
        }
        Ok(if out.trim().is_empty() {
            "Nothing yet.".into()
        } else {
            out.trim().to_string()
        })
    }
}

/// A folder as one word for a shell.
fn shell_word(path: &str) -> String {
    if path
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "/._-~".contains(c))
    {
        path.to_string()
    } else {
        format!("'{}'", path.replace('\'', "'\\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_local_urls_are_used() {
        assert_eq!(local_port("http://127.0.0.1:49374"), Some(49374));
        assert_eq!(local_port("http://localhost:4096/"), Some(4096));
        assert_eq!(local_port("http://example.com:4096"), None);
        assert_eq!(local_port("https://127.0.0.1:4096"), None);
    }

    #[test]
    fn query_values_are_encoded() {
        assert_eq!(encode("/home/me/my app"), "%2Fhome%2Fme%2Fmy%20app");
        assert_eq!(shell_word("/home/me/app"), "/home/me/app");
        assert_eq!(shell_word("/home/me/it's"), "'/home/me/it'\\''s'");
    }

    #[test]
    fn tasks_ask_before_editing_running_or_fetching() {
        let v1 = rules(Api::V1).to_string();
        assert!(
            v1.contains(r#"{"action":"ask","pattern":"*","permission":"bash"}"#),
            "{v1}"
        );
        assert!(
            v1.contains(r#"{"action":"deny","pattern":"*","permission":"question"}"#),
            "{v1}"
        );
        let v2 = rules(Api::V2).to_string();
        assert!(
            v2.contains(r#"{"action":"shell","effect":"ask","resource":"*"}"#),
            "{v2}"
        );
        assert!(
            v2.contains(r#"{"action":"edit","effect":"ask","resource":"*"}"#),
            "{v2}"
        );
    }
}
