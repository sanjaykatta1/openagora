//! Embeds the catalog in the binary, so an installed `openagora` works from any
//! folder and offline. `--catalog <dir>` still reads a local copy instead.

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

fn main() {
    let apps = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("../catalog/apps");
    println!("cargo:rerun-if-changed={}", apps.display());

    let mut entries: Vec<(String, PathBuf)> = fs::read_dir(&apps)
        .unwrap_or_else(|e| panic!("reading {}: {e}", apps.display()))
        .filter_map(|e| e.ok())
        .filter(|e| e.path().join("app.toml").is_file())
        .map(|e| {
            let folder = e.file_name().to_string_lossy().into_owned();
            let file = fs::canonicalize(e.path().join("app.toml")).unwrap();
            println!("cargo:rerun-if-changed={}", file.display());
            (folder, file)
        })
        .collect();
    entries.sort();

    let mut code = String::from("pub static EMBEDDED: &[(&str, &str)] = &[\n");
    for (folder, file) in entries {
        writeln!(
            code,
            "    ({folder:?}, include_str!({:?})),",
            file.display().to_string()
        )
        .unwrap();
    }
    code.push_str("];\n");
    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("embedded_catalog.rs");
    fs::write(out, code).unwrap();
}
