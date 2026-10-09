# OpenAgora: specification

Status: draft v0.1, 2026-10-09.

## 1. What it is

One desktop app where you install open-source apps from a store and use them
side by side, with **Hermes** as the one agent that works across all of them
and remembers across all of them.

You can ask Hermes "install T3 Code", and it finds the app in the store, asks
you to confirm, installs it, and connects to it.

Three principles hold everywhere:

1. **Apps keep their own UI.** OpenAgora never rebuilds an app's interface. It
   runs the real app and shows it in a tab or its own window.
2. **Protocols, not forks.** OpenAgora talks to apps only through what they
   publish: their CLI, their web UI, and their MCP server. Upstream releases
   don't need changes here.
3. **OS-independent.** One codebase for macOS, Linux and Windows. Nothing
   depends on what the OS happens to have installed.

## 2. Architecture

```
┌──────────────────────────── OpenAgora desktop (Electron) ────────────────────────────┐
│  Sidebar of installed apps · tabs · detach to window · split view · Store · Settings │
│  Hermes chat overlay in every window                                                 │
└───────────────┬──────────────────────────────────────────────────────┬───────────────┘
                │ local API                                            │ app web UIs (tabs)
┌───────────────▼──────────────────────┐                     ┌─────────▼─────────┐
│  Engine (Rust, `openagora` daemon)   │  installs, runs,    │  T3 Code, …       │
│  catalog · install · supervise ·     │──supervises────────▶│  (their own UIs)  │
│  update · ports · logs · MCP server  │                     └─────────▲─────────┘
└───────────────▲──────────────────────┘                               │ each app's MCP
                │ store tools (MCP)                                    │
┌───────────────┴──────────────────────────────────────────────────────┴───────────────┐
│  Hermes (built-in agent): memory provider plugin · connected to every app's MCP     │
└──────────────────────────────────────────────────────────────────────────────────────┘
```

| Part | Language | Job |
|---|---|---|
| Engine | Rust | Catalog, install, run, supervise, update, uninstall. Exposes a local API for the shell and an MCP server for Hermes. |
| Shell | Electron (TypeScript) | The window: sidebar, tabs, detach, split view, store UI, permissions, notifications, Hermes overlay. |
| Hermes | (upstream) | The agent. Installed as app #0 by the engine. |
| Catalog | TOML files | One `app.toml` per app, in this repo under `catalog/apps/<id>/`. |

Why Electron for the shell: it embeds many third-party web UIs reliably on all
three OSes and can move a running web view between windows without reloading
it, which detach needs. The engine is a separate process, so the shell could
move to Tauri later without touching it.

## 3. The catalog and `app.toml`

A listing describes an app in four parts. The full schema is in
`engine/src/manifest.rs`; `openagora validate` checks a listing.

| Section | Says |
|---|---|
| top level | id, name, summary, category, homepage, source, license, platforms, role |
| `[install]` | the executable it provides, plus ordered steps per OS |
| `[update]`, `[uninstall]` | the app's own commands for these |
| `[run]` | command and args, port (`auto` or fixed), health check |
| `[ui]` | `web` (tab), `terminal` (terminal tab) or `window` (own window), plus URL |
| `[agent]` | the app's MCP URL and auth, plus the permissions Hermes asks for |

`{port}` in args and URLs is replaced with the port the engine allocates.

Rules the validator enforces:

- Unknown fields are errors, so a typo never reads as a working setting.
- Every listed platform has at least one install step.
- UI and MCP URLs point at `127.0.0.1` or `localhost`.
- The agent listing (`role = "agent"`) cannot declare uninstall.

### Catalog governance

Anyone can propose a listing by pull request. CI validates it and test-installs
it on macOS, Linux and Windows. Listings install from the app's official
source only. No repo transfers and no single gatekeeper.

## 4. Engine

### v1 commands

```
openagora catalog [query]     list or search the store
openagora show <id>           listing details, install steps for this OS, permissions
openagora validate [dirs]     check listings
openagora install <id>        run the install steps, verify `provides` is on PATH
openagora start|stop <id>     supervise the app; allocate its port; wait for health
openagora ps                  running apps, ports, health
openagora logs <id>
openagora update <id>
openagora uninstall <id>
openagora serve               the daemon: local API for the shell + MCP server for Hermes
```

`catalog`, `show` and `validate` exist today. The rest is the next milestone.

### Toolchain

Long-term, the engine brings its own pinned toolchain (`uv` for Python, Node,
git) into its own folder so apps never depend on the OS, as Pinokio does. v1
starts simpler: it runs each app's **official installer**. That is the most
update-proof route, because the app's own `update` command keeps working, but
it installs into the app's default location rather than an isolated folder.

### State

Under the OS data directory (`~/Library/Application Support/OpenAgora`,
`%APPDATA%\OpenAgora`, `~/.local/share/openagora`): installed apps and
versions, granted permissions, allocated ports, logs.

## 5. Hermes, the agent across all apps

Hermes is installed first, by the engine, and cannot be removed.

**Store tools.** The engine's MCP server gives Hermes: `store_search`,
`store_show`, `app_install`, `app_update`, `app_uninstall`, `app_start`,
`app_stop`, `app_status`, `app_open`. Install, update and uninstall always
pause for the user's confirmation in chat. The engine enforces this, so
Hermes's instructions alone are not relied on.

**Connecting to apps.** After an app installs and the user grants its
permissions, the engine adds the app's MCP server to Hermes's config
(`mcp_servers` in the Hermes profile). For T3 Code this is T3's official
outside-agent route: `http://127.0.0.1:<port>/mcp`, OAuth sign-in through T3's
own approval page, at the permission level the user picked.

**Memory.** Hermes's built-in memory holds about 2,200 characters, so
OpenAgora enables a Hermes memory provider plugin (Hindsight, Mem0 and others
are available through `hermes plugins install`). Because one agent sees every
app, its memory is the shared memory.

**Safety.**

- Text read from apps, READMEs and web pages is data, never instructions.
- Hermes gets only the permissions the user granted per app.
- Installing an app from outside the catalog needs an extra, explicit
  confirmation with a warning.
- Every agent action against an app is logged.

## 6. Permissions

Each listing's `[agent]` section declares what Hermes may do with the app.
At install the user sees each permission with its default (`granted`, `ask`
or `denied`) and can change it. They are stored by the engine and can be
revoked in Settings. Revoking removes the app from Hermes's MCP config.

## 7. Shell

- **Sidebar:** installed apps, the store, settings.
- **Tabs:** each app's own web UI. `Cmd/Ctrl+1…9` and a command palette switch
  between them.
- **Detach:** drag a tab out, or use "pop out", to move the app into its own
  window without reloading it. Closing that window returns it to its tab.
- **Split view:** two apps side by side (v1.1).
- **Layout memory:** detached windows, positions and splits are restored.
- **Hermes overlay:** a shortcut opens Hermes chat in any window. Hermes knows
  which app is in front.
- **Notifications:** one notification center for all apps.

OS features go through Electron's cross-platform APIs only: `safeStorage` for
secrets (Keychain / DPAPI / libsecret), `app.getPath` for folders,
`Notification` for notifications.

## 8. v1 scope and acceptance test

In scope: engine (install, run, supervise, MCP store tools), Hermes as app #0
with a memory provider, the T3 Code connector, the Electron shell (sidebar,
tabs, detach, Hermes overlay), and a catalog of T3 Code plus two more apps with
MCP servers. macOS first, then Linux, then Windows, all from the same code.

Acceptance test, on a fresh Mac with only OpenAgora installed:

1. Ask Hermes "install T3 Code". It shows the listing and permissions, waits
   for "yes", installs, starts it, and the T3 tab opens.
2. Ask Hermes "what threads do I have in T3?" It answers from T3 through MCP.
3. Detach T3 into its own window, quit OpenAgora, reopen it: the layout comes
   back.
4. Tell Hermes a decision in one session. After a restart, it still knows it.

## 9. Milestones

1. **Engine core:** install / start / stop / ps / logs for T3 Code and Hermes
   on macOS. (Catalog format, validation and search are done.)
2. **Store tools over MCP**, so "install T3" works from Hermes chat with
   confirmation.
3. **T3 connector:** Hermes connected to T3 through the official outside-agent
   route.
4. **Shell:** sidebar, tabs, detach, Hermes overlay.
5. **Catalog CI:** validate and test-install every listing on all three OSes.

## 10. Prior art

- **Pinokio** (MIT): one-click install and run of open-source apps. A reference
  for the engine. OpenAgora does not depend on it.
- **Ferdium:** many web apps in one window.
- **Mem0 OpenMemory, Hindsight:** memory shared across AI tools.
- **OpenRig, Paperclip:** orchestrating teams of agents.

None combines a store, one window with each app's own UI, and one agent with
memory across every app.
