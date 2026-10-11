//! Coding agents OpenAgora didn't install: found on this computer, and set up
//! to read what Sunday sends them, only through their own commands and only
//! after the user agrees (test 7's setup). The agents are stand-ins
//! (`tests/fakes/coding-agent`) that record every call.
#![cfg(unix)]

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use common::Env;
use serde_json::Value;

/// Puts the stand-in agents where their installers would (`~/.local/bin`).
fn with_agents(names: &[&str]) -> (Env, PathBuf) {
    let mut env = Env::new();
    let calls = env.add_fake_agents(names);
    (env, calls)
}

fn calls(file: &Path) -> Vec<String> {
    fs::read_to_string(file)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

fn agent<'a>(listed: &'a Value, id: &str) -> &'a Value {
    listed
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == id)
        .unwrap_or_else(|| panic!("no {id} in {listed}"))
}

/// Every file under `dir`, with its contents, leaving out `skip`.
fn snapshot(dir: &Path, skip: &[&Path]) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        if skip.iter().any(|s| d.starts_with(s)) {
            continue;
        }
        for entry in fs::read_dir(&d).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                todo.push(path);
            } else if !skip.iter().any(|s| path.starts_with(s)) {
                out.insert(path.clone(), fs::read(&path).unwrap_or_default());
            }
        }
    }
    out
}

#[test]
fn coding_agents_are_set_up_only_with_their_own_commands_and_only_when_asked() {
    let (env, calls_file) = with_agents(&["claude", "codex", "opencode"]);
    let engine = env.data.join("bin/openagora");

    // All three are found, none set up yet; each shows the exact command
    // OpenAgora would run.
    let listed = env.json(&["agents", "list", "--json"]);
    for id in ["claude-code", "codex", "opencode"] {
        assert_eq!(agent(&listed, id)["installed"], true, "{id}");
        assert_eq!(agent(&listed, id)["set_up"], false, "{id}");
    }
    assert_eq!(
        agent(&listed, "claude-code")["setup_command"],
        format!(
            "claude mcp add --scope user --transport stdio openagora -- {} mcp --client claude-code",
            engine.display()
        )
    );
    assert_eq!(agent(&listed, "codex")["version"], "codex-cli 0.130.0");
    assert!(
        agent(&listed, "opencode")["remove_by_hand"]
            .as_str()
            .unwrap()
            .contains("opencode.json")
    );

    // Without a yes, nothing is added.
    let before = snapshot(&env.home, &[]);
    let refused = env.run(&["agents", "setup", "claude-code"]);
    assert!(!refused.status.success());
    assert!(common::stderr(&refused).contains("--yes"));
    assert!(
        !calls(&calls_file).iter().any(|c| c.contains("mcp add")),
        "{:?}",
        calls(&calls_file)
    );
    assert_eq!(snapshot(&env.home, &[]), before);

    // With it, OpenAgora runs each agent's own command, and touches nothing
    // else of the agent's: only the stand-in's own record changes.
    for id in ["claude-code", "codex", "opencode"] {
        env.ok(&["agents", "setup", id, "--yes"]);
    }
    let recorded = calls(&calls_file);
    let adds: Vec<&String> = recorded.iter().filter(|c| c.contains("mcp add")).collect();
    let server = format!("{} mcp --client", engine.display());
    assert_eq!(
        adds,
        [
            &format!(
                "claude mcp add --scope user --transport stdio openagora -- {server} claude-code"
            ),
            &format!("codex mcp add openagora -- {server} codex"),
            &format!("opencode mcp add openagora -- {server} opencode"),
        ]
    );
    assert!(
        engine.is_file(),
        "the agents' configs point at the engine's stable copy"
    );
    // Claude Code also gets its prompt hook, as a plugin from OpenAgora's own
    // marketplace folder, installed with Claude Code's own commands.
    let marketplace = env.data.join("claude-code/marketplace");
    assert!(recorded.contains(&format!(
        "claude plugin marketplace add {} --scope user",
        marketplace.display()
    )));
    assert!(
        recorded.contains(&"claude plugin install openagora@openagora --scope user".to_string())
    );
    let hooks: Value = serde_json::from_str(
        &fs::read_to_string(marketplace.join("plugins/openagora/hooks/hooks.json")).unwrap(),
    )
    .unwrap();
    let hook = &hooks["hooks"]["UserPromptSubmit"][0]["hooks"][0];
    assert_eq!(hook["command"], engine.display().to_string());
    assert_eq!(
        hook["args"],
        serde_json::json!([
            "context",
            "--hook",
            "--client",
            "claude-code",
            "--format",
            "claude-code"
        ])
    );
    let after = snapshot(&env.home, &[]);
    let changed: Vec<&PathBuf> = after
        .iter()
        .filter(|(path, bytes)| before.get(*path) != Some(*bytes))
        .map(|(path, _)| path)
        .collect();
    assert!(
        changed.iter().all(|p| p
            .strip_prefix(&env.home)
            .unwrap()
            .to_string_lossy()
            .starts_with(".fake-")),
        "{changed:?}"
    );
    let listed = env.json(&["agents", "list", "--json"]);
    for id in ["claude-code", "codex", "opencode"] {
        assert_eq!(agent(&listed, id)["set_up"], true, "{id}");
    }

    // Removing goes through the agent's own command too; OpenCode has none,
    // so OpenAgora says how, and changes nothing.
    env.ok(&["agents", "remove", "claude-code", "--yes"]);
    let recorded = calls(&calls_file);
    for removal in [
        "claude mcp remove openagora --scope user",
        "claude plugin uninstall openagora@openagora --scope user",
        "claude plugin marketplace remove openagora --scope user",
    ] {
        assert!(
            recorded.iter().any(|c| c == removal),
            "{removal}: {recorded:?}"
        );
    }
    assert!(!marketplace.exists());
    let opencode = env.run(&["agents", "remove", "opencode", "--yes"]);
    assert!(!opencode.status.success());
    assert!(common::stderr(&opencode).contains("opencode.json"));
    let listed = env.json(&["agents", "list", "--json"]);
    assert_eq!(agent(&listed, "claude-code")["set_up"], false);
    assert_eq!(agent(&listed, "opencode")["set_up"], true);

    // Every setup and removal is in the audit log.
    let audit = env.audit();
    let actions: Vec<String> = audit
        .iter()
        .map(|e| {
            format!(
                "{}:{}",
                e["action"].as_str().unwrap(),
                e["target"].as_str().unwrap()
            )
        })
        .collect();
    for expected in [
        "agent_setup:claude-code",
        "agent_setup:codex",
        "agent_setup:opencode",
        "agent_remove:claude-code",
    ] {
        assert!(
            actions.iter().any(|a| a == expected),
            "{expected}: {actions:?}"
        );
    }
}

#[test]
fn opencode_2_is_set_up_in_its_global_config() {
    let (env, calls_file) = with_agents(&["opencode"]);
    fs::write(env.home.join(".fake-opencode-version"), "opencode 2.0.5\n").unwrap();
    env.ok(&["agents", "setup", "opencode", "--yes"]);
    assert!(
        calls(&calls_file)
            .iter()
            .any(|c| c.starts_with("opencode mcp add openagora --global -- ")),
        "{:?}",
        calls(&calls_file)
    );
}

#[test]
fn agents_that_are_missing_or_unknown_are_said_so() {
    let (env, _) = with_agents(&[]);
    let listed = env.json(&["agents", "list", "--json"]);
    assert_eq!(agent(&listed, "claude-code")["installed"], false);
    let missing = env.run(&["agents", "setup", "codex", "--yes"]);
    assert!(common::stderr(&missing).contains("isn't installed"));
    let unknown = env.run(&["agents", "setup", "cursor", "--yes"]);
    assert!(common::stderr(&unknown).contains("agents add"));

    // Any other MCP agent the user connects by hand can be added, so Sunday
    // can send to it.
    let added = env.ok(&["agents", "add", "zed", "--name", "Zed"]);
    assert!(added.contains("mcp --client zed"), "{added}");
    let listed = env.json(&["agents", "list", "--json"]);
    assert_eq!(agent(&listed, "zed")["custom"], true);
    assert!(
        !env.run(&["agents", "add", "sunday", "--name", "x"])
            .status
            .success()
    );
    assert!(env.ok(&["agents", "forget", "zed"]).contains("Forgot"));
}
