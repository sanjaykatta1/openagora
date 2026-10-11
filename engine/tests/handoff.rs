//! Handing work to coding agents (acceptance test 7, and its rows of test
//! 10). Pull: Sunday sends a screenshot to Claude Code, Codex and OpenCode,
//! each set up once, and each reads it through its own MCP connection to
//! OpenAgora. The agents are stand-ins (`tests/fakes/coding-agent`);
//! OpenAgora's window is a stand-in that captures a fixed picture.
#![cfg(unix)]

mod common;

use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use common::{Desktop, Env, McpClient, Responder};
use serde_json::{Value, json};

fn png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&height.to_be_bytes());
    bytes.extend_from_slice(&[8, 6, 0, 0, 0]);
    bytes
}

fn no_questions(request: &Value) -> Value {
    panic!("unexpected question {request}")
}

fn image_of(result: &Value) -> Option<Vec<u8>> {
    result["content"]
        .as_array()?
        .iter()
        .find(|b| b["type"] == "image")
        .map(|b| BASE64.decode(b["data"].as_str().unwrap()).unwrap())
}

fn desk_id(text: &str) -> String {
    let found = regex::Regex::new(r"d-[0-9a-f]{10}").unwrap();
    found
        .find(text)
        .unwrap_or_else(|| panic!("no desk item in {text}"))
        .as_str()
        .to_string()
}

/// OpenAgora's window: captures `picture`; leaves questions to the test.
fn connect_window(env: &Env, picture: Vec<u8>) -> Desktop {
    let responder: Responder = Box::new(move |request: &Value| {
        if request["method"] != "capture/window" {
            return None;
        }
        Some(json!({
            "type": "image/png", "data": BASE64.encode(&picture), "width": 32, "height": 24,
            "title": "Bug — Mozilla Firefox",
        }))
    });
    Desktop::connect_with(env, json!({"pid": 4242}), Some(responder))
}

/// Claude Code's prompt hook, as Claude Code runs it.
fn claude_prompt_hook(env: &Env) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_openagora"))
        .arg("--catalog")
        .arg(&env.catalog)
        .args([
            "context",
            "--hook",
            "--client",
            "claude-code",
            "--format",
            "claude-code",
        ])
        .env("HOME", &env.home)
        .env("OPENAGORA_HOME", &env.data)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let event = json!({"hook_event_name": "UserPromptSubmit", "session_id": "c1", "prompt": "fix it", "cwd": "/tmp"});
    writeln!(child.stdin.take().unwrap(), "{event}").unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    String::from_utf8(out.stdout).unwrap()
}

/// Sends `items` to `to`, answering OpenAgora's question with `choice`
/// (None: no question expected). Returns (text, is_error).
fn send(
    sunday: &mut McpClient,
    window: &mut Desktop,
    arguments: Value,
    choice: Option<Value>,
) -> (String, bool) {
    match choice {
        None => {
            let result = sunday.call("send_to", arguments, &mut no_questions);
            assert!(window.unanswered().is_empty(), "OpenAgora asked anyway");
            result
        }
        Some(answer) => std::thread::scope(|scope| {
            let call = scope.spawn(|| sunday.call("send_to", arguments, &mut no_questions));
            let ask = window.next();
            assert_eq!(ask["method"], "confirm/ask", "{ask}");
            window.reply(&ask["id"], answer);
            call.join().unwrap()
        }),
    }
}

#[test]
fn sunday_sends_a_screenshot_and_each_coding_agent_reads_it_through_mcp() {
    let mut env = Env::new();
    env.add_fake_agents(&["claude", "codex", "opencode"]);
    let focus = env.data.with_file_name("focus.json");
    env.env("OPENAGORA_FOCUS_FILE", focus.to_str().unwrap());
    fs::write(
        &focus,
        json!([{"id": 31, "pid": 77, "app": "Firefox", "key": "firefox", "title": "Bug — Mozilla Firefox"}])
            .to_string(),
    )
    .unwrap();
    env.ok(&["grants", "allow", "firefox", "--name", "Firefox"]);
    for id in ["claude-code", "codex", "opencode"] {
        env.ok(&["agents", "setup", id, "--yes"]);
    }

    let mut sunday = McpClient::start(&env, "sunday", true);
    for tool in [
        "destinations",
        "send_to",
        "start_task",
        "task_status",
        "task_result",
    ] {
        assert!(sunday.tools().contains(&tool.to_string()), "{tool}");
    }
    let mut window = connect_window(&env, png(32, 24));

    // Sunday sees the user's window, and knows where it can send it.
    let shot = sunday.call_raw("capture_window", json!({}), &mut no_questions);
    let text = shot["content"][0]["text"].as_str().unwrap().to_string();
    let first = desk_id(&text);
    let told = env.hook("s1", true, "cli");
    assert!(
        told.contains("send things to (send_to, start_task): Claude Code (claude-code); Codex (codex); OpenCode (opencode)"),
        "{told}"
    );
    let (listed, _) = sunday.call("destinations", json!({}), &mut no_questions);
    assert!(listed.contains("Claude Code (claude-code)"), "{listed}");

    // "Send this to my coding agent": OpenAgora asks in its own window; the
    // user sends it once.
    let (sent, error) = std::thread::scope(|scope| {
        let call = scope.spawn(|| {
            sunday.call(
                "send_to",
                json!({"to": "claude-code", "items": [first], "message": "This is the error I get"}),
                &mut no_questions,
            )
        });
        let ask = window.next();
        assert_eq!(ask["method"], "confirm/ask", "{ask}");
        assert_eq!(ask["params"]["title"], "Send to Claude Code?");
        let message = ask["params"]["message"].as_str().unwrap();
        assert!(message.contains("screenshot of Firefox"), "{message}");
        let choices: Vec<&str> = ask["params"]["choices"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["id"].as_str().unwrap())
            .collect();
        assert_eq!(choices, ["once", "always"]);
        window.reply(&ask["id"], json!({"allowed": true, "choice": "once"}));
        call.join().unwrap()
    });
    assert!(!error && sent.contains("Sent to Claude Code"), "{sent}");

    // Claude Code's prompt hook tells it with the user's next prompt, once.
    let notice = claude_prompt_hook(&env);
    assert!(
        notice.contains(&first)
            && notice.contains("This is the error I get")
            && notice.contains("context_get"),
        "{notice}"
    );
    assert_eq!(claude_prompt_hook(&env), "");

    // "What did Sunday send me?": Claude Code reads its inbox and sees the
    // picture, as image content without structuredContent.
    let mut claude = McpClient::start(&env, "claude-code", false);
    assert_eq!(
        claude.tools(),
        ["context_inbox", "context_get", "context_search"]
    );
    let (inbox, _) = claude.call("context_inbox", json!({}), &mut no_questions);
    assert!(
        inbox.contains(&first)
            && inbox.contains("screenshot of Firefox")
            && inbox.contains("This is the error I get"),
        "{inbox}"
    );
    let got = claude.call_raw("context_get", json!({"id": first}), &mut no_questions);
    assert_eq!(got["isError"], false, "{got}");
    assert!(got.get("structuredContent").is_none());
    assert_eq!(image_of(&got), Some(png(32, 24)));
    let (found, _) = claude.call(
        "context_search",
        json!({"query": "firefox bug"}),
        &mut no_questions,
    );
    assert!(found.contains(&first), "{found}");
    // Connected: Sunday hears it's ready.
    let (listed, _) = sunday.call("destinations", json!({}), &mut no_questions);
    assert!(listed.contains("Claude Code (claude-code): coding agent, takes pictures, text and files; set up: it reads what you send it"), "{listed}");

    // An agent sees only what it was sent: not the whole desk.
    let mut codex = McpClient::start(&env, "codex", false);
    let (denied, error) = codex.call("context_get", json!({"id": first}), &mut no_questions);
    assert!(error && denied.contains("no item"), "{denied}");
    let (empty, _) = codex.call("context_inbox", json!({}), &mut no_questions);
    assert!(empty.contains("hasn't sent you anything"), "{empty}");

    // Codex and OpenCode the same way, with "always": the next screenshot
    // of Firefox goes without a question.
    let mut opencode = McpClient::start(&env, "opencode", false);
    for (agent, client) in [("codex", &mut codex), ("opencode", &mut opencode)] {
        let (sent, error) = send(
            &mut sunday,
            &mut window,
            json!({"to": agent, "items": [first]}),
            Some(json!({"allowed": true, "choice": "always"})),
        );
        assert!(!error, "{sent}");
        let got = client.call_raw("context_get", json!({"id": first}), &mut no_questions);
        assert_eq!(image_of(&got), Some(png(32, 24)), "{agent}: {got}");
        let shot = sunday.call_raw("capture_window", json!({}), &mut no_questions);
        let next = desk_id(shot["content"][0]["text"].as_str().unwrap());
        let (sent, error) = send(
            &mut sunday,
            &mut window,
            json!({"to": agent, "items": [next]}),
            None,
        );
        assert!(!error && sent.contains("Sent to"), "{sent}");
        let (inbox, _) = client.call("context_inbox", json!({}), &mut no_questions);
        assert!(
            inbox.contains(&next) && inbox.contains(&first),
            "{agent}: {inbox}"
        );
    }

    // A no sends nothing.
    let shot = sunday.call_raw("capture_window", json!({}), &mut no_questions);
    let declined = desk_id(shot["content"][0]["text"].as_str().unwrap());
    let (said, _) = send(
        &mut sunday,
        &mut window,
        json!({"to": "claude-code", "items": [declined]}),
        Some(json!({"allowed": false})),
    );
    assert!(said.contains("didn't want"), "{said}");
    let (inbox, _) = claude.call("context_inbox", json!({}), &mut no_questions);
    assert!(!inbox.contains(&declined), "{inbox}");

    // What comes from an app the user later puts on the never-capture list
    // never leaves, even where sending was always allowed.
    env.ok(&["never", "add", "Browsers", "--app", "Firefox"]);
    let (refused, _) = send(
        &mut sunday,
        &mut window,
        json!({"to": "codex", "items": [declined]}),
        None,
    );
    assert!(refused.contains("never-capture list"), "{refused}");

    // Unknown destinations and items Sunday can't see are refused.
    let (unknown, error) = sunday.call(
        "send_to",
        json!({"to": "cursor", "items": [first]}),
        &mut no_questions,
    );
    assert!(error && unknown.contains("no destination"), "{unknown}");
    let (missing, error) = sunday.call(
        "send_to",
        json!({"to": "codex", "items": ["d-0000000000"]}),
        &mut no_questions,
    );
    assert!(error && missing.contains("no desk item"), "{missing}");

    // Every handoff and every read is in the audit log (test 10).
    let audit = env.audit();
    let rows = |action: &str, decision: &str| -> Vec<String> {
        audit
            .iter()
            .filter(|e| e["action"] == action && e["decision"] == decision)
            .map(|e| e["target"].as_str().unwrap().to_string())
            .collect()
    };
    assert!(
        rows("handoff", "confirmed").contains(&format!("claude-code:{first}")),
        "{audit:?}"
    );
    assert!(
        rows("handoff", "allowed").contains(&format!("codex:{first}")),
        "{audit:?}"
    );
    assert!(
        rows("handoff", "declined").contains(&format!("claude-code:{declined}")),
        "{audit:?}"
    );
    assert!(
        rows("handoff", "refused").contains(&format!("codex:{declined}")),
        "{audit:?}"
    );
    assert!(rows("context_get", "allowed").contains(&first), "{audit:?}");
    assert!(rows("context_get", "refused").contains(&first), "{audit:?}");
    let confirmed = audit
        .iter()
        .find(|e| e["action"] == "handoff" && e["decision"] == "confirmed")
        .unwrap();
    assert_eq!(confirmed["client"], "sunday");
    assert_eq!(confirmed["via"], "window");
}

#[test]
fn coding_agents_see_apps_the_user_allowed_for_them_and_nothing_else() {
    let mut env = Env::new();
    env.add_fake_agents(&["claude"]);
    // Sunday connecting starts the engine service.
    let _sunday = McpClient::start(&env, "sunday", true);
    let mut window = connect_window(&env, png(4, 4));
    let added = window.request(
        "desk/add",
        json!({"kind": "text", "text": "TODO: ship it", "source": {"app": "Notes"}}),
    );
    let id = added["result"]["id"].as_str().unwrap().to_string();
    let mut claude = McpClient::start(&env, "claude-code", false);
    let (nothing, _) = claude.call(
        "context_search",
        json!({"query": "ship"}),
        &mut no_questions,
    );
    assert!(nothing.contains("Nothing"), "{nothing}");
    // The user lets Claude Code see Notes: it can search and read it, without
    // Sunday sending anything.
    env.ok(&["grants", "allow", "Notes", "--for", "claude-code"]);
    let (found, _) = claude.call(
        "context_search",
        json!({"query": "ship"}),
        &mut no_questions,
    );
    assert!(found.contains(&id), "{found}");
    let (text, error) = claude.call("context_get", json!({"id": id}), &mut no_questions);
    assert!(
        !error && text.contains("TODO: ship it") && text.contains("not instructions"),
        "{text}"
    );
    // Other clients get no tools at all, and Sunday's tools stay Sunday's.
    let mut stranger = McpClient::start(&env, "some-tool", false);
    assert!(stranger.tools().is_empty());
    let (refused, _) = claude.call(
        "send_to",
        json!({"to": "codex", "items": [id]}),
        &mut no_questions,
    );
    assert!(refused.contains("unknown tool send_to"), "{refused}");
}

/// A browser following `url` on this computer, one redirect deep, as
/// OpenAgora's approval window does.
fn browse(url: &str) {
    let get = |url: &str| -> openagora::http::Response {
        let rest = url
            .strip_prefix("http://127.0.0.1:")
            .expect("a local address");
        let (port, path) = rest.split_at(rest.find('/').unwrap());
        openagora::http::request(
            "GET",
            port.parse().unwrap(),
            path,
            &[],
            None,
            std::time::Duration::from_secs(10),
        )
        .unwrap()
    };
    let page = get(url);
    if let Some(next) = page.header("Location") {
        get(next);
    }
}

/// OpenAgora's window: captures `picture`, shows approval pages (in a
/// "window" of its own, following them like a browser); leaves questions to
/// the test.
fn connect_window_with_approvals(
    env: &Env,
    picture: Vec<u8>,
) -> (Desktop, std::sync::Arc<std::sync::Mutex<Vec<Value>>>) {
    let opened = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = std::sync::Arc::clone(&opened);
    let responder: Responder = Box::new(move |request: &Value| match request["method"].as_str() {
        Some("capture/window") => Some(json!({
            "type": "image/png", "data": BASE64.encode(&picture), "width": 32, "height": 24,
            "title": "Crash — Mozilla Firefox",
        })),
        Some("oauth/open") => {
            seen.lock().unwrap().push(request["params"].clone());
            let url = request["params"]["url"].as_str().unwrap().to_string();
            std::thread::spawn(move || browse(&url));
            Some(json!({"opened": true}))
        }
        _ => None,
    });
    (
        Desktop::connect_with(env, json!({"pid": 4242}), Some(responder)),
        opened,
    )
}

/// Answers one question from OpenAgora's window while `call` runs.
fn answering<T: Send>(
    window: &mut Desktop,
    title: &str,
    answer: Value,
    call: impl FnOnce() -> T + Send,
) -> T {
    std::thread::scope(|scope| {
        let running = scope.spawn(call);
        let ask = window.next();
        assert_eq!(ask["method"], "confirm/ask", "{ask}");
        assert_eq!(ask["params"]["title"], title, "{ask}");
        window.reply(&ask["id"], answer);
        running.join().unwrap()
    })
}

#[test]
fn sunday_starts_a_t3_code_thread_with_the_screenshot_and_reports_back() {
    let t3 = common::fake_t3::FakeT3::start();
    let mut env = Env::new();
    env.add_running_t3code(t3.port);
    let focus = env.data.with_file_name("focus.json");
    env.env("OPENAGORA_FOCUS_FILE", focus.to_str().unwrap());
    fs::write(
        &focus,
        json!([{"id": 31, "pid": 77, "app": "Firefox", "key": "firefox", "title": "Crash — Mozilla Firefox"}]).to_string(),
    )
    .unwrap();
    env.ok(&["grants", "allow", "firefox", "--name", "Firefox"]);
    let mut sunday = McpClient::start(&env, "sunday", true);
    let (mut window, opened) = connect_window_with_approvals(&env, png(32, 24));
    let shot = sunday.call_raw("capture_window", json!({}), &mut no_questions);
    let id = desk_id(shot["content"][0]["text"].as_str().unwrap());

    // Not connected yet: Sunday is told where the user connects it.
    let (text, error) = sunday.call(
        "start_task",
        json!({"to": "t3code", "prompt": "Fix this crash", "items": [id]}),
        &mut no_questions,
    );
    assert!(error && text.contains("Settings → Coding agents"), "{text}");
    assert!(!sunday.tools().iter().any(|t| t.starts_with("t3code__")));

    // The user connects T3 Code in OpenAgora's Settings: T3's own approval
    // page opens in OpenAgora's window, with T3's own session, and the user
    // approves OpenAgora there.
    let connected = window.request("connections/connect", json!({"app": "t3code"}));
    assert_eq!(connected["result"]["connected"], true, "{connected}");
    assert_eq!(connected["result"]["can_act"], true);
    {
        let opened = opened.lock().unwrap();
        assert_eq!(opened[0]["app"], "t3code");
        let url = opened[0]["url"].as_str().unwrap();
        assert!(
            url.starts_with(&format!(
                "http://127.0.0.1:{}/oauth/mcp/authorize?",
                t3.port
            )),
            "{url}"
        );
        assert!(url.contains("code_challenge_method=S256"), "{url}");
    }
    // The grant is the engine's, in a file only this user can open; the
    // listing never shows the token.
    use std::os::unix::fs::PermissionsExt;
    let grant = env.data.join("connections/t3code.toml");
    assert_eq!(
        fs::metadata(&grant).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(fs::read_to_string(&grant).unwrap().contains("token-1"));
    let listed = env.ok(&["connections", "--json"]);
    assert!(
        listed.contains("\"usable\": true") && !listed.contains("token-1"),
        "{listed}"
    );
    let told = env.hook("s1", true, "cli");
    assert!(told.contains("T3 Code (t3code)"), "{told}");

    // T3's own tools come through for Sunday, except uploads (the engine
    // moves bytes) and tools only T3's own threads can use.
    let tools = sunday.tools();
    for tool in [
        "t3code__t3_thread_list",
        "t3code__t3_thread_read",
        "t3code__t3_thread_send",
    ] {
        assert!(tools.contains(&tool.to_string()), "{tool}: {tools:?}");
    }
    assert!(
        !tools
            .iter()
            .any(|t| t.contains("attachment") || t.contains("create_threads")),
        "{tools:?}"
    );

    // "Start a T3 Code thread with this": OpenAgora asks first, uploads the
    // picture itself, and launches the thread with it attached.
    let (started, error) = answering(
        &mut window,
        "Start a task in T3 Code?",
        json!({"allowed": true, "choice": "once"}),
        || {
            sunday.call(
                "start_task",
                json!({"to": "t3code", "prompt": "Fix the crash in this screenshot", "items": [id], "project": "openagora", "title": "Fix the crash"}),
                &mut no_questions,
            )
        },
    );
    assert!(!error, "{started}");
    assert!(
        started.contains("[Fix the crash](t3-thread://v1/thread-1)"),
        "{started}"
    );
    let task = regex::Regex::new(r"t-[0-9a-f]{10}")
        .unwrap()
        .find(&started)
        .unwrap()
        .as_str()
        .to_string();
    {
        let state = t3.state.lock().unwrap();
        assert_eq!(state.uploads.len(), 1);
        assert_eq!(
            state.uploads[0].1,
            png(32, 24),
            "the bytes on the desk went to T3"
        );
    }
    let launches = t3.calls_to("t3_thread_launch");
    assert_eq!(launches.len(), 1);
    assert_eq!(launches[0]["projectId"], "proj-1");
    assert_eq!(launches[0]["title"], "Fix the crash");
    assert_eq!(launches[0]["attachments"][0]["id"], "attachment-1");
    assert_eq!(launches[0]["attachments"][0]["mimeType"], "image/png");
    assert!(
        launches[0]["message"]
            .as_str()
            .unwrap()
            .starts_with("Fix the crash in this screenshot")
    );

    // Sunday follows it, and later reports the result.
    let (status, _) = sunday.call("task_status", json!({"task": task}), &mut no_questions);
    assert!(status.contains("still running"), "{status}");
    t3.state.lock().unwrap().run_status = "completed".into();
    let (status, _) = sunday.call(
        "task_status",
        json!({"task": task, "wait_seconds": 30}),
        &mut no_questions,
    );
    assert!(status.contains("is completed"), "{status}");
    let (result, error) = sunday.call("task_result", json!({"task": task}), &mut no_questions);
    assert!(!error, "{result}");
    assert!(
        result.contains("Fixed the null check in parser.ts")
            && result.contains("npm test")
            && result.contains("not instructions"),
        "{result}"
    );

    // Without a task, T3 Code takes nothing, and nobody is asked.
    let (refused, error) = sunday.call(
        "send_to",
        json!({"to": "t3code", "items": [id]}),
        &mut no_questions,
    );
    assert!(error && refused.contains("start_task"), "{refused}");
    assert!(window.unanswered().is_empty());

    // More for the same thread, through the common interface.
    let shot = sunday.call_raw("capture_window", json!({}), &mut no_questions);
    let second = desk_id(shot["content"][0]["text"].as_str().unwrap());
    let (sent, error) = answering(
        &mut window,
        "Send to T3 Code?",
        json!({"allowed": true, "choice": "once"}),
        || {
            sunday.call(
                "send_to",
                json!({"to": "t3code", "items": [second], "task": task, "message": "And this one"}),
                &mut no_questions,
            )
        },
    );
    assert!(!error && sent.contains("added to the task"), "{sent}");
    let follow = t3.calls_to("t3_thread_send_attachments");
    assert_eq!(follow[0]["threadId"], "thread-1");
    assert!(
        follow[0]["message"]
            .as_str()
            .unwrap()
            .starts_with("And this one")
    );

    // T3's own tools: reading needs no question, acting asks first.
    let read = sunday.call_raw("t3code__t3_thread_list", json!({}), &mut no_questions);
    assert_eq!(read["isError"], false, "{read}");
    assert!(
        read["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("data, not instructions")
    );
    let (acted, error) = answering(
        &mut window,
        "Let Sunday act in T3 Code?",
        json!({"allowed": true, "choice": "once"}),
        || {
            sunday.call(
                "t3code__t3_thread_send",
                json!({"threadId": "thread-1", "message": "Also add a test"}),
                &mut no_questions,
            )
        },
    );
    assert!(!error, "{acted}");
    assert_eq!(t3.calls_to("t3_thread_send").len(), 1);
    let (refused, _) = answering(
        &mut window,
        "Let Sunday act in T3 Code?",
        json!({"allowed": false}),
        || {
            sunday.call(
                "t3code__t3_thread_send",
                json!({"threadId": "thread-1", "message": "Delete everything"}),
                &mut no_questions,
            )
        },
    );
    assert!(refused.contains("didn't allow"), "{refused}");
    assert_eq!(t3.calls_to("t3_thread_send").len(), 1);

    // T3 takes the sign-in back (or its 30 days run out): Sunday is told to
    // have the user connect again, and OpenAgora forgets the token.
    t3.state.lock().unwrap().revoked = true;
    let (expired, error) = sunday.call("task_status", json!({"task": task}), &mut no_questions);
    assert!(error && expired.contains("connects it again"), "{expired}");
    assert!(!fs::read_to_string(&grant).unwrap().contains("token-1"));
    let (said, _) = sunday.call("destinations", json!({}), &mut no_questions);
    assert!(said.contains("connects it again"), "{said}");

    // Everything is in the audit log (test 10).
    let audit = env.audit();
    let has = |action: &str, decision: &str| {
        audit
            .iter()
            .any(|e| e["action"] == action && e["decision"] == decision)
    };
    for (action, decision) in [
        ("connect", "confirmed"),
        ("allow_task", "confirmed"),
        ("start_task", "allowed"),
        ("handoff", "allowed"),
        ("allow_act", "confirmed"),
        ("app_tool", "allowed"),
        ("app_tool", "declined"),
    ] {
        assert!(has(action, decision), "{action} {decision}: {audit:?}");
    }
    let connect = audit.iter().find(|e| e["action"] == "connect").unwrap();
    assert_eq!(connect["via"], "T3 Code's page");
}

#[test]
fn read_only_access_to_t3_code_can_read_but_not_start_threads() {
    let t3 = common::fake_t3::FakeT3::start();
    t3.state.lock().unwrap().scope = "orchestration:read".into();
    let env = Env::new();
    env.add_running_t3code(t3.port);
    let _sunday = McpClient::start(&env, "sunday", true);
    let (mut window, _) = connect_window_with_approvals(&env, png(2, 2));
    let connected = window.request("connections/connect", json!({"app": "t3code"}));
    assert_eq!(connected["result"]["can_act"], false, "{connected}");
    // Saying no on T3's page connects nothing.
    t3.state.lock().unwrap().deny = true;
    env.ok(&["disconnect", "t3code"]);
    let refused = window.request("connections/connect", json!({"app": "t3code"}));
    assert!(
        refused["error"]["message"]
            .as_str()
            .unwrap()
            .contains("refused"),
        "{refused}"
    );
    assert!(!env.data.join("connections/t3code.toml").exists());
    let audit = env.audit();
    assert!(
        audit
            .iter()
            .any(|e| e["action"] == "connect" && e["decision"] == "declined"),
        "{audit:?}"
    );
}

#[test]
fn the_command_line_connects_t3_code_through_the_browser() {
    let t3 = common::fake_t3::FakeT3::start();
    let mut env = Env::new();
    env.add_running_t3code(t3.port);
    // The user's browser: follows the approval page's redirect back.
    let browser = env.data.with_file_name("browser.sh");
    fs::write(&browser, "#!/bin/sh\ncurl -sL \"$1\" > /dev/null\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&browser, fs::Permissions::from_mode(0o755)).unwrap();
    env.env("BROWSER", browser.to_str().unwrap());
    let out = env.ok(&["connect", "t3code"]);
    assert!(
        out.contains("/oauth/mcp/authorize?") && out.contains("OpenAgora is connected to T3 Code"),
        "{out}"
    );
    assert!(env.ok(&["connections"]).contains("t3code     connected"));
    assert!(env.ok(&["disconnect", "t3code"]).contains("forgot"));
    assert!(env.ok(&["connections"]).contains("isn't connected"));
}
