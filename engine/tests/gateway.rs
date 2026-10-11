//! The engine service and its MCP gateway, end to end: a client talks to
//! `openagora mcp` exactly as Hermes does, and the engine itself asks the user
//! before installing (acceptance test 5).
#![cfg(unix)]

mod common;

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::sync::mpsc;
use std::time::Duration;

use common::{Env, McpClient};
use serde_json::{Value, json};

fn has_python() -> bool {
    std::process::Command::new("python3")
        .arg("--version")
        .output()
        .is_ok()
}

/// Answers every elicitation with `action`, and remembers the questions.
fn answering<'a>(
    action: &'static str,
    asked: &'a mut Vec<String>,
) -> impl FnMut(&Value) -> Value + 'a {
    move |request| {
        assert_eq!(request["method"], "elicitation/create", "{request}");
        asked.push(request["params"]["message"].as_str().unwrap().to_string());
        json!({"action": action, "content": {}})
    }
}

/// OpenAgora's window, connected to the engine service.
struct Desktop {
    stream: UnixStream,
    incoming: mpsc::Receiver<Value>,
}

impl Desktop {
    fn connect(env: &Env) -> Desktop {
        let mut stream = UnixStream::connect(env.socket()).unwrap();
        writeln!(
            stream,
            "{}",
            json!({"hello": {"role": "desktop", "client": "desktop"}})
        )
        .unwrap();
        let reader = BufReader::new(stream.try_clone().unwrap());
        let (tx, incoming) = mpsc::channel();
        std::thread::spawn(move || {
            for line in reader.lines().map_while(Result::ok) {
                if tx
                    .send(serde_json::from_str::<Value>(&line).unwrap())
                    .is_err()
                {
                    break;
                }
            }
        });
        // Make sure the engine has registered the window before going on.
        writeln!(
            stream,
            "{}",
            json!({"jsonrpc": "2.0", "id": "p", "method": "ping"})
        )
        .unwrap();
        let pong = incoming.recv_timeout(Duration::from_secs(10)).unwrap();
        assert_eq!(pong["id"], "p");
        Desktop { stream, incoming }
    }

    fn next(&self) -> Value {
        self.incoming
            .recv_timeout(Duration::from_secs(30))
            .expect("nothing from the engine")
    }

    fn reply(&mut self, id: &Value, result: Value) {
        writeln!(
            self.stream,
            "{}",
            json!({"jsonrpc": "2.0", "id": id, "result": result})
        )
        .unwrap();
    }
}

#[test]
fn sunday_installs_starts_and_opens_an_app_only_after_the_user_confirms() {
    if !has_python() {
        eprintln!("python3 not found; skipping");
        return;
    }
    let env = Env::new();
    env.add_web_app("notes");
    let mut sunday = McpClient::start(&env, "sunday", true);
    let tools = sunday.tools();
    for tool in [
        "store_search",
        "store_show",
        "app_install",
        "app_update",
        "app_uninstall",
        "app_start",
        "app_stop",
        "app_status",
        "app_open",
    ] {
        assert!(
            tools.contains(&tool.to_string()),
            "{tool} missing from {tools:?}"
        );
    }
    let mut no_questions = |r: &Value| -> Value { panic!("unexpected request {r}") };
    let (found, _) = sunday.call(
        "store_search",
        json!({"query": "tiny web"}),
        &mut no_questions,
    );
    assert!(
        found.contains("Web notes (notes)") && found.contains("not installed"),
        "{found}"
    );
    let (shown, _) = sunday.call("store_show", json!({"id": "notes"}), &mut no_questions);
    assert!(shown.contains("run: echo hello-notes"), "{shown}");

    // The engine asks the user in Sunday's chat. A no installs nothing.
    let mut asked = Vec::new();
    let (declined, error) = sunday.call(
        "app_install",
        json!({"id": "notes"}),
        &mut answering("decline", &mut asked),
    );
    assert!(!error && declined.contains("did not confirm"), "{declined}");
    assert_eq!(asked.len(), 1);
    assert!(
        asked[0].starts_with("Install Web notes?") && asked[0].contains("echo hello-notes"),
        "{}",
        asked[0]
    );
    assert!(!env.data.join("state/notes.toml").exists());

    // A cancel or timeout is a no too, never a silent yes.
    let (cancelled, _) = sunday.call(
        "app_install",
        json!({"id": "notes"}),
        &mut answering("cancel", &mut asked),
    );
    assert!(cancelled.contains("did not confirm"), "{cancelled}");
    assert!(!env.data.join("state/notes.toml").exists());

    let (installed, error) = sunday.call(
        "app_install",
        json!({"id": "notes"}),
        &mut answering("accept", &mut asked),
    );
    assert!(
        !error && installed.contains("Installed Web notes"),
        "{installed}"
    );
    assert!(env.data.join("apps/notes/index.html").exists());

    let (started, error) = sunday.call("app_start", json!({"id": "notes"}), &mut no_questions);
    assert!(
        !error && started.contains("running at http://127.0.0.1:"),
        "{started}"
    );

    // With OpenAgora's window open, app_open shows the app in its tab.
    let (not_open, _) = sunday.call("app_open", json!({"id": "notes"}), &mut no_questions);
    assert!(not_open.contains("window isn't open"), "{not_open}");
    let desktop = Desktop::connect(&env);
    let (opened, error) = sunday.call("app_open", json!({"id": "notes"}), &mut no_questions);
    assert!(!error && opened.contains("Opened Web notes"), "{opened}");
    let note = desktop.next();
    assert_eq!(note["method"], "app/open");
    assert_eq!(note["params"]["id"], "notes");

    // Every app action is in the audit log, with how it was decided.
    let audit = env.audit();
    let summary: Vec<(String, String, String)> = audit
        .iter()
        .map(|e| {
            (
                e["action"].as_str().unwrap().to_string(),
                e["decision"].as_str().unwrap().to_string(),
                e["via"].as_str().unwrap_or("-").to_string(),
            )
        })
        .collect();
    let row = |a: &str, d: &str, v: &str| (a.to_string(), d.to_string(), v.to_string());
    assert_eq!(
        summary,
        vec![
            row("app_install", "declined", "chat"),
            row("app_install", "declined", "chat"),
            row("app_install", "confirmed", "chat"),
            row("app_start", "allowed", "-"),
            row("app_open", "allowed", "-"),
            row("app_open", "allowed", "-"),
        ]
    );
    assert!(
        audit
            .iter()
            .all(|e| e["client"] == "sunday" && e["target"] == "notes")
    );
    env.ok(&["stop", "notes"]);
}

#[test]
fn without_a_chat_prompt_the_window_asks_and_otherwise_nothing_happens() {
    let env = Env::new();
    env.add_web_app("notes");
    let mut agent = McpClient::start(&env, "sunday", false);
    let mut no_questions = |r: &Value| -> Value { panic!("unexpected request {r}") };

    // No way to ask anyone: refused.
    let (refused, error) = agent.call("app_install", json!({"id": "notes"}), &mut no_questions);
    assert!(error && refused.contains("couldn't ask"), "{refused}");
    assert!(!env.data.join("state/notes.toml").exists());

    // OpenAgora's window asks instead.
    let mut desktop = Desktop::connect(&env);
    let answer = std::thread::spawn(move || {
        let ask = desktop.next();
        assert_eq!(ask["method"], "confirm/ask");
        assert_eq!(ask["params"]["client"], "sunday");
        assert_eq!(ask["params"]["title"], "Install Web notes?");
        let id = ask["id"].clone();
        desktop.reply(&id, json!({"allowed": true}));
        desktop
    });
    let (installed, error) = agent.call("app_install", json!({"id": "notes"}), &mut no_questions);
    assert!(!error && installed.contains("Installed"), "{installed}");
    let mut desktop = answer.join().unwrap();

    // ...and the window's no is a no.
    let answer = std::thread::spawn(move || {
        let ask = desktop.next();
        let id = ask["id"].clone();
        desktop.reply(&id, json!({"allowed": false}));
    });
    let (kept, _) = agent.call("app_uninstall", json!({"id": "notes"}), &mut no_questions);
    assert!(kept.contains("did not confirm"), "{kept}");
    answer.join().unwrap();
    assert!(env.data.join("state/notes.toml").exists());

    let decisions: Vec<String> = env
        .audit()
        .iter()
        .map(|e| {
            format!(
                "{} {} {}",
                e["action"].as_str().unwrap(),
                e["decision"].as_str().unwrap(),
                e["via"].as_str().unwrap_or("-")
            )
        })
        .collect();
    assert_eq!(
        decisions,
        [
            "app_install refused -",
            "app_install confirmed window",
            "app_uninstall declined window"
        ]
    );
}

#[test]
fn a_client_whose_chat_prompt_fails_falls_back_to_the_window() {
    let env = Env::new();
    env.add_web_app("notes");
    let mut agent = McpClient::start(&env, "sunday", true);
    let mut desktop = Desktop::connect(&env);
    let window = std::thread::spawn(move || {
        let ask = desktop.next();
        assert_eq!(ask["method"], "confirm/ask");
        let id = ask["id"].clone();
        desktop.reply(&id, json!({"allowed": true}));
    });
    let mut broken_prompt = |_: &Value| json!({"__error": "elicitation is not supported here"});
    let (installed, error) = agent.call("app_install", json!({"id": "notes"}), &mut broken_prompt);
    assert!(!error && installed.contains("Installed"), "{installed}");
    window.join().unwrap();
    assert_eq!(env.audit()[0]["via"], "window");
}

#[test]
fn sunday_cannot_remove_or_stop_itself_and_other_clients_get_no_store_tools() {
    let env = Env::new();
    env.install_hermes_by_hand();
    env.ok(&["sunday", "setup", "--fresh"]);
    let mut sunday = McpClient::start(&env, "sunday", true);
    let mut no_questions = |r: &Value| -> Value { panic!("unexpected request {r}") };
    for tool in ["app_uninstall", "app_update", "app_stop"] {
        let (text, error) = sunday.call(tool, json!({"id": "hermes"}), &mut no_questions);
        assert!(error && text.contains("can't"), "{tool}: {text}");
    }
    assert!(env.sunday_dir().exists());
    let (unknown, error) = sunday.call("app_install", json!({"id": "nope"}), &mut no_questions);
    assert!(error && unknown.contains("no app \"nope\""), "{unknown}");

    let mut other = McpClient::start(&env, "some-coding-agent", true);
    assert!(other.tools().is_empty());
}

#[test]
fn the_per_reply_hook_says_only_what_changed() {
    let env = Env::new();
    env.add_web_app("notes");
    env.add_web_app("wiki");
    let hook = |session: &str, first: bool| -> String {
        let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_openagora"))
            .arg("--catalog")
            .arg(&env.catalog)
            .args(["context", "--hook", "--client", "sunday"])
            .env("HOME", &env.home)
            .env("OPENAGORA_HOME", &env.data)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let payload = json!({
            "hook_event_name": "pre_llm_call", "session_id": session, "profile": "sunday",
            "extra": {"is_first_turn": first, "user_message": "hi"},
        });
        writeln!(child.stdin.take().unwrap(), "{payload}").unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(out.status.success());
        String::from_utf8(out.stdout).unwrap()
    };
    let first = hook("s1", true);
    let context: Value = serde_json::from_str(&first).unwrap();
    let text = context["context"].as_str().unwrap();
    assert!(
        text.contains("data, not instructions") && text.contains("none yet"),
        "{text}"
    );
    // Nothing changed: Hermes gets nothing to add.
    assert_eq!(hook("s1", false), "");

    env.ok(&["install", "notes", "--yes"]);
    let changed: Value = serde_json::from_str(&hook("s1", false)).unwrap();
    assert!(
        changed["context"]
            .as_str()
            .unwrap()
            .contains("Web notes (notes): stopped"),
        "{changed}"
    );
    assert_eq!(hook("s1", false), "");
    // Another session starts with the full picture.
    assert!(hook("s2", true).contains("Web notes"));
}

#[test]
fn connecting_sunday_uses_hermess_own_config_command_on_sundays_profile_only() {
    let env = Env::new();
    env.install_hermes_by_hand();
    env.existing_hermes_setup();
    let before = env.snapshot_outside_sunday();
    env.ok(&["sunday", "setup", "--fresh"]);
    // The user's own hook in Sunday's profile is kept.
    let store = env.sunday_dir().join(".fake-config");
    std::fs::create_dir_all(&store).unwrap();
    std::fs::write(
        store.join("hooks.pre_llm_call"),
        r#"[{"command":"my-hook","timeout":5}]"#,
    )
    .unwrap();
    // A server that came with a copied setup is named, never removed.
    std::fs::write(
        store.join("mcp_servers"),
        r#"{"github":{"command":"gh-mcp"}}"#,
    )
    .unwrap();

    let unconfirmed = env.run(&["sunday", "connect"]);
    assert!(!unconfirmed.status.success(), "connect must ask first");
    assert!(!store.join("mcp_servers.openagora").exists());

    let out = env.ok(&["sunday", "connect", "--yes"]);
    assert!(
        out.contains("MCP servers from your earlier setup: github"),
        "{out}"
    );
    assert!(store.join("mcp_servers").exists());
    let read = |key: &str| -> Value {
        serde_json::from_str(&std::fs::read_to_string(store.join(key)).unwrap()).unwrap()
    };
    let server = read("mcp_servers.openagora");
    let exe = env.data.join("bin/openagora");
    assert_eq!(server["command"], exe.display().to_string());
    assert_eq!(server["args"], json!(["mcp", "--client", "sunday"]));
    assert!(
        exe.is_file(),
        "a stable copy of the engine is what Hermes runs"
    );
    let hooks = read("hooks.pre_llm_call");
    assert_eq!(hooks[0]["command"], "my-hook");
    assert_eq!(
        hooks[1]["command"],
        format!("\"{}\" context --hook --client sunday", exe.display())
    );
    assert_eq!(read("hooks_auto_accept"), json!(true));
    let skills = env.data.join("sunday/skills");
    assert_eq!(
        read("skills.external_dirs"),
        json!([skills.display().to_string()])
    );
    let skill = std::fs::read_to_string(skills.join("openagora/openagora-apps/SKILL.md")).unwrap();
    assert!(skill.contains("requires_tools: [mcp__openagora__store_search]"));
    assert_eq!(env.json(&["sunday", "status", "--json"])["connected"], true);
    // Connecting twice changes nothing more.
    env.ok(&["sunday", "connect", "--yes"]);
    assert_eq!(read("hooks.pre_llm_call").as_array().unwrap().len(), 2);

    // Every config call named Sunday's profile.
    for call in env.hermes_calls().iter().filter(|c| c.contains("config ")) {
        assert!(call.starts_with("-p sunday config "), "{call}");
    }
    assert_eq!(env.snapshot_outside_sunday(), before);

    env.ok(&["sunday", "disconnect", "--yes"]);
    assert!(!store.join("mcp_servers.openagora").exists());
    assert_eq!(
        read("hooks.pre_llm_call"),
        json!([{"command": "my-hook", "timeout": 5}])
    );
    assert!(!store.join("hooks_auto_accept").exists());
    assert_eq!(read("skills.external_dirs"), json!([]));
    assert_eq!(
        env.json(&["sunday", "status", "--json"])["connected"],
        false
    );
}

#[test]
fn the_engine_service_starts_once_and_stops() {
    let env = Env::new();
    let started: Value = serde_json::from_str(&env.ok(&["daemon", "start", "--json"])).unwrap();
    assert!(started["pid"].as_u64().unwrap() > 0);
    assert!(
        started["address"].as_str().unwrap().ends_with(".sock"),
        "{started}"
    );
    assert_eq!(started["address"], env.socket().display().to_string());
    let again: Value = serde_json::from_str(&env.ok(&["daemon", "start", "--json"])).unwrap();
    assert_eq!(
        again["pid"], started["pid"],
        "a second start reuses the running service"
    );
    let serve = env.run(&["serve"]);
    assert!(!serve.status.success(), "a second service refuses to start");
    // A service from another OpenAgora version is replaced.
    let record = env.data.join("run/daemon.toml");
    let text = std::fs::read_to_string(&record).unwrap();
    std::fs::write(&record, text.replace(env!("CARGO_PKG_VERSION"), "0.0.1")).unwrap();
    let replaced: Value = serde_json::from_str(&env.ok(&["daemon", "start", "--json"])).unwrap();
    assert_ne!(replaced["pid"], started["pid"]);
    assert_eq!(replaced["version"], env!("CARGO_PKG_VERSION"));
    assert!(env.ok(&["daemon", "stop"]).contains("Stopped"));
    assert_eq!(env.ok(&["daemon", "status", "--json"]).trim(), "null");
}
