//! OpenCode as a push destination, the next push adapter after T3 Code
//! (acceptance test 7's push side, for OpenCode): Sunday starts a session in a
//! folder OpenCode knows, with a picture, after the user says yes; OpenCode
//! must ask before it edits files, and OpenAgora asks the user in its own
//! window and answers OpenCode; Sunday reads what it did. OpenCode 2's
//! background service, started with OpenCode's own command, and OpenCode 1's
//! `opencode serve`, run by OpenAgora with a password. OpenCode, its server
//! and OpenAgora's window are stand-ins.
#![cfg(unix)]

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use common::{Desktop, Env, McpClient, Responder};
use serde_json::{Value, json};

const SERVER: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fakes/opencode-server.py"
);
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x10\0\0\0\x10\x08\x06\0\0\0";

/// Stops what the test started: OpenCode's servers.
struct Cleanup(PathBuf);

impl Drop for Cleanup {
    fn drop(&mut self) {
        for file in [
            "home/.local/state/opencode/service.json",
            "data/run/opencode.toml",
        ] {
            let Ok(text) = fs::read_to_string(self.0.join(file)) else {
                continue;
            };
            let pid = text
                .lines()
                .find_map(|l| l.trim().strip_prefix("pid = "))
                .map(str::to_string)
                .or_else(|| {
                    serde_json::from_str::<Value>(&text)
                        .ok()
                        .and_then(|v| v["pid"].as_u64().map(|p| p.to_string()))
                });
            if let Some(pid) = pid {
                let _ = std::process::Command::new("kill").arg(pid).status();
            }
        }
    }
}

/// A computer with OpenCode `version` on it, which knows one project folder.
fn setup(version: &str) -> (Env, PathBuf, PathBuf, Cleanup) {
    let mut env = Env::new();
    env.add_fake_agents(&["opencode"]);
    fs::write(env.home.join(".fake-opencode-version"), version).unwrap();
    let log = env.data.with_file_name("opencode-requests.log");
    env.env("FAKE_OPENCODE_SERVER", SERVER);
    env.env("FAKE_OPENCODE_LOG", log.to_str().unwrap());
    let project = env.home.join("code/shop");
    fs::create_dir_all(&project).unwrap();
    let db_dir = env.home.join(".local/share/opencode");
    fs::create_dir_all(&db_dir).unwrap();
    let db = rusqlite::Connection::open(db_dir.join("opencode.db")).unwrap();
    db.execute_batch(
        "CREATE TABLE session_v2 (id TEXT PRIMARY KEY, title TEXT, directory TEXT, parent_id TEXT, time_created INTEGER, time_updated INTEGER);",
    )
    .unwrap();
    db.execute(
        "INSERT INTO session_v2 VALUES ('ses_old', 'Earlier work', ?1, NULL, 1700000000000, 1700000000000)",
        [project.to_str().unwrap()],
    )
    .unwrap();
    let root = env.home.parent().unwrap().to_path_buf();
    (env, project, log, Cleanup(root))
}

fn requests(log: &Path) -> Vec<Value> {
    fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

/// OpenAgora's window: says yes to starting tasks, and answers OpenCode's
/// requests with `answer`; keeps what it was asked.
fn window(env: &Env, answer: &'static str) -> (Desktop, Arc<Mutex<Vec<Value>>>) {
    let asked = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&asked);
    let responder: Responder = Box::new(move |request: &Value| {
        if request["method"] != "confirm/ask" {
            return None;
        }
        seen.lock().unwrap().push(request["params"].clone());
        let title = request["params"]["title"].as_str().unwrap_or_default();
        Some(if title.starts_with("Start a task in") {
            json!({"allowed": true, "choice": "once"})
        } else if answer == "reject" {
            json!({"allowed": false})
        } else {
            json!({"allowed": true, "choice": answer})
        })
    });
    (
        Desktop::connect_with(env, json!({}), Some(responder)),
        asked,
    )
}

fn picture_on_desk(env: &Env) -> String {
    let file = env.home.join("shot.png");
    fs::write(&file, PNG).unwrap();
    env.ok(&["desk", "add", "--file", file.to_str().unwrap()]);
    env.json(&["desk", "list", "--json"])[0]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn no_questions(request: &Value) -> Value {
    panic!("unexpected question {request}")
}

#[test]
fn sunday_runs_a_task_in_opencode_2_and_the_user_answers_its_requests_in_openagora() {
    let (env, project, log, _cleanup) = setup("2.0.26");
    let mut sunday = McpClient::start(&env, "sunday", true);
    let (_window, asked) = window(&env, "once");
    let item = picture_on_desk(&env);

    // A folder OpenCode doesn't know is refused, with the ones it knows.
    let (text, error) = sunday.call(
        "start_task",
        json!({"to": "opencode", "prompt": "Fix it", "project": "/etc"}),
        &mut no_questions,
    );
    assert!(error && text.contains("doesn't know the folder"), "{text}");
    assert!(text.contains(project.to_str().unwrap()), "{text}");
    // ... before the user is asked anything.
    assert!(asked.lock().unwrap().is_empty());

    // Sunday starts a task, in the folder (by its name), with the picture.
    let (text, error) = sunday.call(
        "start_task",
        json!({"to": "opencode", "title": "Fix the crash", "prompt": "Fix the crash in the screenshot.", "project": "shop", "items": [item]}),
        &mut no_questions,
    );
    assert!(!error, "{text}");
    assert!(
        text.contains("Started \"Fix the crash\" in OpenCode"),
        "{text}"
    );
    assert!(text.contains("`opencode -s ses_1`"), "{text}");
    let task = text
        .split_whitespace()
        .find(|w| w.starts_with("t-"))
        .unwrap()
        .trim_end_matches('.')
        .to_string();
    // The user said yes first, in OpenAgora's window, knowing where OpenCode
    // works and that it asks before it changes anything.
    let first = asked.lock().unwrap()[0].clone();
    assert_eq!(first["title"], "Start a task in OpenCode?");
    let told = first["message"].as_str().unwrap();
    assert!(
        told.contains(&format!("works on it in {}", project.display())),
        "{told}"
    );
    assert!(
        told.contains("asks before it edits files, runs commands"),
        "{told}"
    );
    // OpenCode's background service, started with its own command.
    let calls = fs::read_to_string(env.data.with_file_name("agent-calls.log")).unwrap();
    assert!(calls.contains("opencode service start"), "{calls}");
    let sent = requests(&log);
    let created = sent
        .iter()
        .find(|r| r["method"] == "POST" && r["path"] == "/api/session")
        .unwrap();
    assert!(created["auth"].as_bool().unwrap());
    assert_eq!(
        created["body"]["location"]["directory"],
        project.to_str().unwrap()
    );
    let rules = &created["body"]["permissions"];
    for what in ["edit", "shell", "webfetch", "external_directory"] {
        assert!(
            rules
                .as_array()
                .unwrap()
                .contains(&json!({"action": what, "resource": "*", "effect": "ask"})),
            "{rules}"
        );
    }
    let prompt = sent
        .iter()
        .find(|r| r["path"] == "/api/session/ses_1/prompt")
        .unwrap();
    assert!(
        prompt["body"]["text"]
            .as_str()
            .unwrap()
            .starts_with("Fix the crash in the screenshot.")
    );
    let uri = prompt["body"]["files"][0]["uri"].as_str().unwrap();
    assert_eq!(
        BASE64
            .decode(uri.strip_prefix("data:image/png;base64,").unwrap())
            .unwrap(),
        PNG
    );

    // OpenCode asks to edit; OpenAgora asks the user, who allows it once.
    let (text, error) = sunday.call(
        "task_status",
        json!({"task": task, "wait_seconds": 20}),
        &mut no_questions,
    );
    assert!(!error && text.contains("is completed"), "{text}");
    let question = asked.lock().unwrap()[1].clone();
    assert_eq!(question["title"], "Let OpenCode edit files?");
    assert_eq!(question["client"], "opencode");
    assert!(question["message"].as_str().unwrap().contains("src/app.ts"));
    assert!(
        question["message"]
            .as_str()
            .unwrap()
            .contains("Sunday can't")
    );
    let reply = requests(&log)
        .into_iter()
        .find(|r| r["path"].as_str().unwrap().ends_with("/reply"))
        .unwrap();
    assert_eq!(reply["path"], "/api/session/ses_1/permission/per_2/reply");
    assert_eq!(reply["body"], json!({"decision": "once"}));
    let (text, _) = sunday.call("task_result", json!({"task": task}), &mut no_questions);
    assert!(text.contains("Fixed the bug in src/app.ts."), "{text}");
    assert!(!text.contains("hmm"), "{text}");

    // Without OpenAgora's window, OpenCode's request waits for the user.
    let (text, _) = sunday.call(
        "start_task",
        json!({"to": "opencode", "title": "Another fix", "prompt": "Fix more.", "project": project.to_str().unwrap()}),
        &mut no_questions,
    );
    let second = text
        .split_whitespace()
        .find(|w| w.starts_with("t-"))
        .unwrap()
        .trim_end_matches('.')
        .to_string();
    _window.close();
    std::thread::sleep(std::time::Duration::from_millis(300));
    let (text, _) = sunday.call(
        "task_status",
        json!({"task": second, "wait_seconds": 1}),
        &mut no_questions,
    );
    assert!(text.contains("waiting for the user's OK"), "{text}");
    assert_eq!(
        requests(&log)
            .iter()
            .filter(|r| r["path"].as_str().unwrap().ends_with("/reply"))
            .count(),
        1
    );

    let audit = env.audit();
    let rows: Vec<String> = audit
        .iter()
        .filter(|e| {
            ["allow_task", "start_task", "task_permission"].contains(&e["action"].as_str().unwrap())
        })
        .map(|e| {
            format!(
                "{}:{}:{}",
                e["client"].as_str().unwrap(),
                e["action"].as_str().unwrap(),
                e["decision"].as_str().unwrap()
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            "sunday:allow_task:confirmed",
            "sunday:start_task:allowed",
            "opencode:task_permission:confirmed",
            "sunday:allow_task:confirmed",
            "sunday:start_task:allowed",
        ],
        "{audit:?}"
    );
    assert!(
        audit
            .iter()
            .any(|e| e["action"] == "task_permission" && e["outcome"] == "edit src/app.ts: once")
    );
}

#[test]
fn opencode_1_runs_tasks_in_a_server_openagora_starts_with_a_password() {
    let (env, project, log, _cleanup) = setup("1.19.3");
    let mut sunday = McpClient::start(&env, "sunday", true);
    let (_window, asked) = window(&env, "reject");
    let item = picture_on_desk(&env);
    let (text, error) = sunday.call(
        "start_task",
        json!({"to": "opencode", "title": "Fix the crash", "prompt": "Fix the crash.", "project": project.to_str().unwrap(), "items": [item]}),
        &mut no_questions,
    );
    assert!(!error, "{text}");
    assert!(
        text.contains(&format!("`opencode {} -s ses_1`", project.display())),
        "{text}"
    );
    // OpenAgora's own `opencode serve`, on 127.0.0.1, with a password in its
    // environment only (nothing in OpenCode's config).
    let calls = fs::read_to_string(env.data.with_file_name("agent-calls.log")).unwrap();
    assert!(
        calls.contains("opencode serve --hostname 127.0.0.1 --port"),
        "{calls}"
    );
    let run = fs::read_to_string(env.data.join("run/opencode.toml")).unwrap();
    assert!(run.contains("password = "), "{run}");
    assert!(!env.home.join(".config/opencode").exists());
    let sent = requests(&log);
    assert!(sent.iter().all(|r| r["auth"] == true), "{sent:?}");
    let created = sent.iter().find(|r| r["path"] == "/session").unwrap();
    assert_eq!(created["query"]["directory"], project.to_str().unwrap());
    let rules = created["body"]["permission"].as_array().unwrap();
    assert!(
        rules.contains(&json!({"permission": "bash", "pattern": "*", "action": "ask"})),
        "{rules:?}"
    );
    assert!(
        rules.contains(&json!({"permission": "question", "pattern": "*", "action": "deny"})),
        "{rules:?}"
    );
    let prompt = sent
        .iter()
        .find(|r| r["path"] == "/session/ses_1/prompt_async")
        .unwrap();
    let parts = prompt["body"]["parts"].as_array().unwrap();
    assert_eq!(parts[0]["type"], "text");
    assert_eq!(parts[1]["type"], "file");
    assert_eq!(parts[1]["mime"], "image/png");
    assert!(
        parts[1]["url"]
            .as_str()
            .unwrap()
            .starts_with("data:image/png;base64,")
    );

    // The user says no: OpenAgora tells OpenCode, and Sunday reads that.
    let task = text
        .split_whitespace()
        .find(|w| w.starts_with("t-"))
        .unwrap()
        .trim_end_matches('.')
        .to_string();
    let (text, _) = sunday.call(
        "task_status",
        json!({"task": task, "wait_seconds": 20}),
        &mut no_questions,
    );
    assert!(text.contains("is completed"), "{text}");
    assert_eq!(
        asked.lock().unwrap()[1]["title"],
        "Let OpenCode edit files?"
    );
    let reply = requests(&log)
        .into_iter()
        .find(|r| r["path"].as_str().unwrap().ends_with("/reply"))
        .unwrap();
    assert_eq!(reply["path"], "/permission/per_2/reply");
    assert_eq!(reply["body"], json!({"reply": "reject"}));
    let (text, _) = sunday.call("task_result", json!({"task": task}), &mut no_questions);
    assert!(
        text.contains("I wasn't allowed to edit src/app.ts"),
        "{text}"
    );
    assert!(!text.contains("Thinking"), "{text}");
    // The same server serves the next task.
    let (text, error) = sunday.call(
        "start_task",
        json!({"to": "opencode", "title": "Again", "prompt": "Again.", "project": "shop"}),
        &mut no_questions,
    );
    assert!(!error, "{text}");
    let serves = fs::read_to_string(env.data.with_file_name("agent-calls.log"))
        .unwrap()
        .matches("opencode serve")
        .count();
    assert_eq!(serves, 1);
    let audit = env.audit();
    assert!(audit.iter().any(|e| e["action"] == "task_permission"
        && e["decision"] == "declined"
        && e["outcome"] == "edit src/app.ts: reject"));
}
