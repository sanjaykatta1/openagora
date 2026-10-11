//! Sunday's profile, end to end, against a fake Hermes: OpenAgora only ever
//! touches Hermes's `sunday` profile.
#![cfg(unix)]

mod common;

use std::fs;
use std::os::unix::process::CommandExt;
use std::process::Command;

use common::{Env, stderr, stdout};

fn has_python() -> bool {
    Command::new("python3").arg("--version").output().is_ok()
}

/// Every Hermes call OpenAgora made either names Sunday's profile or manages
/// that profile by name. None aims at any other profile.
fn assert_only_sunday(env: &Env) {
    for call in env.hermes_calls() {
        let allowed = call.starts_with("-p sunday ")
            || call == "--version"
            || call.starts_with("profile create sunday ")
            || call == "profile delete sunday --yes"
            || call == "update";
        assert!(allowed, "unexpected Hermes call: {call}");
    }
}

#[test]
fn fresh_install_sets_up_sunday_and_opens_its_dashboard() {
    if !has_python() {
        eprintln!("python3 not found; skipping");
        return;
    }
    let env = Env::new();
    let status = env.json(&["sunday", "status", "--json"]);
    assert_eq!(status["hermes"], serde_json::Value::Null);
    assert_eq!(status["setup"], "unknown");

    env.ok(&["install", "hermes", "--yes"]);
    assert!(env.home.join("installer-ran").exists());
    let status = env.json(&["sunday", "status", "--json"]);
    assert_eq!(status["installed"], true);
    assert_eq!(status["adopted"], false);
    assert_eq!(status["ask_how_to_start"], false);
    assert_eq!(status["setup"], "missing");

    // The profile has to exist before Sunday starts for the first time.
    let early = env.run(&["start", "hermes"]);
    assert!(!early.status.success());
    assert!(
        stderr(&early).contains("Sunday isn't set up yet"),
        "{}",
        stderr(&early)
    );

    // OpenAgora installed Hermes itself, so there is no earlier setup to bring.
    let out = env.ok(&["sunday", "setup"]);
    assert!(out.contains("profiles/sunday"), "{out}");
    assert!(
        env.hermes_calls()
            .contains(&"profile create sunday --no-alias".to_string())
    );
    let soul = fs::read_to_string(env.sunday_dir().join("SOUL.md")).unwrap();
    assert!(
        soul.starts_with("<!-- openagora:sunday:begin -->"),
        "{soul}"
    );
    assert!(soul.contains("Your name is Sunday"));
    // Sunday says what it is built on, and who made it.
    assert!(soul.contains("built on Hermes Agent, the open-source agent by Nous Research"));
    // Hermes's seeded text is kept under the block.
    assert!(soul.contains("You are Hermes Agent, built by Nous Research."));

    let status = env.json(&["sunday", "status", "--json"]);
    assert_eq!(status["setup"], "ready");
    assert_eq!(status["version"], "0.21.6");
    assert_eq!(status["version_ok"], true);

    let started = env.ok(&["start", "hermes"]);
    let url = started
        .split_whitespace()
        .find(|w| w.starts_with("http://127.0.0.1:"))
        .expect("start prints the dashboard address")
        .to_string();
    let page = Command::new("curl").args(["-fsS", &url]).output().unwrap();
    assert!(stdout(&page).contains("Hermes dashboard for sunday"));
    let status = env.json(&["sunday", "status", "--json"]);
    assert_eq!(status["dashboard"], "running");
    assert_eq!(status["url"], url.as_str());
    // Hermes owns the messaging gateway; OpenAgora shows what Hermes reports.
    assert_eq!(status["gateway"]["running"], false);
    assert_eq!(status["gateway"]["state"], "stopped");
    env.ok(&["stop", "hermes"]);
    assert_only_sunday(&env);
}

#[test]
fn an_existing_hermes_is_used_as_it_is_and_left_unchanged() {
    let env = Env::new();
    env.install_hermes_by_hand();
    env.existing_hermes_setup();
    let before = env.snapshot_outside_sunday();

    // Installing from the store records the Hermes that is already here.
    let out = env.ok(&["install", "hermes", "--yes"]);
    assert!(!env.home.join("installer-ran").exists(), "{out}");
    let listing = env.json(&["catalog", "--json"]);
    assert_eq!(listing[0]["adopted"], true);
    assert_eq!(listing[0]["present"], true);

    // Someone who already uses Hermes has to choose how Sunday starts.
    let undecided = env.run(&["sunday", "setup"]);
    assert!(!undecided.status.success());
    assert!(
        stderr(&undecided).contains("choose how Sunday starts"),
        "{}",
        stderr(&undecided)
    );
    assert!(!env.sunday_dir().exists());

    env.ok(&["sunday", "setup", "--clone"]);
    assert!(
        env.hermes_calls()
            .contains(&"profile create sunday --no-alias --clone".to_string())
    );
    // The clone brings settings and memory, but the bot stays with the user's Hermes.
    let sunday_env = fs::read_to_string(env.sunday_dir().join(".env")).unwrap();
    assert!(sunday_env.contains("OPENROUTER_API_KEY"));
    assert!(!sunday_env.contains("TELEGRAM_BOT_TOKEN"));
    let soul = fs::read_to_string(env.sunday_dir().join("SOUL.md")).unwrap();
    assert!(soul.starts_with("<!-- openagora:sunday:begin -->"));
    assert!(
        soul.ends_with("You are Jarvis. Speak like a butler.\n"),
        "{soul}"
    );

    assert_eq!(env.snapshot_outside_sunday(), before);
    assert_only_sunday(&env);
}

#[test]
fn bringing_messaging_bots_is_the_users_explicit_choice() {
    let env = Env::new();
    env.install_hermes_by_hand();
    env.existing_hermes_setup();
    let before = env.snapshot_outside_sunday();
    env.ok(&["sunday", "setup", "--clone", "--clone-channels"]);
    assert!(
        env.hermes_calls()
            .contains(&"profile create sunday --no-alias --clone --clone-channels".to_string())
    );
    let sunday_env = fs::read_to_string(env.sunday_dir().join(".env")).unwrap();
    assert!(sunday_env.contains("TELEGRAM_BOT_TOKEN"));
    assert_eq!(env.snapshot_outside_sunday(), before);
}

#[test]
fn an_old_hermes_is_updated_with_its_own_command_first() {
    let mut env = Env::new();
    env.install_hermes_by_hand();
    env.set_hermes_version("0.20.2");
    let too_old = env.run(&["sunday", "setup", "--fresh"]);
    assert!(!too_old.status.success());
    let err = stderr(&too_old);
    assert!(
        err.contains("0.21.3") && err.contains("openagora update hermes"),
        "{err}"
    );
    assert!(!env.sunday_dir().exists());

    env.env("FAKE_UPDATE_TO", "0.21.4");
    env.ok(&["update", "hermes", "--yes"]);
    assert!(env.hermes_calls().contains(&"update".to_string()));
    env.ok(&["sunday", "setup", "--fresh"]);
    let status = env.json(&["sunday", "status", "--json"]);
    assert_eq!(status["version"], "0.21.4");
    assert_only_sunday(&env);
}

#[test]
fn removing_sunday_deletes_only_its_profile() {
    let env = Env::new();
    env.install_hermes_by_hand();
    env.existing_hermes_setup();
    let before = env.snapshot_outside_sunday();
    env.ok(&["sunday", "setup", "--clone"]);
    assert!(env.sunday_dir().exists());

    // Removal asks first; without a terminal only --yes proceeds.
    let unconfirmed = env.run(&["uninstall", "hermes"]);
    assert!(!unconfirmed.status.success());
    assert!(stderr(&unconfirmed).contains("Remove Sunday?"));
    assert!(env.sunday_dir().exists());

    let out = env.ok(&["uninstall", "hermes", "--yes"]);
    assert!(
        out.contains("Removed Sunday. Hermes is still installed."),
        "{out}"
    );
    assert!(
        env.hermes_calls()
            .contains(&"profile delete sunday --yes".to_string())
    );
    assert!(!env.sunday_dir().exists());
    assert!(env.home.join(".local/bin/hermes").exists());
    assert_eq!(env.snapshot_outside_sunday(), before);

    let status = env.json(&["sunday", "status", "--json"]);
    assert_eq!(status["installed"], false);
    assert_eq!(status["setup"], "missing");
    assert!(
        status["hermes"]
            .as_str()
            .unwrap()
            .ends_with(".local/bin/hermes")
    );
    assert_only_sunday(&env);
}

#[test]
fn a_sunday_profile_openagora_did_not_create_is_left_alone() {
    let env = Env::new();
    env.install_hermes_by_hand();
    env.existing_hermes_setup();
    let theirs = env.sunday_dir();
    fs::create_dir_all(&theirs).unwrap();
    fs::write(theirs.join("config.yaml"), "model:\n  default: mine\n").unwrap();

    let status = env.json(&["sunday", "status", "--json"]);
    assert_eq!(status["setup"], "foreign");
    let refused = env.run(&["sunday", "setup", "--fresh"]);
    assert!(!refused.status.success());
    assert!(
        stderr(&refused).contains("--use-existing"),
        "{}",
        stderr(&refused)
    );

    // Removing Sunday never deletes a profile OpenAgora didn't set up.
    env.ok(&["adopt", "hermes"]);
    let out = env.ok(&["uninstall", "hermes", "--yes"]);
    assert!(out.contains("wasn't created by OpenAgora"), "{out}");
    assert!(theirs.join("config.yaml").exists());
    assert!(
        !env.hermes_calls()
            .iter()
            .any(|c| c.starts_with("profile delete"))
    );

    // With the user's go-ahead it becomes Sunday's.
    env.ok(&["sunday", "setup", "--use-existing"]);
    let soul = fs::read_to_string(theirs.join("SOUL.md")).unwrap();
    assert!(soul.contains("Your name is Sunday"));
    assert_eq!(
        fs::read_to_string(theirs.join("config.yaml")).unwrap(),
        "model:\n  default: mine\n"
    );
}

#[test]
fn adopting_an_app_never_runs_its_installer_or_uninstaller() {
    let env = Env::new();
    let missing = env.run(&["adopt", "hermes"]);
    assert!(!missing.status.success());
    assert!(stderr(&missing).contains("not on this computer"));

    env.install_hermes_by_hand();
    let out = env.ok(&["adopt", "hermes"]);
    assert!(out.contains("use it as it is"), "{out}");
    assert!(!env.home.join("installer-ran").exists());
    env.ok(&["uninstall", "hermes", "--yes"]);
    assert!(env.home.join(".local/bin/hermes").exists());
}

#[test]
fn a_hermes_an_earlier_openagora_installed_asks_how_sunday_starts() {
    // OpenAgora 0.3 installed Hermes and opened its default profile, where the
    // user may well have set up a model. Sunday mustn't start fresh unasked.
    let env = Env::new();
    env.install_hermes_by_hand();
    env.existing_hermes_setup();
    fs::create_dir_all(env.data.join("state")).unwrap();
    fs::write(
        env.data.join("state/hermes.toml"),
        "id = \"hermes\"\ninstalled_at = 1760000000\nassets = []\n",
    )
    .unwrap();
    let status = env.json(&["sunday", "status", "--json"]);
    assert_eq!(status["installed"], true);
    assert_eq!(status["ask_how_to_start"], true);
    let undecided = env.run(&["sunday", "setup"]);
    assert!(!undecided.status.success());
    assert!(stderr(&undecided).contains("choose how Sunday starts"));
    env.ok(&["sunday", "setup", "--clone"]);
}

#[test]
fn a_dashboard_an_earlier_openagora_started_is_replaced() {
    if !has_python() {
        eprintln!("python3 not found; skipping");
        return;
    }
    let env = Env::new();
    env.install_hermes_by_hand();
    env.ok(&["sunday", "setup", "--fresh"]);
    // A dashboard started without Sunday's profile, as OpenAgora 0.3 did.
    let mut old = Command::new("sleep")
        .arg("60")
        .process_group(0)
        .spawn()
        .unwrap();
    fs::create_dir_all(env.data.join("run")).unwrap();
    fs::write(
        env.data.join("run/hermes.toml"),
        format!(
            "pid = {}\nport = 9\nurl = \"http://127.0.0.1:9/\"\nstarted_at = 1760000000\n",
            old.id()
        ),
    )
    .unwrap();
    let status = env.json(&["sunday", "status", "--json"]);
    assert_eq!(status["dashboard"], "stopped");

    let started = env.ok(&["start", "hermes"]);
    assert!(!started.contains("http://127.0.0.1:9/"), "{started}");
    assert!(
        old.wait().unwrap().code().is_none(),
        "the old dashboard was stopped"
    );
    let status = env.json(&["sunday", "status", "--json"]);
    assert_eq!(status["dashboard"], "running");
    env.ok(&["stop", "hermes"]);
}
