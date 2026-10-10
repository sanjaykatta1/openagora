use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use openagora::apps::{Engine, Status, tail};
use openagora::catalog::{Catalog, load_listing};
use openagora::manifest::{Arch, Manifest, Os};
use std::io::{BufRead, IsTerminal, Write};

#[derive(Parser)]
#[command(
    name = "openagora",
    version,
    about = "Install and run open-source apps from the OpenAgora catalog"
)]
struct Cli {
    /// Read listings from this folder (contains apps/<id>/app.toml) instead of
    /// the catalog built into this binary.
    #[arg(long, global = true)]
    catalog: Option<PathBuf>,
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// List or search the catalog.
    Catalog {
        /// Only show apps matching this text.
        query: Option<String>,
        /// Print JSON (for the desktop app): every listing with its install
        /// steps for this machine, permissions, and install/run status.
        #[arg(long)]
        json: bool,
    },
    /// Show one listing, including the install steps for this OS.
    Show { id: String },
    /// Check listing folders (for catalog contributors and CI).
    Validate {
        /// Listing folders; defaults to the whole catalog.
        dirs: Vec<PathBuf>,
    },
    /// Install an app (asks for confirmation first).
    Install {
        id: String,
        /// Don't ask for confirmation.
        #[arg(short, long)]
        yes: bool,
    },
    /// Stop and remove an app (asks for confirmation first).
    Uninstall {
        id: String,
        /// Don't ask for confirmation.
        #[arg(short, long)]
        yes: bool,
    },
    /// Start an installed app in the background.
    Start { id: String },
    /// Stop a running app.
    Stop { id: String },
    /// List installed apps and whether they are running.
    Ps,
    /// Show the end of an app's log.
    Logs {
        id: String,
        /// Number of lines.
        #[arg(short = 'n', long, default_value_t = 50)]
        lines: usize,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("openagora: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn load_catalog(dir: &Option<PathBuf>) -> anyhow::Result<Catalog> {
    match dir {
        Some(dir) => Catalog::load(dir),
        None => Catalog::embedded(),
    }
}

fn run(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        Cmd::Catalog { query, json } => {
            let catalog = load_catalog(&cli.catalog)?;
            let listings = match &query {
                Some(q) => catalog.search(q),
                None => catalog.listings.values().collect(),
            };
            if json {
                let engine = Engine::new()?;
                let items = listings
                    .iter()
                    .map(|l| listing_json(&engine, &l.manifest))
                    .collect::<anyhow::Result<Vec<_>>>()?;
                println!("{}", serde_json::to_string_pretty(&items)?);
                return Ok(());
            }
            let here = Os::current();
            for l in listings {
                let m = &l.manifest;
                let note = match here {
                    Some(os) if !m.platforms.contains(&os) => "  (not available on this OS)",
                    _ => "",
                };
                println!("{:<10} {:<10} {}{note}", m.id, m.category, m.summary);
            }
        }
        Cmd::Show { id } => {
            let catalog = load_catalog(&cli.catalog)?;
            describe(listing(&catalog, &id)?);
        }
        Cmd::Validate { dirs } => {
            if dirs.is_empty() {
                let catalog = load_catalog(&cli.catalog)?;
                println!("{} listing(s) valid", catalog.listings.len());
            } else {
                for dir in &dirs {
                    load_listing(dir)?;
                    println!("{}: valid", dir.display());
                }
            }
        }
        Cmd::Install { id, yes } => {
            let catalog = load_catalog(&cli.catalog)?;
            let m = listing(&catalog, &id)?;
            let engine = Engine::new()?;
            describe(m);
            if !confirm(&format!("Install {}?", m.name), yes)? {
                println!("Nothing was installed.");
                return Ok(());
            }
            engine.install(m)?;
            println!(
                "Installed {}. Start it with: openagora start {}",
                m.name, m.id
            );
        }
        Cmd::Uninstall { id, yes } => {
            let catalog = load_catalog(&cli.catalog)?;
            let m = listing(&catalog, &id)?;
            let engine = Engine::new()?;
            if !confirm(&format!("Remove {} and its folder?", m.name), yes)? {
                println!("Nothing was removed.");
                return Ok(());
            }
            engine.uninstall(m)?;
            println!("Removed {}.", m.name);
        }
        Cmd::Start { id } => {
            let catalog = load_catalog(&cli.catalog)?;
            let m = listing(&catalog, &id)?;
            let running = Engine::new()?.start(m)?;
            match &running.url {
                Some(url) => println!("{} is running at {url} (pid {})", m.name, running.pid),
                None => println!("{} is running (pid {})", m.name, running.pid),
            }
        }
        Cmd::Stop { id } => {
            let catalog = load_catalog(&cli.catalog)?;
            let m = listing(&catalog, &id)?;
            if Engine::new()?.stop(m)? {
                println!("Stopped {}.", m.name);
            } else {
                println!("{} was not running.", m.name);
            }
        }
        Cmd::Ps => {
            let catalog = load_catalog(&cli.catalog)?;
            let engine = Engine::new()?;
            let mut any = false;
            for (id, l) in &catalog.listings {
                if engine.installed(id)?.is_none() {
                    continue;
                }
                any = true;
                match engine.status(id)? {
                    Status::Running(r) => println!(
                        "{id:<10} running  pid {:<7} {}",
                        r.pid,
                        r.url.as_deref().unwrap_or("(background)")
                    ),
                    Status::Stopped => println!("{id:<10} stopped  {}", l.manifest.name),
                }
            }
            if !any {
                println!("No apps installed. Browse with: openagora catalog");
            }
        }
        Cmd::Logs { id, lines } => {
            let engine = Engine::new()?;
            println!("{}", tail(&engine.paths.log_file(&id), lines)?);
        }
    }
    Ok(())
}

fn listing<'a>(catalog: &'a Catalog, id: &str) -> anyhow::Result<&'a Manifest> {
    catalog
        .listings
        .get(id)
        .map(|l| &l.manifest)
        .ok_or_else(|| anyhow::anyhow!("no app {id:?} in the catalog (try: openagora catalog)"))
}

fn describe(m: &Manifest) {
    println!(
        "{} ({})\n{}\nsource:  {}\nlicense: {}",
        m.name, m.id, m.summary, m.source, m.license
    );
    match Os::current() {
        Some(os) if m.platforms.contains(&os) => {
            println!("install on {os}:");
            for step in m.steps_for(os) {
                if let Some(d) = &step.download {
                    let asset = Arch::current().and_then(|a| d.asset.for_arch(a));
                    println!(
                        "  download {} from github.com/{} (latest release)",
                        asset.unwrap_or("(no build for this CPU)"),
                        d.github
                    );
                }
                println!("  {}", step.run);
            }
        }
        Some(os) => println!("not available on {os}"),
        None => println!("install: this OS is not supported"),
    }
    if let Some(agent) = &m.agent {
        println!("agent access (asked at install):");
        for p in &agent.permissions {
            println!("  [{:?}] {}", p.default, p.description);
        }
    }
}

/// Asks a yes/no question; without a terminal, only `--yes` proceeds.
fn confirm(question: &str, yes: bool) -> anyhow::Result<bool> {
    if yes {
        return Ok(true);
    }
    if !std::io::stdin().is_terminal() {
        anyhow::bail!("{question} Run again with --yes to confirm without a terminal.");
    }
    print!("{question} [y/N] ");
    std::io::stdout().flush()?;
    let mut answer = String::new();
    std::io::stdin().lock().read_line(&mut answer)?;
    Ok(matches!(answer.trim().to_lowercase().as_str(), "y" | "yes"))
}

fn listing_json(engine: &Engine, m: &Manifest) -> anyhow::Result<serde_json::Value> {
    let os = engine.os;
    let available = m.platforms.contains(&os);
    let steps: Vec<serde_json::Value> = m
        .steps_for(os)
        .map(|step| {
            let download = step.download.as_ref().map(|d| {
                serde_json::json!({
                    "github": d.github,
                    "asset": engine.arch.and_then(|a| d.asset.for_arch(a)),
                })
            });
            serde_json::json!({ "download": download, "run": step.run })
        })
        .collect();
    let permissions: Vec<serde_json::Value> = m
        .agent
        .iter()
        .flat_map(|a| &a.permissions)
        .map(|p| {
            serde_json::json!({
                "id": p.id,
                "description": p.description,
                "default": format!("{:?}", p.default).to_lowercase(),
            })
        })
        .collect();
    let installed = engine.installed(&m.id)?.is_some();
    let (status, pid, url, port) = match engine.status(&m.id)? {
        Status::Running(r) => ("running", Some(r.pid), r.url, r.port),
        Status::Stopped if installed => ("stopped", None, None, None),
        Status::Stopped => ("not-installed", None, None, None),
    };
    // The app's plain address. `url` can differ: a one-time sign-in link the
    // app printed at startup, which only works for the first visit.
    let home_url = match (&m.ui.url, port) {
        (Some(template), Some(port)) => Some(template.replace("{port}", &port.to_string())),
        _ => None,
    };
    Ok(serde_json::json!({
        "id": m.id,
        "name": m.name,
        "summary": m.summary,
        "category": m.category,
        "homepage": m.homepage,
        "source": m.source,
        "license": m.license,
        "platforms": m.platforms.iter().map(|o| o.to_string()).collect::<Vec<_>>(),
        "available": available,
        "role": format!("{:?}", m.role).to_lowercase(),
        "ui": format!("{:?}", m.ui.kind).to_lowercase(),
        "steps": steps,
        "permissions": permissions,
        "installed": installed,
        "status": status,
        "pid": pid,
        "url": url,
        "home_url": home_url,
    }))
}
