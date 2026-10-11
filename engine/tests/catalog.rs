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
    assert_eq!(catalog.agent().map(|m| m.id.as_str()), Some("hermes"));
    // Sunday: Hermes only ever runs under its own profile, in a dashboard of
    // its own rather than the user's machine-wide one.
    assert_eq!(hermes.profile.as_deref(), Some("sunday"));
    assert!(hermes.run.args.windows(2).any(|w| w == ["-p", "{profile}"]));
    assert!(hermes.run.args.contains(&"--isolated".to_string()));
    assert!(hermes.uninstall.is_none());
    let t3 = &catalog.listings["t3code"].manifest;
    assert!(
        t3.agent.is_some(),
        "T3 Code must declare how the agent connects"
    );
    let destination = t3.destination.as_ref().expect("T3 Code is a destination");
    assert_eq!(destination.adapter.as_deref(), Some("t3code"));
    assert!(destination.accepts.iter().any(|t| t == "image/png"));
    for os in [Os::Macos, Os::Linux, Os::Windows] {
        assert!(t3.steps_for(os).count() > 0, "no T3 install step for {os}");
    }

    let maccy = &catalog.listings["maccy"].manifest;
    assert_eq!(maccy.platforms, [Os::Macos]);
    assert_eq!(maccy.ui.kind, UiKind::Background);

    let handy = &catalog.listings["handy"].manifest;
    for os in [Os::Macos, Os::Linux] {
        let step = handy.steps_for(os).next().unwrap();
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
fn folder_must_match_id() {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path().join("apps").join("other");
    fs::create_dir_all(&app).unwrap();
    fs::write(app.join("app.toml"), VALID).unwrap();
    let err = Catalog::load(dir.path()).unwrap_err().to_string();
    assert!(err.contains("must match its folder"), "{err}");
}

const AGENT: &str = r#"
schema = "openagora/v1"
id = "helper"
name = "Helper"
summary = "A personal agent"
category = "agents"
homepage = "https://example.com"
source = "https://github.com/example/helper"
license = "MIT"
platforms = ["linux"]
role = "agent"
profile = "sunday"

[install]
provides = "helper"

[[install.steps]]
os = ["linux"]
shell = "sh"
run = "echo install"

[run]
command = "helper"
args = ["--profile", "{profile}", "--port", "{port}"]
port = "auto"

[run.health]
tcp = true

[ui]
kind = "web"
url = "http://127.0.0.1:{port}/"
"#;

#[test]
fn agent_listings_run_only_under_their_own_profile() {
    Manifest::parse(AGENT).unwrap();
    let agent_problems =
        |edit: &dyn Fn(&str) -> String| Manifest::parse(&edit(AGENT)).err().unwrap_or_default();
    let cases: &[(&Edit, &str)] = &[
        (
            &|s| s.replace("profile = \"sunday\"\n", ""),
            "needs a profile",
        ),
        (
            &|s| s.replace("\"--profile\", \"{profile}\", ", ""),
            "run must use {profile}",
        ),
        (
            &|s| s.replace("profile = \"sunday\"", "profile = \"Sun Day\""),
            "profile \"Sun Day\"",
        ),
        (
            &|s| format!("{s}\n[uninstall]\nrun = \"helper uninstall\"\n"),
            "cannot declare uninstall",
        ),
    ];
    for (edit, expected) in cases {
        let problems = agent_problems(edit);
        assert!(
            problems.contains(expected),
            "expected {expected:?} in {problems:?}"
        );
    }
    // Only the agent has a profile, and {profile} means nothing elsewhere.
    assert!(
        problems(|s| s.replace("license = \"MIT\"", "license = \"MIT\"\nprofile = \"x\""))
            .contains("profile is only for role")
    );
    assert!(
        problems(|s| s.replace(
            "\"--port\", \"{port}\"",
            "\"--port\", \"{port}\", \"{profile}\""
        ))
        .contains("unknown placeholder {profile}")
    );
}

#[test]
fn a_catalog_has_at_most_one_agent() {
    let dir = tempfile::tempdir().unwrap();
    for id in ["helper", "other"] {
        let app = dir.path().join("apps").join(id);
        fs::create_dir_all(&app).unwrap();
        fs::write(
            app.join("app.toml"),
            AGENT.replace("id = \"helper\"", &format!("id = \"{id}\"")),
        )
        .unwrap();
    }
    let err = Catalog::load(dir.path()).unwrap_err().to_string();
    assert!(
        err.contains("only one listing can have role = \"agent\""),
        "{err}"
    );
}

fn destination_problems(destination: &str) -> String {
    let listing = format!(
        "{VALID}\n[agent.mcp]\nurl = \"http://127.0.0.1:{{port}}/mcp\"\nauth = \"oauth\"\n\n[destination]\n{destination}\n"
    );
    Manifest::parse(&listing).err().unwrap_or_default()
}

#[test]
fn destinations_say_what_they_take_and_how() {
    // Through the app's MCP server, by a built-in adapter.
    assert_eq!(
        destination_problems(
            "accepts = [\"image/*\", \"text/plain\"]\ndeliver = \"mcp\"\nadapter = \"t3code\""
        ),
        ""
    );
    assert!(
        destination_problems("accepts = [\"image/*\"]\ndeliver = \"mcp\"")
            .contains("destination.adapter is required")
    );
    assert!(
        destination_problems("accepts = [\"image/*\"]\ndeliver = \"mcp\"\nadapter = \"slack\"")
            .contains("isn't built in")
    );
    let without_mcp = Manifest::parse(&format!(
        "{VALID}\n[destination]\naccepts = [\"image/*\"]\ndeliver = \"mcp\"\nadapter = \"t3code\"\n"
    ))
    .unwrap_err();
    assert!(without_mcp.contains("needs [agent.mcp]"), "{without_mcp}");
    // Into a folder, by a URL scheme, or on the clipboard.
    assert_eq!(
        destination_problems(
            "accepts = [\"image/*\"]\ndeliver = \"folder\"\nfolder = \"{app_dir}/inbox\""
        ),
        ""
    );
    assert!(
        destination_problems("accepts = [\"image/*\"]\ndeliver = \"folder\"\nfolder = \"/tmp/x\"")
            .contains("under {app_dir} or {home}")
    );
    assert_eq!(
        destination_problems(
            "accepts = [\"text/*\"]\ndeliver = \"url-scheme\"\nurl = \"notes://open?file={file}\""
        ),
        ""
    );
    assert!(
        destination_problems("accepts = [\"text/*\"]\ndeliver = \"url-scheme\"\nurl = \"https://example.com/?f={file}\"")
            .contains("own URL scheme")
    );
    assert!(
        destination_problems(
            "accepts = [\"text/*\"]\ndeliver = \"url-scheme\"\nurl = \"notes://new\""
        )
        .contains("must use {file}")
    );
    assert_eq!(
        destination_problems("accepts = [\"text/*\"]\ndeliver = \"clipboard\""),
        ""
    );
    // Types, and fields that belong to another way of delivering.
    assert!(
        destination_problems("accepts = []\ndeliver = \"clipboard\"").contains("accepts is empty")
    );
    assert!(
        destination_problems("accepts = [\"pictures\"]\ndeliver = \"clipboard\"")
            .contains("isn't a MIME type or family")
    );
    assert!(
        destination_problems(
            "accepts = [\"text/*\"]\ndeliver = \"clipboard\"\nadapter = \"t3code\""
        )
        .contains("only for deliver = \"mcp\"")
    );
    assert!(
        destination_problems("accepts = [\"text/*\"]\ndeliver = \"pigeon\"").contains("pigeon")
    );
}
