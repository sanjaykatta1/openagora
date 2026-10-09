use std::fs;
use std::path::Path;

use openagora::catalog::Catalog;
use openagora::manifest::{Manifest, Os, Role};

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
    for os in [Os::Macos, Os::Linux, Os::Windows] {
        assert!(t3.steps_for(os).count() > 0, "no T3 install step for {os}");
    }
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

#[test]
fn folder_must_match_id() {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path().join("apps").join("other");
    fs::create_dir_all(&app).unwrap();
    fs::write(app.join("app.toml"), VALID).unwrap();
    let err = Catalog::load(dir.path()).unwrap_err().to_string();
    assert!(err.contains("must match its folder"), "{err}");
}
