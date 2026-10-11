use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use openagora::apps::{Engine, Status, tail};
use openagora::catalog::{Catalog, load_listing};
use openagora::manifest::{Arch, Manifest, Os, Role};
use openagora::paths::Paths;
use openagora::sunday::{self, MIN_VERSION, Start};
use openagora::{audit, context, daemon, ipc};
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
    /// Record an app that is already on this computer, without reinstalling it.
    Adopt { id: String },
    /// Run an app's own update command (asks for confirmation first).
    Update {
        id: String,
        /// Don't ask for confirmation.
        #[arg(short, long)]
        yes: bool,
    },
    /// Stop and remove an app (asks for confirmation first). For Sunday, this
    /// deletes only its own Hermes profile.
    Uninstall {
        id: String,
        /// Don't ask for confirmation.
        #[arg(short, long)]
        yes: bool,
    },
    /// Sunday, OpenAgora's personal agent (Hermes under its own profile).
    Sunday {
        #[command(subcommand)]
        action: SundayCmd,
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
    /// Run the engine as a service (the desktop app starts it): the MCP
    /// gateway Sunday connects to, over a local socket.
    Serve,
    /// Start, stop or check the engine service.
    Daemon {
        #[command(subcommand)]
        action: DaemonCmd,
    },
    /// The MCP server for an agent, on stdin/stdout. Starts the engine service
    /// if it isn't running.
    Mcp {
        /// Who this connection is for, e.g. `sunday`.
        #[arg(long)]
        client: String,
    },
    /// What changed since the agent's previous reply: installed apps and more.
    Context {
        /// Act as a Hermes `pre_llm_call` shell hook: read Hermes's event on
        /// stdin and print `{"context": …}`, or nothing when nothing changed.
        #[arg(long)]
        hook: bool,
        /// Who the context is for, e.g. `sunday`.
        #[arg(long, default_value = "sunday")]
        client: String,
    },
    /// Show the audit log: every app action, with who asked and who confirmed.
    Audit {
        /// Number of entries.
        #[arg(short = 'n', long, default_value_t = 50)]
        lines: usize,
        /// Print JSON.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum DaemonCmd {
    /// Start the engine service in the background, if it isn't running.
    Start {
        /// Print JSON (for the desktop app).
        #[arg(long)]
        json: bool,
    },
    /// Stop the engine service.
    Stop,
    /// Whether the engine service is running, and where.
    Status {
        /// Print JSON.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum SundayCmd {
    /// Where Sunday stands: Hermes, its profile, the dashboard and the
    /// messaging gateway.
    Status {
        /// Print JSON (for the desktop app).
        #[arg(long)]
        json: bool,
    },
    /// Set Sunday up: use the Hermes already on this computer (or the one
    /// OpenAgora installed), create its `sunday` profile and Sunday's SOUL.md.
    Setup {
        /// Start with a new, empty setup.
        #[arg(long, conflicts_with = "clone")]
        fresh: bool,
        /// Bring your current Hermes setup: settings, keys, SOUL.md, skills and
        /// memory. History and messaging bots stay behind.
        #[arg(long)]
        clone: bool,
        /// With --clone, also copy your messaging bots. A bot answers for one
        /// profile at a time, so turn it off in your other setup.
        #[arg(long, requires = "clone")]
        clone_channels: bool,
        /// Use a profile with Sunday's name that OpenAgora didn't create.
        #[arg(long)]
        use_existing: bool,
    },
    /// Let Sunday use OpenAgora: adds OpenAgora's MCP gateway, a per-reply
    /// hook and OpenAgora's skills to Sunday's Hermes profile, through Hermes's
    /// own `config set` (asks first).
    Connect {
        /// Don't ask for confirmation.
        #[arg(short, long)]
        yes: bool,
    },
    /// Undo `connect`: removes only what it added.
    Disconnect {
        /// Don't ask for confirmation.
        #[arg(short, long)]
        yes: bool,
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
        Cmd::Adopt { id } => {
            let catalog = load_catalog(&cli.catalog)?;
            let m = listing(&catalog, &id)?;
            let installed = Engine::new()?.adopt(m)?;
            if installed.adopted {
                println!(
                    "{} is on this computer; OpenAgora will use it as it is.",
                    m.name
                );
            } else {
                println!("{} is already installed.", m.name);
            }
        }
        Cmd::Update { id, yes } => {
            let catalog = load_catalog(&cli.catalog)?;
            let m = listing(&catalog, &id)?;
            let engine = Engine::new()?;
            let Some(command) = m.update.as_ref().map(|u| u.run.as_str()) else {
                anyhow::bail!("{} has no update command", m.name);
            };
            if !confirm(&format!("Update {} by running `{command}`?", m.name), yes)? {
                println!("Nothing was updated.");
                return Ok(());
            }
            engine.update(m)?;
            println!("Updated {}.", m.name);
        }
        Cmd::Uninstall { id, yes } => {
            let catalog = load_catalog(&cli.catalog)?;
            let m = listing(&catalog, &id)?;
            let engine = Engine::new()?;
            let question = if m.role == Role::Agent {
                format!(
                    "Remove Sunday? This deletes {}'s {:?} profile: its settings, keys, memory, sessions and skills. {} itself and your other profiles are kept.",
                    m.name,
                    m.profile.as_deref().unwrap_or_default(),
                    m.name
                )
            } else {
                format!("Remove {} and its folder?", m.name)
            };
            if !confirm(&question, yes)? {
                println!("Nothing was removed.");
                return Ok(());
            }
            let note = engine.uninstall(m)?;
            if m.role == Role::Agent {
                println!("Removed Sunday. {} is still installed.", m.name);
            } else {
                println!("Removed {}.", m.name);
            }
            if let Some(note) = note {
                println!("{note}");
            }
        }
        Cmd::Sunday { action } => {
            let catalog = load_catalog(&cli.catalog)?;
            let m = catalog
                .agent()
                .ok_or_else(|| anyhow::anyhow!("this catalog has no personal agent"))?;
            let engine = Engine::new()?;
            match action {
                SundayCmd::Status { json } => {
                    let status = sunday_json(&engine, m)?;
                    if json {
                        println!("{}", serde_json::to_string_pretty(&status)?);
                    } else {
                        print_sunday(&status);
                    }
                }
                SundayCmd::Setup {
                    fresh,
                    clone,
                    clone_channels,
                    use_existing,
                } => {
                    let start = match (fresh, clone) {
                        (true, _) => Some(Start::Fresh),
                        (_, true) => Some(Start::Clone {
                            channels: clone_channels,
                        }),
                        _ => None,
                    };
                    let profile = sunday::setup(&engine, m, start, use_existing)?;
                    println!(
                        "Sunday is set up in {}'s {:?} profile ({}). Start it with: openagora start {}",
                        m.name, profile.profile, profile.dir, m.id
                    );
                }
                SundayCmd::Connect { yes } => {
                    let question = format!(
                        "Let Sunday use OpenAgora? This adds to {}'s \"sunday\" profile, with Hermes's own `config set`: OpenAgora's MCP server, a hook that tells Sunday about your apps with each reply, OpenAgora's skills, and hooks_auto_accept for that profile only.",
                        m.name
                    );
                    if !confirm(&question, yes)? {
                        println!("Nothing was changed.");
                        return Ok(());
                    }
                    let others = sunday::connect(&engine, m)?;
                    println!(
                        "Sunday now uses OpenAgora. Start a new Sunday session to pick it up."
                    );
                    if !others.is_empty() {
                        println!(
                            "Sunday's profile also has MCP servers from your earlier setup: {}. They don't go through OpenAgora, so OpenAgora can't check what Sunday does with them. Remove them on Hermes's MCP page if OpenAgora's should be Sunday's only one.",
                            others.join(", ")
                        );
                    }
                }
                SundayCmd::Disconnect { yes } => {
                    if !confirm(
                        "Stop Sunday using OpenAgora's MCP server, hook and skills?",
                        yes,
                    )? {
                        println!("Nothing was changed.");
                        return Ok(());
                    }
                    sunday::disconnect(&engine, m)?;
                    println!("Sunday no longer uses OpenAgora.");
                }
            }
        }
        Cmd::Serve => {
            daemon::serve(Paths::from_env()?, cli.catalog.clone())?;
        }
        Cmd::Daemon { action } => {
            let paths = Paths::from_env()?;
            match action {
                DaemonCmd::Start { json } => {
                    // A service left running by an older or newer OpenAgora is
                    // replaced by this one (agents reconnect on their own).
                    if daemon::info(&paths)?.is_some_and(|i| i.version != env!("CARGO_PKG_VERSION"))
                    {
                        daemon::stop(&paths)?;
                    }
                    ipc::start_daemon(&paths, cli.catalog.as_deref())?;
                    let info = daemon::info(&paths)?
                        .ok_or_else(|| anyhow::anyhow!("the engine service did not start"))?;
                    if json {
                        println!("{}", serde_json::to_string(&info)?);
                    } else {
                        println!(
                            "The engine is running (pid {}) at {}",
                            info.pid, info.address
                        );
                    }
                }
                DaemonCmd::Stop => {
                    if daemon::stop(&paths)? {
                        println!("Stopped the engine service.");
                    } else {
                        println!("The engine service was not running.");
                    }
                }
                DaemonCmd::Status { json } => {
                    let info = daemon::info(&paths)?;
                    if json {
                        println!("{}", serde_json::to_string(&info)?);
                    } else {
                        match info {
                            Some(i) => {
                                println!("The engine is running (pid {}) at {}", i.pid, i.address)
                            }
                            None => println!("The engine service is not running."),
                        }
                    }
                }
            }
        }
        Cmd::Mcp { client } => {
            daemon::bridge(&Paths::from_env()?, cli.catalog.as_deref(), &client)?;
        }
        Cmd::Context { hook, client } => {
            let catalog = load_catalog(&cli.catalog)?;
            let engine = Engine::new()?;
            let payload = if hook {
                let mut input = String::new();
                std::io::Read::read_to_string(&mut std::io::stdin(), &mut input)?;
                serde_json::from_str(&input).unwrap_or(serde_json::Value::Null)
            } else {
                serde_json::json!({"extra": {"is_first_turn": true}})
            };
            let text = context::for_turn(&engine, &catalog, &client, &payload)?;
            match (hook, text) {
                (true, Some(text)) => println!("{}", serde_json::json!({ "context": text })),
                (true, None) => {}
                (false, text) => println!("{}", text.unwrap_or_default()),
            }
        }
        Cmd::Audit { lines, json } => {
            let entries = audit::read(&Paths::from_env()?, lines)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&entries)?);
            } else if entries.is_empty() {
                println!("The audit log is empty.");
            } else {
                for e in entries {
                    let via = e.via.map(|v| format!(" in {v}")).unwrap_or_default();
                    println!(
                        "{}  {:<10} {:<14} {:<10} {}{via}: {}",
                        e.at, e.client, e.action, e.target, e.decision, e.outcome
                    );
                }
            }
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
    let record = engine.installed(&m.id)?;
    let installed = record.is_some();
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
        "profile": m.profile,
        "ui": format!("{:?}", m.ui.kind).to_lowercase(),
        "steps": steps,
        "permissions": permissions,
        "installed": installed,
        "adopted": record.as_ref().is_some_and(|i| i.adopted),
        "present": engine.present(m),
        "status": status,
        "pid": pid,
        "url": url,
        "home_url": home_url,
    }))
}

/// Sunday's state for the desktop app: Hermes, its profile, the dashboard and,
/// while the dashboard runs, Hermes's messaging gateway as Hermes reports it.
fn sunday_json(engine: &Engine, m: &Manifest) -> anyhow::Result<serde_json::Value> {
    let report = sunday::report(engine, m)?;
    // A dashboard an earlier OpenAgora started outside Sunday's profile isn't
    // Sunday's; starting Sunday replaces it.
    let (dashboard, url, pid, port) = match engine.status(&m.id)? {
        Status::Running(r) if r.profile == m.profile => ("running", r.url, Some(r.pid), r.port),
        _ => ("stopped", None, None, None),
    };
    // Hermes's dashboard reports its own gateway; OpenAgora only shows it.
    let gateway = port
        .and_then(|port| {
            let profile = m.profile.as_deref().unwrap_or("default");
            openagora::http::get_json(port, &format!("/api/status?profile={profile}"))
        })
        .map(|s| {
            serde_json::json!({
                "running": s["gateway_running"],
                "state": s["gateway_state"],
                "platforms": s["gateway_platforms"],
            })
        });
    Ok(serde_json::json!({
        "id": m.id,
        "name": m.name,
        "profile": m.profile,
        "hermes": report.hermes.as_ref().map(|p| p.display().to_string()),
        "installed": report.installed,
        "adopted": report.adopted,
        "ask_how_to_start": report.ask_how_to_start,
        "version": report.version.map(|v| v.to_string()),
        "min_version": MIN_VERSION.to_string(),
        "version_ok": report.version_ok(),
        "setup": report.profile.as_str(),
        "profile_dir": report.dir.as_ref().map(|p| p.display().to_string()),
        "connected": sunday::record(engine, m)?.is_some_and(|r| r.connected),
        "dashboard": dashboard,
        "url": url,
        "pid": pid,
        "gateway": gateway,
    }))
}

fn print_sunday(s: &serde_json::Value) {
    let text = |v: &serde_json::Value| v.as_str().unwrap_or("unknown").to_string();
    match s["hermes"].as_str() {
        Some(path) => println!(
            "Hermes:    {path} (version {}, Sunday needs {} or later)",
            text(&s["version"]),
            text(&s["min_version"])
        ),
        None => println!(
            "Hermes:    not installed (openagora install {})",
            text(&s["id"])
        ),
    }
    let setup = match s["setup"].as_str() {
        Some("ready") => format!("ready in {}", text(&s["profile_dir"])),
        Some("missing") => "not set up (openagora sunday setup)".into(),
        Some("foreign") => format!(
            "a profile with Sunday's name exists that OpenAgora didn't create ({})",
            text(&s["profile_dir"])
        ),
        _ => "unknown".into(),
    };
    println!("Sunday:    {setup}");
    match s["url"].as_str() {
        Some(url) => println!("Dashboard: running at {url}"),
        None => println!("Dashboard: stopped"),
    }
    if let Some(running) = s["gateway"]["running"].as_bool() {
        println!(
            "Messaging: {}",
            if running {
                "gateway running"
            } else {
                "gateway stopped"
            }
        );
    }
}
