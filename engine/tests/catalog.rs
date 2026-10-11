use std::fs;
use std::path::Path;

use openagora::catalog::Catalog;
use openagora::manifest::{Arch, Manifest, Os, Role, UiKind};

fn repo_catalog() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../catalog"))
}

#[test]
fn shipped_catalog_is_valid() {
    let catalog = Catalog::load(repo_catalog()).expect("catalog should load");
    let hermes = &catalog.listings["hermes"].manifest;
    assert_eq!(hermes.role, Role::Agent);
    let t3 = &catalog.listings["t3code"].manifest;
    assert!(
        t3.agent.is_some(),
        "T3 Code must declare how the agent connects"
    );
    // Neither ships a native build for Intel Macs (T3's installer 404s there,
    // Hermes would have to compile `cryptography`), so there both run their
    // Linux install in a container. Everywhere else they run natively.
    for app in [t3, hermes] {
        for os in [Os::Macos, Os::Linux, Os::Windows] {
            for arch in [Arch::X64, Arch::Arm64] {
                let boxed = os == Os::Macos && arch == Arch::X64;
                let id = &app.id;
                assert_eq!(
                    app.unavailable_on(os, Some(arch)),
                    None,
                    "{id} on {os} {arch}"
                );
                assert_eq!(
                    app.in_container(os, Some(arch)),
                    boxed,
                    "{id} on {os} {arch}"
                );
                let target = app.target_os(os, Some(arch));
                assert_eq!(target, if boxed { Os::Linux } else { os });
                assert!(
                    app.steps_for(target, Some(arch)).count() > 0,
                    "{id} steps for {os} {arch}"
                );
            }
        }
        assert_eq!(app.steps_for(Os::Macos, Some(Arch::X64)).count(), 0);
    }

    let maccy = &catalog.listings["maccy"].manifest;
    assert_eq!(maccy.platforms, [Os::Macos]);
    assert_eq!(maccy.ui.kind, UiKind::Background);

    let handy = &catalog.listings["handy"].manifest;
    for os in [Os::Macos, Os::Linux] {
        let step = handy.steps_for(os, None).next().unwrap();
        let asset = &step.download.as_ref().unwrap().asset;
        for arch in [Arch::X64, Arch::Arm64] {
            assert!(
                asset.for_arch(arch).is_some(),
                "handy: no {os} asset for {arch:?}"
            );
        }
        assert!(handy.run.command.for_os(os).is_some());
    }
}

#[test]
fn embedded_catalog_matches_repo() {
    let embedded = Catalog::embedded().unwrap();
    let on_disk = Catalog::load(repo_catalog()).unwrap();
    let ids = |c: &Catalog| c.listings.keys().cloned().collect::<Vec<_>>();
    assert_eq!(ids(&embedded), ids(&on_disk));
}

#[test]
fn search_matches_name_and_category() {
    let catalog = Catalog::load(repo_catalog()).unwrap();
    let ids = |q: &str| {
        catalog
            .search(q)
            .iter()
            .map(|l| l.manifest.id.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(ids("t3"), ["t3code"]);
    assert_eq!(ids("MEMORY"), ["hermes"]);
    assert_eq!(ids("agents"), ["hermes", "t3code"]);
    assert!(ids("nothing-like-this").is_empty());
}

const VALID: &str = r#"
schema = "openagora/v1"
id = "demo"
name = "Demo"
summary = "A demo app"
category = "coding"
homepage = "https://example.com"
source = "https://github.com/example/demo"
license = "MIT"
platforms = ["linux"]

[install]
provides = "demo"

[[install.steps]]
os = ["linux"]
shell = "sh"
run = "echo install"

[run]
command = "demo"
args = ["--port", "{port}"]
port = "auto"

[run.health]
tcp = true

[ui]
kind = "web"
url = "http://127.0.0.1:{port}/"
"#;

fn problems(edit: impl Fn(&str) -> String) -> String {
    Manifest::parse(&edit(VALID)).err().unwrap_or_default()
}

#[test]
fn valid_manifest_parses() {
    Manifest::parse(VALID).unwrap();
}

#[test]
fn rejects_bad_listings() {
    assert!(problems(|s| s.replace("id = \"demo\"", "id = \"Demo App\"")).contains("id"));
    assert!(
        problems(|s| s.replace(
            "platforms = [\"linux\"]",
            "platforms = [\"linux\", \"macos\"]"
        ))
        .contains("no install step for listed platform macos")
    );
    assert!(
        problems(|s| s.replace("http://127.0.0.1:{port}/", "https://evil.example/"))
            .contains("ui.url")
    );
    assert!(problems(|s| s.replace("port = \"auto\"", "port = \"any\"")).contains("run.port"));
    assert!(
        problems(|s| s.replace("shell = \"sh\"", "shell = \"powershell\"")).contains("powershell")
    );
    // Unknown fields are errors, so a typo never reads as a working setting.
    assert!(
        problems(|s| s.replace("license = \"MIT\"", "license = \"MIT\"\nlicence = \"MIT\""))
            .contains("licence")
    );
}

const BACKGROUND: &str = r#"
schema = "openagora/v1"
id = "tool"
name = "Tool"
summary = "A background tool"
category = "utilities"
homepage = "https://example.com"
source = "https://github.com/example/tool"
license = "MIT"
platforms = ["macos", "linux"]

[install]
provides = { macos = "{app_dir}/Tool.app", linux = "{app_dir}/tool" }

[[install.steps]]
os = ["macos", "linux"]
shell = "sh"
download = { github = "example/tool", asset = { x64 = "tool-x64.tar.gz", arm64 = "tool-arm64.tar.gz" } }
run = "tar -xzf '{download}' -C '{app_dir}'"

[run]
command = { macos = "{app_dir}/Tool.app/Contents/MacOS/Tool", linux = "{app_dir}/tool" }

[run.health]
process = true

[ui]
kind = "background"
"#;

type Edit = dyn Fn(&str) -> String;

fn background_problems(edit: impl Fn(&str) -> String) -> String {
    Manifest::parse(&edit(BACKGROUND)).err().unwrap_or_default()
}

#[test]
fn background_app_rules() {
    Manifest::parse(BACKGROUND).unwrap();
    let cases: &[(&Edit, &str)] = &[
        (
            &|s| s.replace(", linux = \"{app_dir}/tool\" }\n\n[[", " }\n\n[["),
            "install.provides has no value for linux",
        ),
        (
            &|s| {
                s.replace(
                    "kind = \"background\"",
                    "kind = \"background\"\nurl = \"http://127.0.0.1:1/\"",
                )
            },
            "only for kind",
        ),
        (
            &|s| {
                s.replace(
                    "kind = \"background\"",
                    "kind = \"web\"\nurl = \"http://127.0.0.1:{port}/\"",
                )
            },
            "needs run.port",
        ),
        (
            &|s| s.replace("tar -xzf '{download}'", "tar -xzf x"),
            "never uses {download}",
        ),
        (
            &|s| s.replace("github = \"example/tool\"", "github = \"someone/else\""),
            "not this app's source",
        ),
        (
            &|s| {
                s.replace(
                    "{app_dir}/tool\" }\n\n[run.health]",
                    "{app_dir}/tool\" }\nargs = [\"{prot}\"]\n\n[run.health]",
                )
            },
            "unknown placeholder {prot}",
        ),
        (
            &|s| s.replace("process = true", "process = false"),
            "needs a check",
        ),
        (
            &|s| {
                s.replace(
                    "kind = \"background\"",
                    "kind = \"background\"\nurl_from_log_after = \"URL: \"",
                )
            },
            "url_from_log_after is only for kind",
        ),
    ];
    for (edit, expected) in cases {
        let problems = background_problems(edit);
        assert!(
            problems.contains(expected),
            "expected {expected:?} in {problems:?}"
        );
    }
}

#[test]
fn steps_can_be_limited_to_some_cpus() {
    // A step that names CPUs limits its OS to them, even next to a shared step.
    let limited = VALID.replace(
        "run = \"echo install\"\n",
        "run = \"echo install\"\n\n[[install.steps]]\nos = [\"linux\"]\narch = [\"arm64\"]\nshell = \"sh\"\nrun = \"echo arm\"\n",
    );
    let m = Manifest::parse(&limited).unwrap();
    assert_eq!(m.steps_for(Os::Linux, Some(Arch::Arm64)).count(), 2);
    assert_eq!(m.unavailable_on(Os::Linux, Some(Arch::Arm64)), None);
    assert_eq!(
        m.unavailable_on(Os::Linux, Some(Arch::X64)).as_deref(),
        Some("Linux on x64")
    );
    assert!(m.unavailable_on(Os::Linux, None).is_some());
    assert_eq!(
        m.unavailable_on(Os::Windows, Some(Arch::X64)).as_deref(),
        Some("Windows")
    );

    // A download with no asset for this CPU has nothing to install.
    let arm_only = BACKGROUND.replace("x64 = \"tool-x64.tar.gz\", ", "");
    let m = Manifest::parse(&arm_only).unwrap();
    assert_eq!(
        m.unavailable_on(Os::Macos, Some(Arch::X64)).as_deref(),
        Some("Intel Macs")
    );
    assert_eq!(m.unavailable_on(Os::Macos, Some(Arch::Arm64)), None);

    assert!(
        problems(|s| s.replace("shell = \"sh\"", "arch = []\nshell = \"sh\""))
            .contains("empty arch list")
    );
    assert!(
        problems(|s| s.replace("shell = \"sh\"", "arch = [\"x86\"]\nshell = \"sh\""))
            .contains("x86")
    );
}

#[test]
fn container_rules() {
    let boxed = |s: &str| format!("{s}\n[container]\nos = [\"linux\"]\n");
    let m = Manifest::parse(&boxed(VALID)).unwrap();
    assert!(m.in_container(Os::Linux, Some(Arch::X64)));
    assert_eq!(m.target_os(Os::Linux, Some(Arch::X64)), Os::Linux);
    // A container runs on this CPU, so an unknown one can't use it.
    assert!(m.unavailable_on(Os::Linux, None).is_some());

    let cases: &[(&Edit, &str)] = &[
        (
            &|s| s.replace("os = [\"linux\"]\n", "os = []\n"),
            "container.os is empty",
        ),
        (&|s| format!("{s}arch = []\n"), "container.arch is empty"),
        (
            &|s| {
                s.replace(
                    "[container]\nos = [\"linux\"]",
                    "[container]\nos = [\"macos\"]",
                )
            },
            "container targets macos, which is not in platforms",
        ),
        (
            &|s| {
                s.replace("kind = \"web\"", "kind = \"terminal\"")
                    .replace("url = \"http://127.0.0.1:{port}/\"\n", "")
            },
            "container needs ui.kind = \"web\"",
        ),
    ];
    for (edit, expected) in cases {
        let problems = Manifest::parse(&edit(&boxed(VALID)))
            .err()
            .unwrap_or_default();
        assert!(
            problems.contains(expected),
            "expected {expected:?} in {problems:?}"
        );
    }

    // Downloads aren't carried into containers yet.
    let problems =
        background_problems(|s| boxed(s).replace("kind = \"background\"", "kind = \"web\""));
    assert!(
        problems.contains("downloads are not supported in containers"),
        "{problems}"
    );

    // On an OS that uses the container, native steps for the same CPUs clash.
    let both = VALID
        .replace(
            "platforms = [\"linux\"]",
            "platforms = [\"linux\", \"macos\"]",
        )
        .replace(
            "os = [\"linux\"]\nshell",
            "os = [\"linux\", \"macos\"]\nshell",
        );
    let clash = format!("{both}\n[container]\nos = [\"macos\"]\narch = [\"x64\"]\n");
    let problems = Manifest::parse(&clash).err().unwrap_or_default();
    assert!(
        problems.contains("macos x64 has install steps but runs in the container"),
        "{problems}"
    );
    // Limiting the macOS step to the other CPU resolves it.
    let fixed = clash.replace(
        "os = [\"linux\", \"macos\"]\nshell = \"sh\"\nrun = \"echo install\"",
        "os = [\"linux\"]\nshell = \"sh\"\nrun = \"echo install\"\n\n\
         [[install.steps]]\nos = [\"macos\"]\narch = [\"arm64\"]\nshell = \"sh\"\nrun = \"echo mac\"",
    );
    let m = Manifest::parse(&fixed).unwrap();
    assert!(m.in_container(Os::Macos, Some(Arch::X64)));
    assert!(!m.in_container(Os::Macos, Some(Arch::Arm64)));
    assert_eq!(m.unavailable_on(Os::Macos, Some(Arch::X64)), None);
}

#[test]
fn folder_must_match_id() {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path().join("apps").join("other");
    fs::create_dir_all(&app).unwrap();
    fs::write(app.join("app.toml"), VALID).unwrap();
    let err = Catalog::load(dir.path()).unwrap_err().to_string();
    assert!(err.contains("must match its folder"), "{err}");
}
