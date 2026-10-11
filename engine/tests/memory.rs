//! Sunday's memory across coding agents (acceptance test 11): "what did I do
//! in my coding sessions today?" is answered from the agents' own records,
//! read only when the user lets Sunday read them. The records are stand-ins
//! written the way Claude Code, Codex (both of its formats), OpenCode (1 and
//! 2, in one SQLite file) and T3 Code (its MCP tools) keep them.
#![cfg(unix)]

mod common;

use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use common::fake_t3::{FakeT3, iso_ago};
use common::{Desktop, Env, McpClient, Responder};
use serde_json::{Value, json};

const DAY: u64 = 86_400;

fn no_questions(request: &Value) -> Value {
    panic!("unexpected question {request}")
}

fn write_lines(path: &Path, lines: &[Value]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let text: Vec<String> = lines.iter().map(Value::to_string).collect();
    fs::write(path, text.join("\n") + "\n").unwrap();
}

fn age(path: &Path, seconds: u64) {
    let file = fs::File::options().write(true).open(path).unwrap();
    file.set_modified(SystemTime::now() - Duration::from_secs(seconds))
        .unwrap();
}

/// Claude Code's transcripts: today's session, with what must be left out
/// (tool results, a subagent's side chain, Claude Code's own notes), and an
/// old one.
fn claude_sessions(home: &Path) {
    let projects = home.join(".claude/projects/-home-u-code-app");
    let today = projects.join("5f0c1d2e-aaaa-4bbb-8ccc-123456789abc.jsonl");
    write_lines(
        &today,
        &[
            json!({"type": "summary", "summary": "Fix the flaky parser test", "leafUuid": "x"}),
            json!({"type": "user", "cwd": "/home/u/code/app", "timestamp": iso_ago(7000), "message": {"role": "user", "content": "The parser test fails on CI"}}),
            json!({"type": "user", "isMeta": true, "timestamp": iso_ago(6990), "message": {"role": "user", "content": "<local-command-caveat>internal</local-command-caveat>"}}),
            json!({"type": "assistant", "timestamp": iso_ago(6900), "message": {"role": "assistant", "content": [{"type": "text", "text": "Let me look."}, {"type": "tool_use", "id": "t1", "name": "Read", "input": {}}]}}),
            json!({"type": "user", "timestamp": iso_ago(6800), "message": {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "t1", "content": "TOOL OUTPUT SHOULD NOT SHOW"}]}}),
            json!({"type": "user", "isSidechain": true, "timestamp": iso_ago(6700), "message": {"role": "user", "content": "SIDECHAIN SHOULD NOT SHOW"}}),
            json!({"type": "assistant", "timestamp": iso_ago(6600), "message": {"role": "assistant", "content": [{"type": "text", "text": "Fixed: the test now waits for the parser."}]}}),
            json!({"type": "user", "timestamp": iso_ago(6500), "message": {"role": "user", "content": "We decided to keep pnpm"}}),
            json!({"type": "assistant", "timestamp": iso_ago(6400), "message": {"role": "assistant", "content": [{"type": "text", "text": "Noted: pnpm stays."}]}}),
        ],
    );
    let old = projects.join("0ld00000-aaaa-4bbb-8ccc-123456789abc.jsonl");
    write_lines(
        &old,
        &[
            json!({"type": "user", "cwd": "/home/u/code/app", "timestamp": iso_ago(3 * DAY), "message": {"role": "user", "content": "OLD CLAUDE SESSION"}}),
        ],
    );
    age(&old, 3 * DAY);
}

/// Codex's rollouts: one in its older format, one in its newer, a name in its
/// index, and an old one.
fn codex_sessions(home: &Path) {
    let day = home.join(".codex/sessions/2026/10/11");
    write_lines(
        &day.join("rollout-2026-10-11T09-00-00-legacy-1111.jsonl"),
        &[
            json!({"timestamp": iso_ago(5000), "type": "session_meta", "payload": {"id": "legacy-1111", "timestamp": iso_ago(5000), "cwd": "/home/u/code/api"}}),
            json!({"timestamp": iso_ago(4990), "type": "response_item", "payload": {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "<environment_context>NOT A PROMPT</environment_context>"}]}}),
            json!({"timestamp": iso_ago(4980), "type": "event_msg", "payload": {"type": "user_message", "message": "Add rate limiting to the API"}}),
            json!({"timestamp": iso_ago(4900), "type": "event_msg", "payload": {"type": "agent_message", "message": "COMMENTARY SHOULD NOT SHOW", "phase": "commentary"}}),
            json!({"timestamp": iso_ago(4800), "type": "event_msg", "payload": {"type": "agent_message", "message": "Added a token bucket limiter.", "phase": "final_answer"}}),
            json!({"timestamp": iso_ago(4790), "type": "event_msg", "payload": {"type": "task_complete", "last_agent_message": "Added a token bucket limiter."}}),
        ],
    );
    write_lines(
        &day.join("rollout-2026-10-11T10-00-00-paged-2222.jsonl"),
        &[
            json!({"timestamp": iso_ago(4000), "ordinal": 0, "type": "session_meta", "payload": {"id": "paged-2222", "timestamp": iso_ago(4000), "cwd": "/home/u/code/web", "history_mode": "paginated"}}),
            json!({"timestamp": iso_ago(3990), "ordinal": 1, "type": "event_msg", "payload": {"type": "item_completed", "item": {"type": "UserMessage", "content": [{"type": "text", "text": "Make the header sticky", "text_elements": []}]}}}),
            json!({"timestamp": iso_ago(3900), "ordinal": 2, "type": "event_msg", "payload": {"type": "item_completed", "item": {"type": "AgentMessage", "content": [{"type": "Text", "text": "Done: the header is position: sticky."}], "phase": "final_answer"}}}),
        ],
    );
    write_lines(
        &home.join(".codex/session_index.jsonl"),
        &[
            json!({"id": "paged-2222", "thread_name": "Header", "updated_at": "x"}),
            json!({"id": "paged-2222", "thread_name": "Sticky header", "updated_at": "y"}),
        ],
    );
    let old = home.join(".codex/sessions/2026/10/01/rollout-2026-10-01T09-00-00-old-3333.jsonl");
    write_lines(
        &old,
        &[
            json!({"timestamp": iso_ago(10 * DAY), "type": "session_meta", "payload": {"id": "old-3333", "timestamp": iso_ago(10 * DAY), "cwd": "/home/u"}}),
            json!({"timestamp": iso_ago(10 * DAY), "type": "event_msg", "payload": {"type": "user_message", "message": "OLD CODEX SESSION"}}),
        ],
    );
    age(&old, 10 * DAY);
}

/// OpenCode's database, as OpenCode 1 and 2 share it: a session only in
/// OpenCode 1's tables, one copied to OpenCode 2's and then carried on in
/// OpenCode 1, one only in OpenCode 2's, a subagent's and an old one.
fn opencode_sessions(home: &Path) {
    let dir = home.join(".local/share/opencode");
    fs::create_dir_all(&dir).unwrap();
    let db = rusqlite::Connection::open(dir.join("opencode.db")).unwrap();
    let now = openagora::state::now() as i64 * 1000;
    let ago = |seconds: i64| now - seconds * 1000;
    db.execute_batch(
        "PRAGMA journal_mode=WAL;
         CREATE TABLE session (id TEXT PRIMARY KEY, project_id TEXT, parent_id TEXT, slug TEXT, directory TEXT, title TEXT NOT NULL, version TEXT, time_created INTEGER, time_updated INTEGER);
         CREATE TABLE message (id TEXT PRIMARY KEY, session_id TEXT, time_created INTEGER, time_updated INTEGER, data TEXT NOT NULL);
         CREATE TABLE part (id TEXT PRIMARY KEY, message_id TEXT, session_id TEXT, time_created INTEGER, time_updated INTEGER, data TEXT NOT NULL);
         CREATE TABLE session_v2 (id TEXT PRIMARY KEY, project_id TEXT, parent_id TEXT, slug TEXT, directory TEXT, title TEXT, version TEXT, time_created INTEGER, time_updated INTEGER);
         CREATE TABLE session_message (id TEXT PRIMARY KEY, session_id TEXT, type TEXT, seq INTEGER, time_created INTEGER, time_updated INTEGER, data TEXT NOT NULL);",
    )
    .unwrap();
    let session = |table: &str,
                   id: &str,
                   parent: Option<&str>,
                   title: Option<&str>,
                   created: i64,
                   updated: i64| {
        db.execute(
            &format!("INSERT INTO {table} (id, project_id, parent_id, slug, directory, title, version, time_created, time_updated) VALUES (?1, 'p', ?2, 's', '/home/u/code/cli', ?3, '1', ?4, ?5)"),
            rusqlite::params![id, parent, title, created, updated],
        )
        .unwrap();
    };
    let message = |id: &str, session: &str, at: i64, data: Value, parts: &[Value]| {
        db.execute(
            "INSERT INTO message VALUES (?1, ?2, ?3, ?3, ?4)",
            rusqlite::params![id, session, at, data.to_string()],
        )
        .unwrap();
        for (n, part) in parts.iter().enumerate() {
            db.execute(
                "INSERT INTO part VALUES (?1, ?2, ?3, ?4, ?4, ?5)",
                rusqlite::params![format!("{id}-p{n}"), id, session, at, part.to_string()],
            )
            .unwrap();
        }
    };
    let v2_message = |id: &str, session: &str, seq: i64, kind: &str, at: i64, data: Value| {
        db.execute(
            "INSERT INTO session_message VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6)",
            rusqlite::params![id, session, kind, seq, at, data.to_string()],
        )
        .unwrap();
    };
    let user = json!({"role": "user", "time": {"created": 0}});
    let assistant = json!({"role": "assistant", "time": {"created": 0}});

    // Only in OpenCode 1, still with its placeholder title.
    session(
        "session",
        "ses_a",
        None,
        Some("New session - 2026-10-11T09:00:00.000Z"),
        ago(6000),
        ago(5500),
    );
    message(
        "msg_a1",
        "ses_a",
        ago(6000),
        user.clone(),
        &[
            json!({"type": "text", "text": "Rename the verbose flag"}),
            json!({"type": "text", "text": "SYNTHETIC SHOULD NOT SHOW", "synthetic": true}),
        ],
    );
    message(
        "msg_a2",
        "ses_a",
        ago(5900),
        assistant.clone(),
        &[
            json!({"type": "tool", "tool": "edit", "state": {"output": "TOOL SHOULD NOT SHOW"}}),
            json!({"type": "text", "text": "Renamed --verbose to --debug."}),
        ],
    );
    // A subagent's session under it.
    session(
        "session",
        "ses_child",
        Some("ses_a"),
        Some("Subagent"),
        ago(5800),
        ago(5700),
    );
    message(
        "msg_c1",
        "ses_child",
        ago(5800),
        user.clone(),
        &[json!({"type": "text", "text": "CHILD SHOULD NOT SHOW"})],
    );
    // Copied to OpenCode 2 earlier, then carried on in OpenCode 1.
    session(
        "session_v2",
        "ses_b",
        None,
        Some("Docs"),
        ago(9000),
        ago(8000),
    );
    v2_message(
        "m_b1",
        "ses_b",
        1,
        "user",
        ago(9000),
        json!({"time": {"created": 0}, "text": "Write the README"}),
    );
    session("session", "ses_b", None, Some("Docs"), ago(9000), ago(2000));
    message(
        "msg_b1",
        "ses_b",
        ago(9000),
        user.clone(),
        &[json!({"type": "text", "text": "Write the README"})],
    );
    message(
        "msg_b2",
        "ses_b",
        ago(2100),
        user.clone(),
        &[json!({"type": "text", "text": "Also update the docs site"})],
    );
    message(
        "msg_b3",
        "ses_b",
        ago(2000),
        assistant.clone(),
        &[json!({"type": "text", "text": "Both are updated."})],
    );
    // Only in OpenCode 2.
    session(
        "session_v2",
        "ses_c",
        None,
        Some("Speed up the build"),
        ago(3000),
        ago(2500),
    );
    v2_message(
        "m_c1",
        "ses_c",
        1,
        "user",
        ago(3000),
        json!({"time": {"created": 0}, "text": "Make the build faster"}),
    );
    v2_message(
        "m_c2",
        "ses_c",
        2,
        "assistant",
        ago(2500),
        json!({"time": {"created": 0}, "content": [{"type": "reasoning", "text": "REASONING SHOULD NOT SHOW"}, {"type": "text", "text": "Cached the dependencies."}]}),
    );
    // An old one.
    session(
        "session",
        "ses_old",
        None,
        Some("Old"),
        ago(5 * DAY as i64),
        ago(5 * DAY as i64),
    );
    message(
        "msg_o1",
        "ses_old",
        ago(5 * DAY as i64),
        user,
        &[json!({"type": "text", "text": "OLD OPENCODE SESSION"})],
    );
}

/// Connects OpenAgora to the stand-in T3 Code through the "browser".
fn connect_t3(env: &mut Env, t3: &FakeT3) {
    env.add_running_t3code(t3.port);
    let browser = env.data.with_file_name("browser.sh");
    fs::write(&browser, "#!/bin/sh\ncurl -sL \"$1\" > /dev/null\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&browser, fs::Permissions::from_mode(0o755)).unwrap();
    env.env("BROWSER", browser.to_str().unwrap());
    env.ok(&["connect", "t3code"]);
}

/// OpenAgora's window, answering "may Sunday read …?" with `answers` (by the
/// agent's name in the title), and noting each question.
fn window_answering(
    env: &Env,
    answers: &[(&'static str, &'static str)],
) -> (Desktop, Arc<Mutex<Vec<String>>>) {
    let asked = Arc::new(Mutex::new(Vec::new()));
    let noted = Arc::clone(&asked);
    let answers = answers.to_vec();
    let responder: Responder = Box::new(move |request: &Value| {
        if request["method"] != "confirm/ask" {
            return None;
        }
        let title = request["params"]["title"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        noted.lock().unwrap().push(title.clone());
        let answer = answers
            .iter()
            .find(|(agent, _)| title.contains(agent))
            .map(|(_, answer)| *answer)
            .unwrap_or("no");
        Some(if answer == "no" {
            json!({"allowed": false})
        } else {
            json!({"allowed": true, "choice": answer})
        })
    });
    (
        Desktop::connect_with(env, json!({"pid": 4242}), Some(responder)),
        asked,
    )
}

fn activity(sunday: &mut McpClient, arguments: Value) -> String {
    let (text, error) = sunday.call("coding_activity", arguments, &mut no_questions);
    assert!(!error, "{text}");
    text
}

#[test]
fn sunday_answers_what_the_user_did_in_their_coding_sessions() {
    let t3 = FakeT3::start();
    let mut env = Env::new();
    let home = env.home.clone();
    claude_sessions(&home);
    codex_sessions(&home);
    opencode_sessions(&home);
    // Where each agent keeps its records, whatever this machine's own
    // settings say.
    env.env("CLAUDE_CONFIG_DIR", home.join(".claude").to_str().unwrap());
    env.env("CODEX_HOME", home.join(".codex").to_str().unwrap());
    env.env("XDG_DATA_HOME", home.join(".local/share").to_str().unwrap());
    env.env("OPENCODE_DB", "");
    connect_t3(&mut env, &t3);
    let since = iso_ago(3 * 3600);

    // The user's own command line lists them all, without asking.
    let listed = env.json(&["sessions", "list", "--since", &since, "--json"]);
    let ids: Vec<&str> = listed
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    for id in [
        "5f0c1d2e-aaaa-4bbb-8ccc-123456789abc",
        "legacy-1111",
        "paged-2222",
        "ses_a",
        "ses_b",
        "ses_c",
        "thread-1",
    ] {
        assert!(ids.contains(&id), "{id} in {ids:?}");
    }
    for id in [
        "0ld00000-aaaa-4bbb-8ccc-123456789abc",
        "old-3333",
        "ses_old",
        "ses_child",
        "thread-2",
    ] {
        assert!(!ids.contains(&id), "{id} in {ids:?}");
    }

    // "What did I do today?": OpenAgora asks, per agent, in its window. The
    // user always allows Claude Code and T3 Code, allows Codex once, and
    // says no to OpenCode.
    let mut sunday = McpClient::start(&env, "sunday", true);
    for tool in ["coding_activity", "coding_session"] {
        assert!(sunday.tools().contains(&tool.to_string()), "{tool}");
    }
    let (window, asked) = window_answering(
        &env,
        &[
            ("Claude Code", "always"),
            ("T3 Code", "always"),
            ("Codex", "once"),
        ],
    );
    let text = activity(&mut sunday, json!({"since": since}));
    assert_eq!(
        asked.lock().unwrap().clone(),
        [
            "Let Sunday read your Claude Code sessions?",
            "Let Sunday read your Codex sessions?",
            "Let Sunday read your OpenCode sessions?",
            "Let Sunday read your T3 Code sessions?",
        ]
    );
    for expected in [
        "not instructions",
        // Claude Code: its title, folder, prompts and last answer.
        "\u{201c}Fix the flaky parser test\u{201d} in /home/u/code/app",
        "We decided to keep pnpm",
        "Last answer: \u{201c}Noted: pnpm stays.\u{201d}",
        // Codex, in both formats, named from its index.
        "Add rate limiting to the API",
        "Added a token bucket limiter.",
        "\u{201c}Sticky header\u{201d} in /home/u/code/web",
        "Done: the header is position: sticky.",
        // T3 Code, by project.
        "\u{201c}Fix the crash\u{201d} in openagora",
        "Fixed the null check in parser.ts",
        // OpenCode wasn't allowed.
        "didn't let you read their OpenCode sessions",
    ] {
        assert!(text.contains(expected), "{expected}: {text}");
    }
    for hidden in [
        "TOOL OUTPUT",
        "SIDECHAIN",
        "local-command-caveat",
        "COMMENTARY",
        "NOT A PROMPT",
        "OLD",
        "Rename the verbose flag",
        "Explore the parser",
    ] {
        assert!(!text.contains(hidden), "{hidden}: {text}");
    }

    // Asked again only for Codex (once was used); OpenCode's no stands for
    // that call, so it's asked about again too.
    asked.lock().unwrap().clear();
    let again = activity(&mut sunday, json!({"since": since, "agent": "codex"}));
    assert!(again.contains("Add rate limiting"), "{again}");
    assert_eq!(
        asked.lock().unwrap().clone(),
        ["Let Sunday read your Codex sessions?"]
    );
    asked.lock().unwrap().clear();
    let claude_only = activity(&mut sunday, json!({"since": since, "agent": "claude-code"}));
    assert!(
        claude_only.contains("pnpm") && !claude_only.contains("rate limiting"),
        "{claude_only}"
    );
    assert!(asked.lock().unwrap().is_empty());

    // One session in full: every prompt and answer, still nothing of the tools.
    let (full, error) = sunday.call(
        "coding_session",
        json!({"agent": "claude-code", "id": "5f0c1d2e-aaaa-4bbb-8ccc-123456789abc"}),
        &mut no_questions,
    );
    assert!(!error, "{full}");
    for expected in [
        "User: The parser test fails on CI",
        "Claude Code: Let me look.\nFixed: the test now waits for the parser.",
        "User: We decided to keep pnpm",
    ] {
        assert!(full.contains(expected), "{expected}: {full}");
    }
    assert!(
        !full.contains("TOOL OUTPUT") && !full.contains("SIDECHAIN"),
        "{full}"
    );
    let (missing, error) = sunday.call(
        "coding_session",
        json!({"agent": "claude-code", "id": "../../etc/passwd"}),
        &mut no_questions,
    );
    assert!(error && missing.contains("no session"), "{missing}");

    // The user lets Sunday always read OpenCode from the command line: the
    // newer copy of a session wins, a placeholder title gives way to the
    // first prompt, and subagents' sessions stay out.
    env.ok(&["sessions", "allow", "opencode"]);
    asked.lock().unwrap().clear();
    let opencode = activity(&mut sunday, json!({"since": since, "agent": "opencode"}));
    assert!(asked.lock().unwrap().is_empty());
    for expected in [
        "\u{201c}Rename the verbose flag\u{201d} in /home/u/code/cli",
        "Renamed --verbose to --debug.",
        "\u{201c}Docs\u{201d}",
        "Also update the docs site",
        "Both are updated.",
        "\u{201c}Speed up the build\u{201d}",
        "Cached the dependencies.",
    ] {
        assert!(opencode.contains(expected), "{expected}: {opencode}");
    }
    for hidden in [
        "SYNTHETIC",
        "TOOL SHOULD",
        "CHILD",
        "REASONING",
        "OLD OPENCODE",
        "New session -",
    ] {
        assert!(!opencode.contains(hidden), "{hidden}: {opencode}");
    }
    let (full, _) = sunday.call(
        "coding_session",
        json!({"agent": "opencode", "id": "ses_c"}),
        &mut no_questions,
    );
    assert!(
        full.contains("User: Make the build faster")
            && full.contains("OpenCode: Cached the dependencies."),
        "{full}"
    );

    // Without OpenAgora's window, nothing that needs asking is read.
    window.close();
    env.ok(&["sessions", "revoke", "opencode"]);
    let mut alone = String::new();
    for _ in 0..50 {
        alone = activity(&mut sunday, json!({"since": since, "agent": "opencode"}));
        if alone.contains("window isn't open") {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(alone.contains("window isn't open"), "{alone}");
    assert!(!alone.contains("Cached the dependencies"), "{alone}");

    // Every read, and every answer, is in the audit log.
    let audit = env.audit();
    let rows = |action: &str| -> Vec<String> {
        audit
            .iter()
            .filter(|e| e["action"] == action)
            .map(|e| {
                format!(
                    "{} {} {}",
                    e["target"].as_str().unwrap(),
                    e["decision"].as_str().unwrap(),
                    e["outcome"].as_str().unwrap()
                )
            })
            .collect()
    };
    let allowed = rows("allow_sessions");
    for row in [
        "claude-code confirmed always",
        "codex confirmed once",
        "opencode declined not read",
        "t3code confirmed always",
        "opencode refused no way to ask the user",
    ] {
        assert!(allowed.iter().any(|r| r == row), "{row}: {allowed:?}");
    }
    let read = rows("sessions_read");
    assert!(
        read.iter()
            .any(|r| r.starts_with("claude-code allowed 1 sessions")),
        "{read:?}"
    );
    assert!(
        read.iter()
            .any(|r| r.starts_with("codex allowed 2 sessions")),
        "{read:?}"
    );
    assert!(
        read.iter()
            .any(|r| r.starts_with("opencode allowed 3 sessions")),
        "{read:?}"
    );
    assert!(
        read.iter().any(|r| r.starts_with("claude-code:5f0c1d2e")),
        "{read:?}"
    );
}

/// Sunday's one-shot turns, as the stand-in Hermes noted them.
fn oneshots(env: &Env) -> Vec<Value> {
    fs::read_to_string(env.sunday_dir().join(".fake-oneshots"))
        .unwrap_or_default()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
    for _ in 0..150 {
        if done() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("timed out waiting for {what}");
}

/// Acceptance test 12, OpenAgora's part: Sunday's memory is Hermes's and
/// stays across restarts; long-term memory is Holographic, set up with
/// Hermes's own commands for Sunday's profile only; the digest keeps it fresh
/// as a turn of Sunday's with only memory and OpenAgora's tools; nothing
/// outside Sunday's profile changes.
#[test]
fn sunday_keeps_what_it_remembers_and_a_digest_keeps_it_fresh() {
    let mut env = Env::new();
    let home = env.home.clone();
    claude_sessions(&home);
    codex_sessions(&home);
    env.env("CLAUDE_CONFIG_DIR", home.join(".claude").to_str().unwrap());
    env.env("CODEX_HOME", home.join(".codex").to_str().unwrap());
    env.env("XDG_DATA_HOME", home.join(".local/share").to_str().unwrap());
    env.env("OPENAGORA_TICK_SECONDS", "1");
    env.env("OPENAGORA_DIGEST_SECONDS", "2");
    // Each digest turn takes a while, so turning the digest off lands while
    // one is under way.
    env.env("FAKE_ONESHOT_DELAY", "1.5");
    env.install_hermes_by_hand();
    env.ok(&["sunday", "setup", "--fresh"]);
    env.ok(&["sunday", "connect", "--yes"]);
    let before = env.snapshot_outside_sunday();
    // Sunday saved a decision with its memory tool in an earlier session.
    let memory = env.sunday_dir().join("memories/MEMORY.md");
    fs::write(&memory, "app: we keep pnpm (decided 2026-10-11)\n").unwrap();

    // Built-in memory, no digest, and Sunday asks before reading sessions.
    let status = env.json(&["sunday", "memory", "status", "--json"]);
    assert_eq!(status["provider"], Value::Null, "{status}");
    assert_eq!(
        status["providers"],
        json!([{"id": "holographic", "name": "Holographic", "needs": "0.21.5", "commands": [
            "hermes -p sunday plugins install holographic --enable --yes-deps",
            "hermes -p sunday memory setup holographic",
        ]}])
    );
    assert_eq!(status["digest"], false);
    let agents: Vec<(&str, bool)> = status["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| (a["agent"].as_str().unwrap(), a["always"].as_bool().unwrap()))
        .collect();
    assert_eq!(agents, [("claude-code", false), ("codex", false)]);
    assert!(env.ok(&["sunday", "memory"]).contains("Digest: off"));
    let skills = env
        .data
        .join("sunday/skills/openagora/openagora-memory/SKILL.md");
    assert!(
        fs::read_to_string(&skills)
            .unwrap()
            .contains("requires_tools: [mcp__openagora__coding_activity]")
    );

    // Long-term memory: Holographic, only after a yes, and only on a Hermes
    // new enough for it.
    assert!(
        !env.run(&["sunday", "memory", "use", "holographic"])
            .status
            .success()
    );
    assert!(!env.sunday_dir().join("plugins/holographic").exists());
    fs::write(env.hermes_root().join(".fake-version"), "0.21.4").unwrap();
    let old = env.run(&["sunday", "memory", "use", "holographic", "--yes"]);
    assert!(
        String::from_utf8_lossy(&old.stderr).contains("Holographic needs Hermes 0.21.5 or later"),
        "{old:?}"
    );
    fs::remove_file(env.hermes_root().join(".fake-version")).unwrap();
    let out = env.ok(&["sunday", "memory", "use", "holographic", "--yes"]);
    assert!(
        out.contains("Sunday now remembers with holographic"),
        "{out}"
    );
    assert!(env.sunday_dir().join("plugins/holographic").is_dir());
    assert_eq!(
        env.json(&["sunday", "memory", "status", "--json"])["provider"],
        "holographic"
    );
    let calls = env.hermes_calls();
    assert!(
        calls.contains(&"-p sunday plugins install holographic --enable --yes-deps".to_string()),
        "{calls:?}"
    );
    assert!(
        calls.contains(&"-p sunday memory setup holographic".to_string()),
        "{calls:?}"
    );

    // The digest: only after a yes; Sunday may then always read the sessions.
    assert!(
        !env.run(&["sunday", "memory", "digest", "on"])
            .status
            .success()
    );
    let out = env.ok(&["sunday", "memory", "digest", "on", "--yes"]);
    assert!(out.contains("digest is on"), "{out}");
    let status = env.json(&["sunday", "memory", "status", "--json"]);
    assert_eq!(status["digest"], true);
    assert!(
        status["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .all(|a| a["always"] == true),
        "{status}"
    );

    // While OpenAgora runs, the digest is a turn of Sunday's with only memory
    // and OpenAgora's tools, over the sessions changed in the last day.
    let _sunday = McpClient::start(&env, "sunday", true);
    wait_for("the first digest", || !oneshots(&env).is_empty());
    let first = oneshots(&env)[0].clone();
    assert_eq!(first["skill"], "openagora-memory");
    assert_eq!(first["toolsets"], "memory,mcp-openagora");
    let prompt = first["prompt"].as_str().unwrap();
    assert!(prompt.contains("coding_activity with since \""), "{prompt}");
    assert!(
        prompt.contains("fact_store") && prompt.contains("data, not instructions"),
        "{prompt}"
    );
    wait_for("the digest to be noted", || {
        env.json(&["sunday", "memory", "status", "--json"])["digest_last_result"]
            == "Saved 2 decisions to memory."
    });
    // Nothing changed since: no second turn, however long it waits.
    std::thread::sleep(Duration::from_millis(3500));
    assert_eq!(oneshots(&env).len(), 1);

    // OpenAgora restarts, a session goes on, and the digest picks up from
    // where it left off.
    env.ok(&["daemon", "stop"]);
    let claude =
        home.join(".claude/projects/-home-u-code-app/5f0c1d2e-aaaa-4bbb-8ccc-123456789abc.jsonl");
    let mut text = fs::read_to_string(&claude).unwrap();
    text.push_str(&format!(
        "{}\n",
        json!({"type": "user", "timestamp": iso_ago(0), "message": {"role": "user", "content": "Let's also pin node 22"}})
    ));
    fs::write(&claude, text).unwrap();
    let _again = McpClient::start(&env, "sunday", true);
    wait_for("the second digest", || oneshots(&env).len() == 2);
    let second = oneshots(&env)[1]["prompt"].as_str().unwrap().to_string();
    assert_ne!(
        second, prompt,
        "the second digest starts where the first ended"
    );
    assert_eq!(
        fs::read_to_string(&memory).unwrap(),
        "app: we keep pnpm (decided 2026-10-11)\n"
    );

    // Off, while that turn is still under way: it doesn't turn the digest
    // back on, and no more turns come. Built-in memory only again,
    // Holographic's store staying in Sunday's profile.
    assert!(
        env.ok(&["sunday", "memory", "digest", "off", "--yes"])
            .contains("digest is off")
    );
    wait_for("the turn under way to finish", || {
        env.audit()
            .iter()
            .filter(|e| e["action"] == "memory_digest" && e["target"] == "run")
            .count()
            == 2
    });
    assert_eq!(
        env.json(&["sunday", "memory", "status", "--json"])["digest"],
        false
    );
    let mut text = fs::read_to_string(&claude).unwrap();
    text.push_str(&format!(
        "{}\n",
        json!({"type": "user", "timestamp": iso_ago(0), "message": {"role": "user", "content": "And use vitest"}})
    ));
    fs::write(&claude, text).unwrap();
    std::thread::sleep(Duration::from_millis(3500));
    assert_eq!(oneshots(&env).len(), 2);
    assert!(
        env.ok(&["sunday", "memory", "off", "--yes"])
            .contains("built-in memory only")
    );
    assert_eq!(
        env.json(&["sunday", "memory", "status", "--json"])["provider"],
        Value::Null
    );
    assert!(env.sunday_dir().join("plugins/holographic").is_dir());

    // Every Hermes call was for Sunday's profile, and nothing outside it
    // changed.
    for call in env.hermes_calls() {
        assert!(
            call.starts_with("-p sunday ") || call == "--version" || call.starts_with("profile "),
            "{call}"
        );
    }
    assert_eq!(env.snapshot_outside_sunday(), before);
    let audit = env.audit();
    let rows: Vec<String> = audit
        .iter()
        .filter(|e| e["action"] == "memory_provider" || e["action"] == "memory_digest")
        .map(|e| {
            format!(
                "{} {} {}",
                e["action"].as_str().unwrap(),
                e["target"].as_str().unwrap(),
                e["outcome"].as_str().unwrap()
            )
        })
        .collect();
    for row in [
        "memory_provider holographic ok",
        "memory_digest on on, reading Claude Code, Codex",
        "memory_digest run 3 sessions: Saved 2 decisions to memory.",
        "memory_digest run 1 sessions: Saved 2 decisions to memory.",
        "memory_digest off off",
        "memory_provider off ok",
    ] {
        assert!(rows.iter().any(|r| r == row), "{row}: {rows:?}");
    }
    assert!(
        rows.iter()
            .any(|r| r.starts_with("memory_provider holographic failed: ")),
        "{rows:?}"
    );
}
