use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use openagora::apps::{Engine, Status, tail};
use openagora::catalog::{Catalog, load_listing};
use openagora::gateway::Gateway;
use openagora::manifest::{Arch, Manifest, Os, Role};
use openagora::paths::Paths;
use openagora::sunday::{self, MIN_VERSION, Start};
use openagora::{
    agents, audit, broker, connections, context, daemon, desk, focus, history, ipc, memory, paths,
    sessions, state, t3,
};
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
        /// How to print it: `hermes` (Hermes's `{"context": …}`), or
        /// `claude-code` (plain text for a Claude Code UserPromptSubmit hook:
        /// what Sunday sent it).
        #[arg(long, default_value = "hermes")]
        format: String,
        /// Who the context is for, e.g. `sunday`.
        #[arg(long, default_value = "sunday")]
        client: String,
    },
    /// Which windows are in front, and whether OpenAgora may capture them.
    Focus {
        /// Print JSON.
        #[arg(long)]
        json: bool,
    },
    /// The desk: what you and Sunday share. Items are kept for 24 hours.
    Desk {
        #[command(subcommand)]
        action: Option<DeskCmd>,
    },
    /// The apps you allowed Sunday to see.
    Grants {
        #[command(subcommand)]
        action: Option<GrantsCmd>,
    },
    /// Coding agents on this computer (Claude Code, Codex, OpenCode, and any
    /// other MCP agent you add) and whether each can read what Sunday sends it.
    Agents {
        #[command(subcommand)]
        action: Option<AgentsCmd>,
    },
    /// Connect OpenAgora to an app that signs agents in with OAuth (T3 Code):
    /// its approval page opens, and you approve OpenAgora there.
    Connect { app: String },
    /// Forget OpenAgora's sign-in to an app.
    Disconnect { app: String },
    /// The apps OpenAgora is connected to.
    Connections {
        /// Print JSON (for the desktop app).
        #[arg(long)]
        json: bool,
    },
    /// Your sessions in your coding agents (Claude Code, Codex, OpenCode,
    /// T3 Code), and which of them Sunday may read.
    Sessions {
        #[command(subcommand)]
        action: Option<SessionsCmd>,
    },
    /// Background mode (off unless you turn it on): OpenAgora keeps what the
    /// apps you always allowed show, on this computer, for Sunday to search.
    Background {
        #[command(subcommand)]
        action: Option<BackgroundCmd>,
    },
    /// The never-capture list: apps and windows OpenAgora never captures.
    Never {
        #[command(subcommand)]
        action: Option<NeverCmd>,
    },
    /// Show the audit log: every app action and capture, with who asked and
    /// who confirmed.
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
enum DeskCmd {
    /// List what's on the desk (the default).
    List {
        /// Print JSON.
        #[arg(long)]
        json: bool,
    },
    /// Describe one item, or save its contents with --out.
    Show {
        id: String,
        /// Write the item's contents to this file.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Put text, a link or a file on the desk for Sunday.
    Add {
        /// Text (or a link) to share.
        #[arg(long, conflicts_with = "file")]
        text: Option<String>,
        /// A file to share.
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// Take one item off the desk.
    Remove { id: String },
    /// Clear the desk.
    Clear,
}

#[derive(Subcommand)]
enum GrantsCmd {
    /// List what you allowed (the default).
    List {
        /// Print JSON.
        #[arg(long)]
        json: bool,
    },
    /// Let Sunday see an app's windows. APP is the app's key, as `openagora
    /// focus` shows it (a bundle id, an executable or a window class).
    Allow {
        app: String,
        /// The app's name, for people.
        #[arg(long)]
        name: Option<String>,
        /// For the next capture only.
        #[arg(long)]
        once: bool,
        /// Let a coding agent see everything from the app on the desk,
        /// instead of Sunday seeing its windows (e.g. `--for claude-code`).
        #[arg(long = "for")]
        for_client: Option<String>,
    },
    /// Take back what you allowed for an app (by key or name).
    Revoke {
        app: String,
        /// Only what this client may do (e.g. `sunday`, `claude-code`).
        #[arg(long = "for")]
        for_client: Option<String>,
        /// Only sending to this destination (e.g. `claude-code`, `t3code`).
        #[arg(long)]
        to: Option<String>,
    },
}

#[derive(Subcommand)]
enum AgentsCmd {
    /// List them (the default).
    List {
        /// Print JSON (for the desktop app).
        #[arg(long)]
        json: bool,
    },
    /// Add OpenAgora's MCP server to an agent, with the agent's own command
    /// (asks first).
    Setup {
        id: String,
        /// Don't ask for confirmation.
        #[arg(short, long)]
        yes: bool,
        /// Claude Code: leave out the prompt hook that tells it what Sunday
        /// sent, with your next prompt.
        #[arg(long)]
        no_hook: bool,
    },
    /// Remove OpenAgora's MCP server from an agent, with the agent's own
    /// command (asks first).
    Remove {
        id: String,
        /// Don't ask for confirmation.
        #[arg(short, long)]
        yes: bool,
    },
    /// Another MCP agent you connected yourself (its MCP config runs
    /// `openagora mcp --client ID`), so Sunday can send things to it.
    Add {
        id: String,
        /// Its name, for people.
        #[arg(long)]
        name: String,
    },
    /// Forget an agent you added.
    Forget { id: String },
}

#[derive(Subcommand)]
enum NeverCmd {
    /// Show the built-in list and your own rules (the default).
    List {
        /// Print JSON.
        #[arg(long)]
        json: bool,
    },
    /// Never capture an app, or windows whose title has certain words.
    Add {
        /// A name for the rule.
        name: String,
        /// An app's name or key. Repeat for more.
        #[arg(long = "app")]
        apps: Vec<String>,
        /// Words in a window title. Repeat for more.
        #[arg(long = "title")]
        titles: Vec<String>,
    },
    /// Remove one of your own rules.
    Remove { name: String },
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
    /// Sunday's memory: Hermes's memory for Sunday, and the digest that keeps
    /// it up to date with your coding sessions.
    Memory {
        #[command(subcommand)]
        action: Option<MemoryCmd>,
    },
}

#[derive(Subcommand)]
enum MemoryCmd {
    /// Where Sunday's memory stands (the default).
    Status {
        /// Print JSON (for the desktop app).
        #[arg(long)]
        json: bool,
    },
    /// Give Sunday long-term memory with a memory provider, set up with
    /// Hermes's own commands for Sunday's profile only (asks first).
    Use {
        #[arg(value_parser = ["holographic"])]
        provider: String,
        /// Don't ask for confirmation.
        #[arg(short, long)]
        yes: bool,
    },
    /// Back to Hermes's built-in memory only (`hermes -p sunday memory off`).
    Off {
        /// Don't ask for confirmation.
        #[arg(short, long)]
        yes: bool,
    },
    /// Keep Sunday's memory up to date with your coding sessions: every two
    /// hours, while OpenAgora runs, Sunday reads what changed and saves what's
    /// worth remembering (asks first).
    Digest {
        #[arg(value_parser = ["on", "off"])]
        state: String,
        /// Don't ask for confirmation.
        #[arg(short, long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
enum BackgroundCmd {
    /// Whether it's on, and what it kept (the default).
    Status {
        /// Print JSON.
        #[arg(long)]
        json: bool,
    },
    /// Turn it on: asks first.
    On {
        /// Don't ask (for scripts and OpenAgora's window, which asks itself).
        #[arg(long)]
        yes: bool,
    },
    /// Turn it off. What it kept stays until it expires or you clear it.
    Off,
    /// Stop capturing for a while (until you resume, without --minutes).
    Pause {
        #[arg(long)]
        minutes: Option<u64>,
    },
    /// Capture again after a pause.
    Resume,
    /// Delete everything it kept.
    Clear {
        #[arg(long)]
        yes: bool,
    },
    /// How many days to keep captures (1 to 30; 7 unless you change it).
    Keep { days: u32 },
    /// Search what it kept, as Sunday does.
    Search {
        /// Words to find; leave out for the latest captures.
        query: Option<String>,
        /// "today", "yesterday", a date (2026-10-11) or a time.
        #[arg(long)]
        since: Option<String>,
        /// Only this app.
        #[arg(long)]
        app: Option<String>,
        /// Print JSON.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum SessionsCmd {
    /// Your sessions since a time, as Sunday reads them (the default: the
    /// last 24 hours).
    List {
        /// "today", "yesterday", a date (2026-10-11) or a time.
        #[arg(long)]
        since: Option<String>,
        /// Only this agent, e.g. claude-code.
        #[arg(long)]
        agent: Option<String>,
        /// Print JSON.
        #[arg(long)]
        json: bool,
    },
    /// Let Sunday always read an agent's sessions.
    Allow { agent: String },
    /// Take that back: Sunday asks again before reading them.
    Revoke { agent: String },
}

fn main() -> ExitCode {
    openagora::process::stop_fd_inheritance();
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
                SundayCmd::Memory { action } => {
                    match action.unwrap_or(MemoryCmd::Status { json: false }) {
                        MemoryCmd::Status { json } => {
                            let status = memory::status(&engine, m)?;
                            if json {
                                println!("{}", serde_json::to_string_pretty(&status)?);
                            } else {
                                print_memory(&status);
                            }
                        }
                        MemoryCmd::Use { provider, yes } => {
                            let commands = memory::use_commands(&provider);
                            let question = format!(
                                "Give Sunday long-term memory with {provider}? It keeps facts in a local database in Sunday's profile: no account, network or model calls. OpenAgora runs Hermes's own commands, for the sunday profile only:\n  {}\nThe first installs the plugin from Hermes's catalog, with its Python packages.",
                                commands.join("\n  ")
                            );
                            if !confirm(&question, yes)? {
                                println!("Nothing was changed.");
                                return Ok(());
                            }
                            let mut entry = audit::Entry::new("user", "memory_provider", &provider);
                            entry.decision = "confirmed".into();
                            entry.via = Some("command line".into());
                            let done = memory::use_provider(&engine, m, &provider);
                            entry.outcome = match &done {
                                Ok(()) => "ok".into(),
                                Err(e) => format!("failed: {e:#}"),
                            };
                            audit::record(&engine.paths, &entry)?;
                            done?;
                            println!(
                                "Sunday now remembers with {provider}. Start a new Sunday session to pick it up."
                            );
                        }
                        MemoryCmd::Off { yes } => {
                            if !confirm(
                                "Switch Sunday back to Hermes's built-in memory only (`hermes -p sunday memory off`)? The provider's facts stay in Sunday's profile.",
                                yes,
                            )? {
                                println!("Nothing was changed.");
                                return Ok(());
                            }
                            memory::memory_off(&engine, m)?;
                            let mut entry = audit::Entry::new("user", "memory_provider", "off");
                            entry.decision = "confirmed".into();
                            entry.via = Some("command line".into());
                            entry.outcome = "ok".into();
                            audit::record(&engine.paths, &entry)?;
                            println!("Sunday uses Hermes's built-in memory only.");
                        }
                        MemoryCmd::Digest { state, yes } if state == "on" => {
                            let agents = memory::readable_agents();
                            let names: Vec<&str> = agents.iter().map(|(_, n)| n.as_str()).collect();
                            let question = format!(
                                "Keep Sunday's memory up to date with your coding sessions? Every two hours while OpenAgora runs, if a session changed, Sunday reads your sessions{} and saves what's worth remembering: decisions, conventions, open problems (with Hermes's one-shot mode, `hermes -p sunday -z …`, and only memory and OpenAgora's tools). Sunday may then always read those sessions; what it reads goes to the model provider it uses, and each run costs one turn.",
                                if names.is_empty() {
                                    String::new()
                                } else {
                                    format!(" in {}", names.join(", "))
                                }
                            );
                            if !confirm(&question, yes)? {
                                println!("Nothing was changed.");
                                return Ok(());
                            }
                            memory::digest_on(&engine, m, &agents)?;
                            let mut entry = audit::Entry::new("user", "memory_digest", "on");
                            entry.decision = "confirmed".into();
                            entry.via = Some("command line".into());
                            entry.outcome = if names.is_empty() {
                                "on".into()
                            } else {
                                format!("on, reading {}", names.join(", "))
                            };
                            audit::record(&engine.paths, &entry)?;
                            println!(
                                "Sunday's memory digest is on (every two hours while OpenAgora runs)."
                            );
                        }
                        MemoryCmd::Digest { yes, .. } => {
                            if !confirm("Turn off Sunday's memory digest?", yes)? {
                                println!("Nothing was changed.");
                                return Ok(());
                            }
                            memory::digest_off(&engine.paths)?;
                            let mut entry = audit::Entry::new("user", "memory_digest", "off");
                            entry.decision = "confirmed".into();
                            entry.via = Some("command line".into());
                            entry.outcome = "off".into();
                            audit::record(&engine.paths, &entry)?;
                            println!("Sunday's memory digest is off.");
                        }
                    }
                }
            }
        }
        Cmd::Background { action } => {
            let paths = Paths::from_env()?;
            let record = |action: &str, outcome: String| -> anyhow::Result<()> {
                let mut entry = audit::Entry::new("user", "background", action);
                entry.decision = "confirmed".into();
                entry.via = Some("command line".into());
                entry.outcome = outcome;
                audit::record(&paths, &entry)
            };
            match action.unwrap_or(BackgroundCmd::Status { json: false }) {
                BackgroundCmd::Status { json } => {
                    let settings = history::settings(&paths);
                    history::prune(&paths, state::now(), settings.keep_days)?;
                    let status = history::status(&paths)?;
                    if json {
                        println!("{}", serde_json::to_string_pretty(&status)?);
                    } else {
                        let state = match (status.settings.on, status.paused) {
                            (false, _) => "off".to_string(),
                            (true, true) => match status.settings.paused_until {
                                Some(history::UNTIL_RESUMED) | None => {
                                    "on, paused until you resume".into()
                                }
                                Some(until) => {
                                    format!("on, paused until {} (UTC)", sessions::stamp(until))
                                }
                            },
                            (true, false) => "on".into(),
                        };
                        println!("Background mode is {state}.");
                        println!(
                            "Kept: {} captures ({} KB), for {} days.",
                            status.captures,
                            status.bytes / 1024,
                            status.settings.keep_days
                        );
                    }
                }
                BackgroundCmd::On { yes } => {
                    let keep = history::settings(&paths).keep_days;
                    let question = format!(
                        "Turn on background mode? While OpenAgora runs, it looks at the window you're in when you switch windows, and every few minutes while you stay, and keeps what apps you always allowed Sunday to see show (a picture and the window's text) on this computer for {keep} days, for Sunday to search. Apps you allowed once, and apps and windows on the never-capture list, are never captured. What Sunday reads from it is sent to the model provider you chose. You can pause it or clear it any time."
                    );
                    if !confirm(&question, yes)? {
                        println!("Nothing was changed.");
                        return Ok(());
                    }
                    history::update(&paths, |s| {
                        s.on = true;
                        s.paused_until = None;
                    })?;
                    record("on", format!("on, keeping {keep} days"))?;
                    println!(
                        "Background mode is on: OpenAgora keeps what your allowed apps show for {keep} days."
                    );
                }
                BackgroundCmd::Off => {
                    history::update(&paths, |s| {
                        s.on = false;
                        s.paused_until = None;
                    })?;
                    record("off", "off".into())?;
                    println!(
                        "Background mode is off. What it kept stays until it expires; `openagora background clear` deletes it now."
                    );
                }
                BackgroundCmd::Pause { minutes } => {
                    let until = match minutes {
                        Some(0) => anyhow::bail!("pause for at least a minute"),
                        Some(m) => state::now()
                            .saturating_add(m.saturating_mul(60))
                            .min(history::UNTIL_RESUMED),
                        None => history::UNTIL_RESUMED,
                    };
                    let settings = history::update(&paths, |s| s.paused_until = Some(until))?;
                    record(
                        "pause",
                        match minutes {
                            Some(m) => format!("for {m} minutes"),
                            None => "until resumed".into(),
                        },
                    )?;
                    if !settings.on {
                        println!("Background mode is off anyway.");
                    } else if let Some(m) = minutes {
                        println!("Background mode is paused for {m} minutes.");
                    } else {
                        println!("Background mode is paused until you resume it.");
                    }
                }
                BackgroundCmd::Resume => {
                    history::update(&paths, |s| s.paused_until = None)?;
                    record("resume", "resumed".into())?;
                    println!("Background mode is no longer paused.");
                }
                BackgroundCmd::Clear { yes } => {
                    if !confirm("Delete everything background mode kept?", yes)? {
                        println!("Nothing was changed.");
                        return Ok(());
                    }
                    let removed = history::clear(&paths)?;
                    record("clear", format!("{removed} captures deleted"))?;
                    println!("Deleted {removed} captures.");
                }
                BackgroundCmd::Keep { days } => {
                    let days = history::check_keep(days)?;
                    history::update(&paths, |s| s.keep_days = days)?;
                    let removed = history::prune(&paths, state::now(), days)?;
                    record("keep", format!("{days} days"))?;
                    println!(
                        "Background mode keeps captures for {days} days{}.",
                        if removed > 0 {
                            format!(" ({removed} older ones deleted)")
                        } else {
                            String::new()
                        }
                    );
                }
                BackgroundCmd::Search {
                    query,
                    since,
                    app,
                    json,
                } => {
                    let settings = history::settings(&paths);
                    let now = state::now();
                    history::prune(&paths, now, settings.keep_days)?;
                    let from = match since.as_deref() {
                        Some(text) => Some(
                            sessions::since(text, now, sessions::local_offset())
                                .or_else(|| sessions::parse_time(text))
                                .ok_or_else(|| anyhow::anyhow!("{text:?} isn't a time I know"))?,
                        ),
                        None => None,
                    };
                    let hits = history::search(
                        &paths,
                        query.as_deref().unwrap_or(""),
                        from,
                        app.as_deref(),
                        50,
                    )?;
                    if json {
                        println!("{}", serde_json::to_string_pretty(&hits)?);
                    } else if hits.is_empty() {
                        println!("Nothing found.");
                    } else {
                        for h in hits {
                            println!(
                                "{}  {} (UTC)  {} — {}: {}",
                                h.id,
                                sessions::stamp(h.at),
                                h.app,
                                h.title,
                                h.snippet.replace('\n', " ")
                            );
                        }
                    }
                }
            }
        }
        Cmd::Sessions { action } => {
            let paths = Paths::from_env()?;
            match action.unwrap_or(SessionsCmd::List {
                since: None,
                agent: None,
                json: false,
            }) {
                SessionsCmd::List { since, agent, json } => {
                    let gateway = Gateway::new(paths.clone(), cli.catalog.clone());
                    let now = state::now();
                    let from = match since.as_deref() {
                        Some(text) => sessions::since(text, now, sessions::local_offset())
                            .ok_or_else(|| anyhow::anyhow!("{text:?} isn't a time I know"))?,
                        None => now.saturating_sub(24 * 3600),
                    };
                    let (found, notes) = gateway.gather_sessions(from, agent.as_deref());
                    if json {
                        let listed: Vec<serde_json::Value> = found
                            .iter()
                            .map(|(agent, s)| {
                                serde_json::json!({
                                    "agent": agent,
                                    "id": s.id,
                                    "title": s.title,
                                    "project": s.project,
                                    "started_at": s.started_at,
                                    "updated_at": s.updated_at,
                                    "prompts": s.turns.iter().filter(|t| t.role == sessions::Role::User).count(),
                                })
                            })
                            .collect();
                        println!("{}", serde_json::to_string_pretty(&listed)?);
                    } else {
                        if found.is_empty() {
                            println!("No sessions since {} (UTC).", sessions::stamp(from));
                        }
                        for (agent, session) in &found {
                            print!("{}", sessions::listing(agent, session));
                        }
                        for note in notes {
                            eprintln!("{note}");
                        }
                    }
                }
                SessionsCmd::Allow { agent } => {
                    let Some((id, name)) = memory::readable_agents()
                        .into_iter()
                        .find(|(id, _)| *id == agent)
                    else {
                        anyhow::bail!(
                            "OpenAgora can't read {agent:?}'s sessions on this computer (see openagora sessions)"
                        );
                    };
                    memory::allow_sessions(&paths, &id, &name)?;
                    let mut entry = audit::Entry::new("user", "allow_sessions", &id);
                    entry.decision = "confirmed".into();
                    entry.via = Some("command line".into());
                    entry.outcome = "always".into();
                    audit::record(&paths, &entry)?;
                    println!("Sunday may always read your {name} sessions.");
                }
                SessionsCmd::Revoke { agent } => {
                    let removed = memory::revoke_sessions(&paths, &agent)?;
                    let mut entry = audit::Entry::new("user", "revoke_sessions", &agent);
                    entry.decision = "confirmed".into();
                    entry.via = Some("command line".into());
                    entry.outcome = format!("{removed} grants");
                    audit::record(&paths, &entry)?;
                    println!("Sunday asks again before reading your {agent} sessions.");
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
        Cmd::Context {
            hook,
            format,
            client,
        } => {
            if format == "claude-code" {
                // Claude Code's prompt hook: what Sunday sent this agent since
                // it was last told, as plain text Claude Code adds to the
                // user's prompt. Claude Code's own event on stdin isn't needed.
                if hook {
                    let mut input = String::new();
                    let _ = std::io::Read::read_to_string(&mut std::io::stdin(), &mut input);
                }
                let paths = Paths::from_env()?;
                if let Some((text, told)) =
                    openagora::gateway::Gateway::agent_notice(&paths, &client)?
                {
                    println!("{text}");
                    openagora::inbox::mark_told(&paths, &client, &told)?;
                }
                return Ok(());
            }
            if format != "hermes" {
                anyhow::bail!("unknown --format {format:?} (hermes or claude-code)");
            }
            let payload = if hook {
                let mut input = String::new();
                std::io::Read::read_to_string(&mut std::io::stdin(), &mut input)?;
                serde_json::from_str(&input).unwrap_or(serde_json::Value::Null)
            } else {
                // Someone at this computer's command line.
                serde_json::json!({"extra": {"is_first_turn": true, "platform": "cli"}})
            };
            // The running engine also knows the user's screen; without it,
            // the hook says the rest.
            let paths = Paths::from_env()?;
            let turn = match context_from_service(&paths, &client, &payload) {
                Some(turn) => turn,
                None => {
                    let catalog = load_catalog(&cli.catalog)?;
                    let engine = Engine::new()?;
                    let live = context::Live::default();
                    context::for_turn(&engine, &catalog, &client, &payload, &live)?
                }
            };
            match (hook, &turn.text) {
                (true, Some(text)) => println!("{}", serde_json::json!({ "context": text })),
                (true, None) => {}
                (false, text) => println!("{}", text.as_deref().unwrap_or_default()),
            }
            // What this answer tells Sunday counts as told only now that it
            // is on its way to Hermes. If that can't be noted, Sunday is
            // told again next turn.
            if let Some(snapshot) = &turn.snapshot {
                if let Err(e) = context::remember(&paths, &client, &payload, snapshot) {
                    eprintln!("openagora: noting what Sunday was told: {e:#}");
                }
            }
        }
        Cmd::Focus { json } => {
            let paths = Paths::from_env()?;
            let windows = focus::windows().map_err(|why| anyhow::anyhow!(why))?;
            let rows = windows
                .iter()
                .map(|w| {
                    let source = broker::Source {
                        app: &w.app,
                        key: &w.key,
                        title: w.title.as_deref(),
                        exe: w.exe.as_deref(),
                    };
                    let never = broker::never(&paths, &source)?;
                    let allowed =
                        broker::allowed(&paths, "sunday", &w.key, "sunday", broker::IMAGES)?;
                    Ok((w, never, allowed))
                })
                .collect::<anyhow::Result<Vec<_>>>()?;
            if json {
                let items: Vec<serde_json::Value> = rows
                    .iter()
                    .map(|(w, never, allowed)| {
                        let mut v = serde_json::to_value(w).unwrap_or_default();
                        v["never"] = serde_json::json!(never);
                        v["allowed"] = serde_json::json!(allowed);
                        v
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&items)?);
            } else if rows.is_empty() {
                println!("No windows on screen.");
            } else {
                println!("Front to back:");
                for (w, never, allowed) in rows {
                    let state = match (never, allowed) {
                        (Some(rule), _) => format!("never captured ({rule})"),
                        (None, Some(broker::Scope::Always)) => "Sunday may see it".into(),
                        (None, Some(broker::Scope::Once)) => "allowed once".into(),
                        (None, None) => "not allowed".into(),
                    };
                    println!(
                        "  {} [{}] {}: {state}",
                        w.app,
                        w.key,
                        w.title
                            .as_deref()
                            .map(|t| format!("\u{201c}{t}\u{201d}"))
                            .unwrap_or_else(|| "(title hidden)".into())
                    );
                }
            }
        }
        Cmd::Desk { action } => {
            let paths = Paths::from_env()?;
            match action.unwrap_or(DeskCmd::List { json: false }) {
                DeskCmd::List { json } => {
                    let items = desk::list(&paths)?;
                    if json {
                        println!("{}", serde_json::to_string_pretty(&items)?);
                    } else if items.is_empty() {
                        println!("The desk is empty.");
                    } else {
                        for item in items {
                            println!(
                                "{}  {}  (for {}; until {})",
                                item.id,
                                item.describe(),
                                item.shared_with.join(", "),
                                item.expires_at
                            );
                        }
                    }
                }
                DeskCmd::Show { id, out } => {
                    let item = desk::get(&paths, &id)?
                        .ok_or_else(|| anyhow::anyhow!("there's no desk item {id}"))?;
                    match out {
                        Some(out) => {
                            std::fs::write(&out, desk::payload(&paths, &item)?)?;
                            println!("Saved {} to {}.", item.id, out.display());
                        }
                        None => println!("{}", serde_json::to_string_pretty(&item)?),
                    }
                }
                DeskCmd::Add { text, file } => {
                    let new = match (text, file) {
                        (Some(text), _) => {
                            let link = text.starts_with("https://") || text.starts_with("http://");
                            desk::New {
                                kind: if link {
                                    desk::Kind::Link
                                } else {
                                    desk::Kind::Text
                                },
                                mime: if link { "text/uri-list" } else { "text/plain" }.into(),
                                source: desk::Source {
                                    app: "Command line".into(),
                                    ..Default::default()
                                },
                                shared_with: vec!["sunday".into()],
                                added_by: "user".into(),
                                bytes: text.into_bytes(),
                                name: None,
                            }
                        }
                        (None, Some(file)) => {
                            let bytes = std::fs::read(&file)?;
                            let picture = desk::image_type(&bytes);
                            desk::New {
                                kind: if picture.is_some() {
                                    desk::Kind::Image
                                } else {
                                    desk::Kind::File
                                },
                                mime: picture.unwrap_or("application/octet-stream").into(),
                                source: desk::Source {
                                    app: "Files".into(),
                                    ..Default::default()
                                },
                                shared_with: vec!["sunday".into()],
                                added_by: "user".into(),
                                bytes,
                                name: file.file_name().map(|n| n.to_string_lossy().into_owned()),
                            }
                        }
                        (None, None) => anyhow::bail!("give --text or --file"),
                    };
                    let item = desk::add(&paths, new)?;
                    let mut entry = audit::Entry::new("user", "desk_add", &item.source.app);
                    entry.via = Some("command line".into());
                    entry.outcome = item.id.clone();
                    audit::record(&paths, &entry)?;
                    println!(
                        "Put {} on the desk for Sunday: {}.",
                        item.id,
                        item.describe()
                    );
                }
                DeskCmd::Remove { id } => {
                    if desk::remove(&paths, &id)? {
                        println!("Took {id} off the desk.");
                    } else {
                        println!("There's no desk item {id}.");
                    }
                }
                DeskCmd::Clear => {
                    let count = desk::clear(&paths)?;
                    let mut entry = audit::Entry::new("user", "desk_clear", "desk");
                    entry.via = Some("command line".into());
                    entry.outcome = format!("{count} item(s)");
                    audit::record(&paths, &entry)?;
                    println!("Cleared the desk ({count} item(s)).");
                }
            }
        }
        Cmd::Agents { action } => {
            let paths = Paths::from_env()?;
            match action.unwrap_or(AgentsCmd::List { json: false }) {
                AgentsCmd::List { json } => {
                    let listed = agents_json(&paths)?;
                    if json {
                        println!("{}", serde_json::to_string_pretty(&listed)?);
                    } else {
                        for a in listed.as_array().into_iter().flatten() {
                            let state = if a["custom"] == true {
                                "added by you".to_string()
                            } else if a["installed"] != true {
                                "not installed".to_string()
                            } else if a["set_up"] == true {
                                "set up: it can read what Sunday sends it".to_string()
                            } else {
                                format!(
                                    "installed, not set up (openagora agents setup {})",
                                    a["id"].as_str().unwrap_or_default()
                                )
                            };
                            println!(
                                "{:<12} {:<12} {state}",
                                a["id"].as_str().unwrap_or_default(),
                                a["name"].as_str().unwrap_or_default()
                            );
                        }
                    }
                }
                AgentsCmd::Setup { id, yes, no_hook } => {
                    let d = agents::descriptor(&id).ok_or_else(|| {
                        anyhow::anyhow!("no coding agent {id:?} that OpenAgora can set up (claude-code, codex, opencode); for another one, see: openagora agents add --help")
                    })?;
                    let found = agents::find(d);
                    if found.exe.is_none() {
                        anyhow::bail!(
                            "{} isn't installed on this computer (see {})",
                            d.name,
                            d.homepage
                        );
                    }
                    let hook = d.id == "claude-code" && !no_hook;
                    let mut commands = vec![agents::display(
                        d.command,
                        &agents::add_args(&found, &paths.stable_exe()),
                    )];
                    if hook {
                        commands.extend(
                            agents::claude_hook::install_args(&paths)
                                .iter()
                                .map(|args| agents::display(d.command, args)),
                        );
                    }
                    let question = format!(
                        "Add OpenAgora's MCP server to {}'s settings, so it can read what Sunday sends it{}? OpenAgora runs {}'s own commands:\n  {}\n",
                        d.name,
                        if hook {
                            ", and a plugin that tells it about new items with your next prompt"
                        } else {
                            ""
                        },
                        d.name,
                        commands.join("\n  ")
                    );
                    if !confirm(&question, yes)? {
                        println!("Nothing changed.");
                        return Ok(());
                    }
                    let exe = paths::install_stable_exe(&paths)?;
                    let mut entry = audit::Entry::new("user", "agent_setup", &id);
                    entry.decision = "confirmed".into();
                    entry.via = Some("command line".into());
                    let ran = agents::set_up(&found, &exe);
                    entry.outcome = match &ran {
                        Ok(ran) => ran.clone(),
                        Err(e) => format!("failed: {e:#}"),
                    };
                    audit::record(&paths, &entry)?;
                    ran?;
                    agents::mark_set_up(&paths, &id)?;
                    println!(
                        "{} can now read what Sunday sends it (ask it: \"what did Sunday send me?\").",
                        d.name
                    );
                    if hook {
                        let mut entry = audit::Entry::new("user", "agent_hook", &id);
                        entry.decision = "confirmed".into();
                        entry.via = Some("command line".into());
                        match agents::claude_hook::install(&found, &paths, &exe) {
                            Ok(ran) => {
                                entry.outcome = ran.join("; ");
                                println!(
                                    "It also hears about new items with your next prompt (restart open Claude Code sessions, or run /reload-plugins in them)."
                                );
                            }
                            Err(e) => {
                                entry.outcome = format!("failed: {e:#}");
                                eprintln!(
                                    "warning: the prompt hook wasn't installed ({e:#}); ask Claude Code \"what did Sunday send me?\" instead."
                                );
                            }
                        }
                        audit::record(&paths, &entry)?;
                    }
                }
                AgentsCmd::Remove { id, yes } => {
                    let d = agents::descriptor(&id).ok_or_else(|| {
                        anyhow::anyhow!("no coding agent {id:?} that OpenAgora set up")
                    })?;
                    let found = agents::find(d);
                    let Some(args) = agents::remove_args(&found) else {
                        anyhow::bail!("{}", agents::remove_by_hand(&found));
                    };
                    let mut commands = vec![agents::display(d.command, &args)];
                    if d.id == "claude-code" {
                        commands.extend(
                            agents::claude_hook::uninstall_args()
                                .iter()
                                .map(|args| agents::display(d.command, args)),
                        );
                    }
                    let question = format!(
                        "Remove OpenAgora's MCP server from {}? OpenAgora runs:\n  {}\n",
                        d.name,
                        commands.join("\n  ")
                    );
                    if !confirm(&question, yes)? {
                        println!("Nothing changed.");
                        return Ok(());
                    }
                    let mut entry = audit::Entry::new("user", "agent_remove", &id);
                    entry.decision = "confirmed".into();
                    entry.via = Some("command line".into());
                    let ran = agents::remove(&found);
                    let mut outcome = match &ran {
                        Ok(ran) => ran.clone(),
                        Err(e) => format!("failed: {e:#}"),
                    };
                    // The prompt hook goes too, if it was there.
                    if d.id == "claude-code"
                        && agents::claude_hook::marketplace_dir(&paths).exists()
                    {
                        for result in agents::claude_hook::uninstall(&found) {
                            match result {
                                Ok(ran) => outcome.push_str(&format!("; {ran}")),
                                Err(e) => outcome.push_str(&format!("; {e:#}")),
                            }
                        }
                        let _ =
                            std::fs::remove_dir_all(agents::claude_hook::marketplace_dir(&paths));
                    }
                    entry.outcome = outcome;
                    audit::record(&paths, &entry)?;
                    ran?;
                    agents::mark_removed(&paths, &id)?;
                    println!("{} no longer has OpenAgora's MCP server.", d.name);
                }
                AgentsCmd::Add { id, name } => {
                    agents::add_custom(&paths, &id, &name)?;
                    let command =
                        agents::display("", &agents::server_command(&paths.stable_exe(), &id));
                    println!(
                        "Sunday can now send things to {name}. In {name}'s MCP settings, add a server named \"openagora\" that runs:\n {command}"
                    );
                }
                AgentsCmd::Forget { id } => {
                    if agents::forget_custom(&paths, &id)? {
                        println!("Forgot {id}.");
                    } else {
                        println!("No agent {id} was added.");
                    }
                }
            }
        }
        Cmd::Connect { app } => {
            let paths = Paths::from_env()?;
            let catalog = load_catalog(&cli.catalog)?;
            let m = listing(&catalog, &app)?;
            let oauth = m
                .agent
                .as_ref()
                .is_some_and(|a| a.mcp.auth == openagora::manifest::McpAuth::Oauth);
            if !oauth {
                anyhow::bail!(
                    "{} doesn't sign agents in with OAuth; there's nothing to connect",
                    m.name
                );
            }
            let engine = Engine::new()?;
            let port = match engine.status(&m.id)? {
                Status::Running(r) => r.port,
                Status::Stopped => None,
            }
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "{} isn't running. Start it first: openagora start {}",
                    m.name,
                    m.id
                )
            })?;
            let name = m.name.clone();
            let open = |url: &str| -> anyhow::Result<()> {
                println!(
                    "Approve OpenAgora on {name}'s page (choose Supervised or more, so Sunday can start threads):\n  {url}"
                );
                open_in_browser(url);
                Ok(())
            };
            let mut entry = audit::Entry::new("user", "connect", &m.id);
            entry.via = Some(format!("{}'s page (command line)", m.name));
            let result = t3::connect(&paths, &m.id, port, &open);
            match &result {
                Ok(c) => {
                    entry.decision = "confirmed".into();
                    entry.outcome = c.scope.clone().unwrap_or_else(|| "connected".into());
                }
                Err(e) => {
                    entry.decision = "failed".into();
                    entry.outcome = format!("{e:#}");
                }
            }
            audit::record(&paths, &entry)?;
            let connection = result?;
            println!("OpenAgora is connected to {}.", m.name);
            if !connection.can_act() {
                println!(
                    "{} gave OpenAgora read-only access: Sunday can read threads but not start them. Connect again and choose Supervised or more to let it.",
                    m.name
                );
            }
        }
        Cmd::Disconnect { app } => {
            let paths = Paths::from_env()?;
            let removed = connections::remove(&paths, &app)?;
            let mut entry = audit::Entry::new("user", "disconnect", &app);
            entry.via = Some("command line".into());
            entry.outcome = if removed {
                "forgotten"
            } else {
                "not connected"
            }
            .into();
            audit::record(&paths, &entry)?;
            if removed {
                println!(
                    "OpenAgora forgot its sign-in to {app}. The app may still list OpenAgora among its connections until you remove it there."
                );
            } else {
                println!("OpenAgora wasn't connected to {app}.");
            }
        }
        Cmd::Connections { json } => {
            let paths = Paths::from_env()?;
            let all = connections::list(&paths)?;
            if json {
                let rows: Vec<serde_json::Value> = all
                    .iter()
                    .map(|c| {
                        serde_json::json!({
                            "app": c.app,
                            "connected_at": c.connected_at,
                            "expires_at": c.expires_at,
                            "usable": c.usable(),
                            "can_act": c.can_act(),
                            "scope": c.scope,
                            "port": c.port,
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&rows)?);
            } else if all.is_empty() {
                println!("OpenAgora isn't connected to any app.");
            } else {
                for c in all {
                    let state = if c.usable() {
                        if c.can_act() {
                            "connected"
                        } else {
                            "connected, read-only"
                        }
                    } else {
                        "signed out: connect again"
                    };
                    println!("{:<10} {state}", c.app);
                }
            }
        }
        Cmd::Grants { action } => {
            let paths = Paths::from_env()?;
            match action.unwrap_or(GrantsCmd::List { json: false }) {
                GrantsCmd::List { json } => {
                    let grants = broker::grants(&paths)?;
                    if json {
                        println!("{}", serde_json::to_string_pretty(&grants)?);
                    } else if grants.is_empty() {
                        println!("Sunday isn't allowed to see any app yet.");
                    } else {
                        for g in grants {
                            println!(
                                "{} [{}]: {} may see it ({:?}, {})",
                                g.source_name, g.source, g.client, g.scope, g.family
                            );
                        }
                    }
                }
                GrantsCmd::Allow {
                    app,
                    name,
                    once,
                    for_client,
                } => {
                    let name = name.unwrap_or_else(|| app.clone());
                    let source = broker::Source {
                        app: &name,
                        key: &app,
                        title: None,
                        exe: None,
                    };
                    if let Some(rule) = broker::never(&paths, &source)? {
                        anyhow::bail!("{name} is on the never-capture list ({rule})");
                    }
                    let scope = if once {
                        broker::Scope::Once
                    } else {
                        broker::Scope::Always
                    };
                    // Sunday sees an app's windows; a coding agent sees what
                    // comes from the app on the desk.
                    let (client, family) = match &for_client {
                        Some(agent) if agents::is_agent(&paths, agent) => {
                            (agent.clone(), broker::ANY)
                        }
                        Some(other) => {
                            anyhow::bail!("{other:?} isn't a coding agent (see openagora agents)")
                        }
                        None => ("sunday".to_string(), broker::IMAGES),
                    };
                    broker::grant(
                        &paths,
                        broker::Grant {
                            client: client.clone(),
                            source: app.clone(),
                            source_name: name.clone(),
                            destination: client.clone(),
                            family: family.into(),
                            scope,
                            granted_at: state::now(),
                        },
                    )?;
                    let mut entry = audit::Entry::new("user", "allow_app", &app);
                    entry.decision = "confirmed".into();
                    entry.via = Some("command line".into());
                    entry.outcome =
                        format!("{} for {client}", if once { "once" } else { "always" });
                    audit::record(&paths, &entry)?;
                    println!(
                        "{} may now see {name}.",
                        agents::name(&paths, &client).unwrap_or_else(|| "Sunday".into())
                    );
                }
                GrantsCmd::Revoke {
                    app,
                    for_client,
                    to,
                } => {
                    let removed =
                        broker::revoke_where(&paths, &app, for_client.as_deref(), to.as_deref())?;
                    let mut entry = audit::Entry::new("user", "revoke", &app);
                    entry.via = Some("command line".into());
                    entry.outcome = format!("{removed} grant(s)");
                    audit::record(&paths, &entry)?;
                    if removed == 0 {
                        println!("Nothing was allowed for {app}.");
                    } else if for_client.is_none() && to.is_none() {
                        println!("Sunday may no longer see {app}.");
                    } else {
                        println!("Took back {removed} permission(s) for {app}.");
                    }
                }
            }
        }
        Cmd::Never { action } => {
            let paths = Paths::from_env()?;
            match action.unwrap_or(NeverCmd::List { json: false }) {
                NeverCmd::List { json } => {
                    let built_in = broker::built_in();
                    let own = broker::user_rules(&paths)?;
                    if json {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(
                                &serde_json::json!({"built_in": built_in, "user": own})
                            )?
                        );
                    } else {
                        println!("Built in:");
                        for r in built_in {
                            println!("  {}", r.name);
                        }
                        println!("Yours:");
                        if own.is_empty() {
                            println!("  (none)");
                        }
                        for r in own {
                            let mut parts = r.apps.clone();
                            parts.extend(r.titles.iter().map(|t| format!("title: {t}")));
                            println!("  {} ({})", r.name, parts.join(", "));
                        }
                    }
                }
                NeverCmd::Add { name, apps, titles } => {
                    broker::add_rule(
                        &paths,
                        broker::Rule {
                            name: name.clone(),
                            apps,
                            titles,
                        },
                    )?;
                    let mut entry = audit::Entry::new("user", "never_add", &name);
                    entry.via = Some("command line".into());
                    audit::record(&paths, &entry)?;
                    println!("OpenAgora will never capture {name}.");
                }
                NeverCmd::Remove { name } => {
                    if broker::remove_rule(&paths, &name)? {
                        let mut entry = audit::Entry::new("user", "never_remove", &name);
                        entry.via = Some("command line".into());
                        audit::record(&paths, &entry)?;
                        println!("Removed {name} from your never-capture list.");
                    } else {
                        println!("Your never-capture list has no {name}.");
                    }
                }
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

/// Asks the running engine for the hook's answer. None when it isn't running
/// or doesn't answer in time.
fn context_from_service(
    paths: &Paths,
    client: &str,
    payload: &serde_json::Value,
) -> Option<context::Turn> {
    let mut stream = ipc::connect(paths).ok()?;
    ipc::hello(&mut stream, "hook", client).ok()?;
    let request = serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "context/turn",
        "params": {"payload": payload},
    });
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let answer = (|| -> anyhow::Result<context::Turn> {
            writeln!(stream, "{request}")?;
            stream.flush()?;
            let mut reader = std::io::BufReader::new(stream);
            let reply = ipc::read_message(&mut reader)?
                .ok_or_else(|| anyhow::anyhow!("the engine closed the connection"))?;
            if let Some(error) = reply.get("error") {
                anyhow::bail!("{}", error["message"]);
            }
            Ok(context::Turn {
                text: reply["result"]["context"].as_str().map(str::to_string),
                snapshot: serde_json::from_value(reply["result"]["snapshot"].clone()).ok(),
            })
        })();
        let _ = tx.send(answer);
    });
    // Hermes gives the hook 20 seconds; leave time for the fallback.
    rx.recv_timeout(std::time::Duration::from_secs(12))
        .ok()?
        .ok()
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

/// The coding agents, for `openagora agents`: each built-in agent as found on
/// this computer (asked in parallel: each agent's own command is slow to
/// start), then the ones the user added.
fn agents_json(paths: &Paths) -> anyhow::Result<serde_json::Value> {
    let engine = paths.stable_exe();
    let found: Vec<serde_json::Value> = std::thread::scope(|scope| {
        let handles: Vec<_> = agents::AGENTS
            .iter()
            .map(|d| {
                let engine = &engine;
                scope.spawn(move || {
                    let found = agents::find(d);
                    let set_up = found.exe.is_some() && agents::is_set_up(&found).unwrap_or(false);
                    let remove = agents::remove_args(&found).map(|args| agents::display(d.command, &args));
                    let (hook, hook_remove): (Vec<String>, Vec<String>) = if d.id == "claude-code" {
                        (
                            agents::claude_hook::install_args(paths)
                                .iter()
                                .map(|args| agents::display(d.command, args))
                                .collect(),
                            agents::claude_hook::uninstall_args()
                                .iter()
                                .map(|args| agents::display(d.command, args))
                                .collect(),
                        )
                    } else {
                        (Vec::new(), Vec::new())
                    };
                    serde_json::json!({
                        "id": d.id,
                        "name": d.name,
                        "homepage": d.homepage,
                        "custom": false,
                        "installed": found.exe.is_some(),
                        "command": found.exe.as_ref().map(|p| p.display().to_string()),
                        "version": found.version,
                        "set_up": set_up,
                        "setup_command": agents::display(d.command, &agents::add_args(&found, engine)),
                        "hook_commands": hook,
                        "remove_command": remove,
                        "hook_remove_commands": hook_remove,
                        "remove_by_hand": agents::remove_args(&found).is_none().then(|| agents::remove_by_hand(&found)),
                    })
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap_or_default())
            .collect()
    });
    let mut all = found;
    for c in agents::custom(paths)? {
        all.push(serde_json::json!({
            "id": c.id,
            "name": c.name,
            "custom": true,
            "installed": true,
            "set_up": true,
            "mcp_command": agents::display("", &agents::server_command(&engine, &c.id)).trim().to_string(),
        }));
    }
    Ok(serde_json::Value::Array(all))
}

/// Opens a page in the user's browser: `$BROWSER` if set, else the system's
/// own opener. Failing is fine: the address was printed too.
fn open_in_browser(url: &str) {
    let mut cmd = match std::env::var("BROWSER")
        .ok()
        .filter(|b| !b.trim().is_empty())
    {
        Some(browser) => {
            let mut c = std::process::Command::new(browser);
            c.arg(url);
            c
        }
        None if cfg!(target_os = "macos") => {
            let mut c = std::process::Command::new("open");
            c.arg(url);
            c
        }
        None if cfg!(windows) => {
            // No shell between the address and the opener.
            let mut c = std::process::Command::new("rundll32");
            c.args(["url.dll,FileProtocolHandler", url]);
            c
        }
        None => {
            let mut c = std::process::Command::new("xdg-open");
            c.arg(url);
            c
        }
    };
    let _ = cmd
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
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
    let destination = m.destination.as_ref().map(|d| {
        serde_json::json!({"accepts": d.accepts, "deliver": d.deliver.as_str(), "adapter": d.adapter, "folder": d.folder, "url": d.url})
    });
    let mcp_auth = m.agent.as_ref().map(|a| match a.mcp.auth {
        openagora::manifest::McpAuth::Oauth => "oauth",
        openagora::manifest::McpAuth::None => "none",
    });
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
        "destination": destination,
        "mcp_auth": mcp_auth,
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

fn print_memory(status: &memory::Status) {
    println!(
        "Memory: {}",
        match &status.provider {
            Some(provider) => format!("Hermes's built-in memory, plus {provider}"),
            None => "Hermes's built-in memory (MEMORY.md and USER.md); for long-term memory: openagora sunday memory use holographic".to_string(),
        }
    );
    println!(
        "Digest: {}",
        if status.digest {
            format!(
                "on{}",
                status
                    .digest_last_run
                    .map(|at| format!(
                        ", last run {} (UTC){}",
                        sessions::stamp(at),
                        status
                            .digest_last_result
                            .as_deref()
                            .map(|r| format!(": {r}"))
                            .unwrap_or_default()
                    ))
                    .unwrap_or_default()
            )
        } else {
            "off (openagora sunday memory digest on)".to_string()
        }
    );
    for agent in &status.sessions {
        println!(
            "Sessions in {}: {}",
            agent.name,
            if agent.always {
                "Sunday may always read them"
            } else {
                "Sunday asks first"
            }
        );
    }
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
