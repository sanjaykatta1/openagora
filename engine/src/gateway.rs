//! The MCP gateway: the one MCP server Sunday connects to. The engine decides
//! what each client may do and asks the user itself; a client's instructions
//! are never what keeps an action safe. Every app action goes to the audit log.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use anyhow::Result;
use serde_json::{Value, json};

use crate::apps::{Engine, Status};
use crate::audit::{self, Entry};
use crate::catalog::Catalog;
use crate::jsonrpc::{Peer, RequestError};
use crate::manifest::{Arch, Manifest, Role};
use crate::mcp::{Client, Handler, ToolResult};
use crate::paths::Paths;

/// How long the user has to answer a confirmation (Hermes waits as long).
const ANSWER_WITHIN: Duration = Duration::from_secs(300);

pub struct Gateway {
    paths: Paths,
    catalog: Option<PathBuf>,
    desktop: Mutex<Option<Arc<Peer>>>,
    clients: Mutex<Vec<Weak<Client>>>,
    /// Apps with an install, update or removal in progress.
    busy: Mutex<BTreeSet<String>>,
}

/// How a confirmation went.
#[derive(Debug, PartialEq, Eq)]
pub enum Answer {
    Yes {
        via: &'static str,
    },
    No {
        via: &'static str,
    },
    /// Neither the client nor OpenAgora's window could ask the user.
    NoOneToAsk,
}

impl Gateway {
    pub fn new(paths: Paths, catalog: Option<PathBuf>) -> Gateway {
        Gateway {
            paths,
            catalog,
            desktop: Mutex::new(None),
            clients: Mutex::new(Vec::new()),
            busy: Mutex::new(BTreeSet::new()),
        }
    }

    /// OpenAgora's window connected: it can show confirmations and open apps.
    pub fn attach_desktop(&self, peer: Arc<Peer>) {
        *self.desktop.lock().unwrap() = Some(peer);
    }

    pub fn detach_desktop(&self, peer: &Arc<Peer>) {
        let mut desktop = self.desktop.lock().unwrap();
        if desktop.as_ref().is_some_and(|d| Arc::ptr_eq(d, peer)) {
            *desktop = None;
        }
    }

    fn desktop(&self) -> Option<Arc<Peer>> {
        self.desktop
            .lock()
            .unwrap()
            .clone()
            .filter(|peer| !peer.is_closed())
    }

    pub fn register(&self, client: &Arc<Client>) {
        let mut clients = self.clients.lock().unwrap();
        clients.retain(|c| c.strong_count() > 0);
        clients.push(Arc::downgrade(client));
    }

    /// Tells every connected client that its tools changed.
    pub fn tools_changed(&self) {
        for client in self
            .clients
            .lock()
            .unwrap()
            .iter()
            .filter_map(Weak::upgrade)
        {
            client.tools_changed();
        }
    }

    /// Asks the user, in the client's chat if it can ask (Hermes shows its own
    /// approval prompt wherever the conversation is), otherwise in OpenAgora's
    /// window. Only an explicit yes counts.
    pub fn confirm(&self, client: &Client, title: &str, message: &str) -> Answer {
        if client.can_elicit() {
            match client.elicit_approval(message, ANSWER_WITHIN) {
                Ok(true) => return Answer::Yes { via: "chat" },
                // A no, or no answer: never ask again somewhere else.
                Ok(false) | Err(RequestError::Timeout) => return Answer::No { via: "chat" },
                // The client couldn't ask after all: try OpenAgora's window.
                Err(RequestError::Closed | RequestError::Remote(_)) => {}
            }
        }
        if let Some(desktop) = self.desktop() {
            let asked = desktop.request(
                "confirm/ask",
                json!({"client": client.id, "title": title, "message": message}),
                ANSWER_WITHIN,
            );
            return match asked {
                Ok(reply) if reply["allowed"] == true => Answer::Yes { via: "window" },
                _ => Answer::No { via: "window" },
            };
        }
        Answer::NoOneToAsk
    }

    fn catalog(&self) -> Result<Catalog> {
        match &self.catalog {
            Some(dir) => Catalog::load(dir),
            None => Catalog::embedded(),
        }
    }

    fn audit(&self, entry: &Entry) {
        if let Err(e) = audit::record(&self.paths, entry) {
            eprintln!("openagora: writing the audit log: {e:#}");
        }
    }
}

fn tool(
    name: &str,
    title: &str,
    description: &str,
    properties: Value,
    required: &[&str],
    read_only: bool,
) -> Value {
    json!({
        "name": name,
        "title": title,
        "description": description,
        "inputSchema": {"type": "object", "properties": properties, "required": required},
        "annotations": {"title": title, "readOnlyHint": read_only, "openWorldHint": false},
    })
}

fn app_id_property() -> Value {
    json!({"id": {"type": "string", "description": "The app's id in the store, e.g. \"t3code\" (see store_search)."}})
}

/// The store tools from SPEC §5.
fn store_tools() -> Vec<Value> {
    let mut install = tool(
        "app_install",
        "Install an app",
        "Install an app from the OpenAgora store, from its official source. OpenAgora asks the user to confirm first.",
        app_id_property(),
        &["id"],
        false,
    );
    install["annotations"]["openWorldHint"] = json!(true);
    let mut uninstall = tool(
        "app_uninstall",
        "Remove an app",
        "Stop and remove an installed app. OpenAgora asks the user to confirm first.",
        app_id_property(),
        &["id"],
        false,
    );
    uninstall["annotations"]["destructiveHint"] = json!(true);
    vec![
        tool(
            "store_search",
            "Search the store",
            "Search the OpenAgora store of open-source apps. Without a query, lists every app. Says which are installed and running.",
            json!({"query": {"type": "string", "description": "Words to look for in an app's name, summary or category."}}),
            &[],
            true,
        ),
        tool(
            "store_show",
            "Show a store listing",
            "Details of one app in the store: what it is, its source and license, what installing it runs on this computer, and its status.",
            app_id_property(),
            &["id"],
            true,
        ),
        install,
        tool(
            "app_update",
            "Update an app",
            "Update an installed app with its own update command. OpenAgora asks the user to confirm first.",
            app_id_property(),
            &["id"],
            false,
        ),
        uninstall,
        tool(
            "app_start",
            "Start an app",
            "Start an installed app in the background and say where it runs.",
            app_id_property(),
            &["id"],
            false,
        ),
        tool(
            "app_stop",
            "Stop an app",
            "Stop a running app.",
            app_id_property(),
            &["id"],
            false,
        ),
        tool(
            "app_status",
            "App status",
            "Whether installed apps are running, and where. With an id, just that app.",
            json!({"id": {"type": "string", "description": "An app id; leave out for every installed app."}}),
            &[],
            true,
        ),
        tool(
            "app_open",
            "Open an app",
            "Show a running app in the OpenAgora window, in its own tab.",
            app_id_property(),
            &["id"],
            false,
        ),
    ]
}

impl Handler for Gateway {
    fn tools(&self, client: &Client) -> Vec<Value> {
        match client.id.as_str() {
            // Sunday manages the store; coding agents get their own tools later.
            "sunday" => store_tools(),
            _ => Vec::new(),
        }
    }

    fn call(&self, client: &Client, name: &str, arguments: &Value) -> ToolResult {
        let result = (|| -> Result<ToolResult> {
            let catalog = self.catalog()?;
            let engine = Engine::new()?;
            if name == "store_search" {
                return Ok(search(
                    &engine,
                    &catalog,
                    arguments["query"].as_str().unwrap_or(""),
                ));
            }
            if name == "app_status" && arguments["id"].as_str().is_none() {
                return Ok(status_all(&engine, &catalog));
            }
            let id = arguments["id"].as_str().unwrap_or_default();
            let Some(m) = catalog.listings.get(id).map(|l| &l.manifest) else {
                return Ok(ToolResult::error(format!(
                    "There is no app {id:?} in the OpenAgora store. Use store_search to find its id."
                )));
            };
            Ok(match name {
                "store_show" => show(&engine, m),
                "app_status" => status_one(&engine, m),
                "app_install" | "app_update" | "app_uninstall" => {
                    self.change(client, &engine, m, name)
                }
                "app_start" => self.start(client, &engine, m),
                "app_stop" => self.stop(client, &engine, m),
                "app_open" => self.open(client, &engine, m),
                _ => ToolResult::error(format!("unknown tool {name}")),
            })
        })();
        result.unwrap_or_else(|e| ToolResult::error(format!("{e:#}")))
    }
}

fn describe_status(engine: &Engine, m: &Manifest) -> String {
    match engine.status(&m.id) {
        Ok(Status::Running(r)) => match r.url {
            Some(url) => format!("running at {url}"),
            None => "running in the background".into(),
        },
        _ if engine.installed(&m.id).ok().flatten().is_some() => "installed, stopped".into(),
        _ => "not installed".into(),
    }
}

fn search(engine: &Engine, catalog: &Catalog, query: &str) -> ToolResult {
    let listings: Vec<&Manifest> = if query.trim().is_empty() {
        catalog.listings.values().map(|l| &l.manifest).collect()
    } else {
        catalog
            .search(query)
            .into_iter()
            .map(|l| &l.manifest)
            .collect()
    };
    if listings.is_empty() {
        return ToolResult::text(format!("No app in the OpenAgora store matches {query:?}."));
    }
    let lines: Vec<String> = listings
        .iter()
        .map(|m| {
            let here = if m.platforms.contains(&engine.os) {
                describe_status(engine, m)
            } else {
                format!("not available on {}", engine.os)
            };
            format!(
                "- {} ({}), {}: {} [{here}]",
                m.name, m.id, m.category, m.summary
            )
        })
        .collect();
    ToolResult::text(format!("OpenAgora store:\n{}", lines.join("\n")))
}

fn install_plan(engine: &Engine, m: &Manifest) -> String {
    let mut lines = Vec::new();
    for step in m.steps_for(engine.os) {
        if let Some(d) = &step.download {
            let asset = Arch::current()
                .and_then(|a| d.asset.for_arch(a))
                .unwrap_or("(no build for this CPU)");
            lines.push(format!(
                "  download {asset} from the latest release of github.com/{}, checking its checksum",
                d.github
            ));
        }
        lines.push(format!("  run: {}", step.run));
    }
    lines.join("\n")
}

fn show(engine: &Engine, m: &Manifest) -> ToolResult {
    let mut text = format!(
        "{} ({})\n{}\nCategory: {}\nHomepage: {}\nSource: {}\nLicense: {}\nStatus: {}\n",
        m.name,
        m.id,
        m.summary,
        m.category,
        m.homepage,
        m.source,
        m.license,
        describe_status(engine, m)
    );
    if m.platforms.contains(&engine.os) {
        text.push_str(&format!(
            "Installing it on this computer:\n{}\n",
            install_plan(engine, m)
        ));
    } else {
        text.push_str(&format!("Not available on {}.\n", engine.os));
    }
    if let Some(agent) = &m.agent {
        text.push_str("What OpenAgora's agent may do with it:\n");
        for p in &agent.permissions {
            text.push_str(&format!("  [{:?}] {}\n", p.default, p.description));
        }
    }
    ToolResult::text(text)
}

fn status_one(engine: &Engine, m: &Manifest) -> ToolResult {
    ToolResult::text(format!(
        "{} ({}): {}",
        m.name,
        m.id,
        describe_status(engine, m)
    ))
}

fn status_all(engine: &Engine, catalog: &Catalog) -> ToolResult {
    let lines: Vec<String> = catalog
        .listings
        .values()
        .map(|l| &l.manifest)
        .filter(|m| engine.installed(&m.id).ok().flatten().is_some())
        .map(|m| format!("- {} ({}): {}", m.name, m.id, describe_status(engine, m)))
        .collect();
    if lines.is_empty() {
        ToolResult::text("No apps are installed yet. Use store_search to find one.")
    } else {
        ToolResult::text(format!("Installed apps:\n{}", lines.join("\n")))
    }
}

impl Gateway {
    /// Install, update or remove: always confirmed by the user first.
    fn change(&self, client: &Client, engine: &Engine, m: &Manifest, action: &str) -> ToolResult {
        let mut entry = Entry::new(&client.id, action, &m.id);
        let installed = engine.installed(&m.id).ok().flatten().is_some();
        let refusal = match action {
            _ if m.role == Role::Agent && action != "app_install" => Some(
                "Sunday can't update or remove itself. The user can do that in OpenAgora's Settings."
                    .to_string(),
            ),
            "app_install" if installed => Some(format!("{} is already installed.", m.name)),
            "app_install" if !m.platforms.contains(&engine.os) => {
                Some(format!("{} is not available on {}.", m.name, engine.os))
            }
            "app_update" | "app_uninstall" if !installed => {
                Some(format!("{} is not installed.", m.name))
            }
            "app_update" if m.update.is_none() => {
                Some(format!("{} has no update command.", m.name))
            }
            _ => None,
        };
        if let Some(refusal) = refusal {
            entry.decision = "refused".into();
            entry.outcome = refusal.clone();
            self.audit(&entry);
            return ToolResult::error(refusal);
        }
        if !self.busy.lock().unwrap().insert(m.id.clone()) {
            return ToolResult::error(format!(
                "{} is already being installed, updated or removed. Wait for that to finish.",
                m.name
            ));
        }
        let result = self.change_confirmed(client, engine, m, action, &mut entry);
        self.busy.lock().unwrap().remove(&m.id);
        self.audit(&entry);
        result
    }

    fn change_confirmed(
        &self,
        client: &Client,
        engine: &Engine,
        m: &Manifest,
        action: &str,
        entry: &mut Entry,
    ) -> ToolResult {
        let (verb, done_verb, title, question) = match action {
            "app_install" => (
                "install",
                "installed",
                format!("Install {}?", m.name),
                format!(
                    "Install {}? {}\nFrom {} ({}). On this computer OpenAgora will:\n{}\nOpen-source apps run with your permissions. Only install apps you trust.",
                    m.name,
                    m.summary,
                    m.source,
                    m.license,
                    install_plan(engine, m)
                ),
            ),
            "app_update" => (
                "update",
                "updated",
                format!("Update {}?", m.name),
                format!(
                    "Update {} by running its own update command: {}",
                    m.name,
                    m.update
                        .as_ref()
                        .map(|u| u.run.as_str())
                        .unwrap_or_default()
                ),
            ),
            _ => (
                "remove",
                "removed",
                format!("Remove {}?", m.name),
                format!("Stop {} and remove it from this computer?", m.name),
            ),
        };
        match self.confirm(client, &title, &question) {
            Answer::Yes { via } => {
                entry.decision = "confirmed".into();
                entry.via = Some(via.into());
            }
            Answer::No { via } => {
                entry.decision = "declined".into();
                entry.via = Some(via.into());
                entry.outcome = "not done".into();
                return ToolResult::text(format!(
                    "The user did not confirm, so {} was not {done_verb}. Don't try again unless the user asks.",
                    m.name
                ));
            }
            Answer::NoOneToAsk => {
                entry.decision = "refused".into();
                entry.outcome = "no way to ask the user".into();
                return ToolResult::error(format!(
                    "OpenAgora needs the user to confirm this, and couldn't ask: open OpenAgora on this computer and try again, or use OpenAgora's Store to {verb} {}.",
                    m.name
                ));
            }
        }
        let done = match action {
            "app_install" => engine
                .install(m)
                .map(|_| format!("Installed {}. Start it with app_start.", m.name)),
            "app_update" => engine.update(m).map(|_| format!("Updated {}.", m.name)),
            _ => engine.uninstall(m).map(|note| {
                format!(
                    "Removed {}.{}",
                    m.name,
                    note.map(|n| format!(" {n}")).unwrap_or_default()
                )
            }),
        };
        match done {
            Ok(text) => ToolResult::text(text),
            Err(e) => {
                entry.outcome = format!("failed: {e:#}");
                ToolResult::error(format!("{e:#}"))
            }
        }
    }

    fn start(&self, client: &Client, engine: &Engine, m: &Manifest) -> ToolResult {
        let mut entry = Entry::new(&client.id, "app_start", &m.id);
        let result = match engine.start(m) {
            Ok(running) => ToolResult::text(match running.url {
                Some(url) => format!(
                    "{} is running at {url}. Show it to the user with app_open.",
                    m.name
                ),
                None => format!("{} is running in the background.", m.name),
            }),
            Err(e) => {
                entry.outcome = format!("failed: {e:#}");
                ToolResult::error(format!("{e:#}"))
            }
        };
        self.audit(&entry);
        result
    }

    fn stop(&self, client: &Client, engine: &Engine, m: &Manifest) -> ToolResult {
        let mut entry = Entry::new(&client.id, "app_stop", &m.id);
        if m.role == Role::Agent {
            entry.decision = "refused".into();
            entry.outcome = "the agent can't stop itself".into();
            self.audit(&entry);
            return ToolResult::error("Sunday can't stop itself.");
        }
        let result = match engine.stop(m) {
            Ok(true) => ToolResult::text(format!("Stopped {}.", m.name)),
            Ok(false) => ToolResult::text(format!("{} was not running.", m.name)),
            Err(e) => {
                entry.outcome = format!("failed: {e:#}");
                ToolResult::error(format!("{e:#}"))
            }
        };
        self.audit(&entry);
        result
    }

    fn open(&self, client: &Client, engine: &Engine, m: &Manifest) -> ToolResult {
        let mut entry = Entry::new(&client.id, "app_open", &m.id);
        let result = match engine.status(&m.id) {
            Ok(Status::Running(running)) => match (self.desktop(), running.url) {
                (Some(desktop), _) => {
                    let _ = desktop.notify("app/open", json!({"id": m.id}));
                    ToolResult::text(format!("Opened {} in the OpenAgora window.", m.name))
                }
                (None, Some(url)) => ToolResult::text(format!(
                    "OpenAgora's window isn't open, so {} couldn't be shown there. It runs at {url}.",
                    m.name
                )),
                (None, None) => ToolResult::text(format!("{} runs in the background.", m.name)),
            },
            _ => {
                entry.outcome = "not running".into();
                ToolResult::error(format!(
                    "{} isn't running. Start it with app_start first.",
                    m.name
                ))
            }
        };
        self.audit(&entry);
        result
    }
}
