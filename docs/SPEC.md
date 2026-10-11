# OpenAgora: specification

Status: draft v0.3, 2026-10-11. The personal agent is **Sunday**, built from
Hermes ([issue #9](https://github.com/sanjaykatta1/openagora/issues/9)). Sections
mark what is **built** and what is **planned**, by the phase of issue #9 that
delivers it. User-facing detail is in [Sunday](personal-agents.md) and
[the command line](command-line.md).

## 1. What it is

One desktop app where you install open-source apps from a store and use them
side by side, with **Sunday**, a personal agent that works across all of them
and remembers across all of them, and carries what you are working on to the
coding agent you use.

Sunday *is* Hermes: its dashboard, setup, settings, channels, memory, profiles,
hooks and skills. OpenAgora changes only its identity and how it is presented. It
runs in a Hermes profile of its own (`sunday`), so a Hermes the user already has
is never changed.

Principles that hold everywhere:

1. **Don't reinvent the wheel.** Apps keep their own UI; Sunday keeps Hermes's.
   OpenAgora never rebuilds an app's or the agent's interface.
2. **Protocols, not forks.** OpenAgora talks to apps only through what they
   publish: their CLI, their web UI, and their MCP server.
3. **Generic, not built for one app or agent.** Every app is a *source*, every
   coding agent a *destination*, each reached through an adapter behind one
   interface. T3 Code is the first adapter, not the design target. (Planned:
   phases 2–4.)
4. **The engine is the go-between.** Sunday never connects to apps directly. Its
   only MCP server is the engine's gateway, which enforces permissions and logs
   every action. (Built: phase 2.)
5. **Private by default.** Capture only when asked, per-app permission, a
   never-capture list, everything stored locally, background mode opt-in.
   (Built: phase 3. Background mode: phase 6.)
6. **OS-independent.** One codebase for macOS, Linux and Windows.

## 2. Architecture

```
 Any app on the computer            OpenAgora windows
 (installed by OpenAgora or not)    (tabs + popped-out windows)
          │ OS window capture,             │ focus + tab screenshots
          ▼                                ▼
 ┌──────────────── OpenAgora engine (Rust) ─────────────────────────────────┐
 │  Catalog · install · run · supervise · update · audit log (built)        │
 │  MCP gateway: one server, many clients (built: store and desk tools)    │
 │  Desk · Focus tracker · Broker (built) · Registry · Router (planned)    │
 └────────┬───────────────────────────────┬────────────────────────────────┘
          │ MCP + per-reply hook          │ MCP (pull) / adapters (push)
          ▼                               ▼
   Sunday (Hermes profile "sunday")   Coding agents: T3 Code, Claude Code,
   dashboard · memory · channels      Codex, OpenCode, …
```

| Part | Language | Job |
|---|---|---|
| Engine | Rust | Catalog, install, run, supervise, update, uninstall; Sunday's profile; the daemon with the MCP gateway, the desk, the broker, the focus query and the audit log. |
| Shell | Electron (TypeScript) | The window: Sunday's home screen, sidebar, tabs, store, settings; Sunday's shortcut and panel; the menu bar or tray icon; capturing windows when the engine asks. |
| Sunday | Hermes (upstream) | The agent, under its own profile. Installed by the engine from the store, or adopted when already installed. |
| Catalog | TOML files | One `app.toml` per app, in this repo under `catalog/apps/<id>/`. |

Why Electron for the shell: it embeds many third-party web UIs reliably on all
three OSes and can move a running web view between windows without reloading
it. The engine is a separate process, so the shell could move to Tauri later
without touching it.

## 3. The catalog and `app.toml`

A listing describes an app in these parts. The full schema is in
`engine/src/manifest.rs`; `openagora validate` checks a listing.

| Section | Says |
|---|---|
| top level | id, name, summary, category, homepage, source, license, platforms, role, and for the agent its `profile` |
| `[install]` | what it provides (an executable on PATH or a path under `{app_dir}`), plus ordered steps per OS; a step can first download a file from the app's latest GitHub release, picked per CPU |
| `[update]`, `[uninstall]` | the app's own commands for these |
| `[run]` | command (same everywhere or per OS) and args, port (`auto`, fixed, or none), health check (`tcp` or `process`) |
| `[ui]` | `web` (tab), `terminal`, `window` or `background`, plus URL for `web`, and optionally `url_from_log_after` for apps that print a one-time sign-in link (only 127.0.0.1/localhost addresses are accepted) |
| `[agent]` | the app's MCP URL and auth, plus the permissions Sunday asks for |

Placeholders the engine fills in: `{app_dir}` (the app's own folder under
OpenAgora's data directory), `{download}` (the file a step downloaded), `{port}`
(the allocated port) and, for the agent only, `{profile}`. Unknown placeholders
are errors.

Rules the validator enforces:

- Unknown fields are errors, so a typo never reads as a working setting.
- Every listed platform has at least one install step.
- UI and MCP URLs point at `127.0.0.1` or `localhost`.
- Downloads come only from the repository named in the listing's `source`.
- `provides` and `run.command` cover every listed platform.
- A catalog has at most one `role = "agent"` listing. It must name a `profile`
  and use `{profile}` in `run`, so the agent never runs outside its own profile.
  Only the agent has a profile.
- The agent cannot declare `[uninstall]`: removing it deletes only its profile.

### Catalog governance

Anyone can propose a listing by pull request. CI validates it and test-installs
it on macOS, Linux and Windows. Listings install from the app's official
source only. No repo transfers and no single gatekeeper.

## 4. Engine

### Commands (built)

```
openagora catalog [query]     list or search the store (--json for the desktop)
openagora show <id>           listing details, install steps for this OS, permissions
openagora validate [dirs]     check listings
openagora install <id>        run the install steps, verify `provides`
openagora adopt <id>          record an app already on this computer, without installing it
openagora update <id>         run the app's own update command
openagora start|stop <id>     supervise the app; allocate its port; wait for health
openagora ps                  running apps, ports, health
openagora logs <id>
openagora uninstall <id>
openagora sunday status       Hermes, Sunday's profile, its dashboard and gateway (--json)
openagora sunday setup        create Sunday's profile and SOUL.md
openagora sunday connect      let Sunday use OpenAgora (gateway, hook, skills), after asking
openagora serve               the daemon: the MCP gateway, over a local socket
openagora daemon start|stop|status
openagora mcp --client <id>   an MCP client's stdio, relayed to the daemon
openagora context --hook      Sunday's per-reply Hermes hook
openagora audit               the audit log
openagora focus               the windows in front, and whether OpenAgora may capture them
openagora desk                the desk: list, show, add, remove, clear
openagora grants              the apps Sunday may see: list, allow, revoke
openagora never               the never-capture list: list, add, remove
```

Details as built:

- `install` shows the listing and its steps and asks first (`--yes` skips the
  question; without a terminal it refuses unless `--yes` is given). Downloads
  come from the app's latest GitHub release and are checked against the SHA-256
  digest GitHub publishes. Any failure removes the app's folder, so a failed
  install leaves nothing behind. The agent is never reinstalled over an existing
  install: if it is already on the computer, `install` adopts it.
- `adopt` records an app whose `provides` is already present. Uninstalling an
  adopted app only forgets it; its own uninstaller is not run.
- `update` runs the listing's `[update]` command (for Hermes, `hermes update`),
  after asking.
- `start` runs the app detached, in its own process group, with output going
  to its log. Web apps count as started once their port accepts connections
  (90 s limit); background apps once they have stayed up for 2 s. If the app
  exits during startup, `start` fails and prints the end of its log. The agent
  starts only once its profile exists.
- `stop` sends SIGTERM to the app's process group (Windows: `taskkill /T`),
  then forces it after 10 s.
- The apps' own commands (install steps, update, uninstall) run with
  `~/.local/bin` added to PATH, where official installers put their commands.
- `uninstall` of the agent removes Sunday: it deletes only the agent's own
  profile with the agent's own command (`hermes profile delete sunday --yes`),
  and only if OpenAgora created that profile.

### Daemon and MCP gateway (built: phase 2)

- `openagora serve` is a long-running service. The desktop app starts it
  (`openagora daemon start`); so does `openagora mcp` when it isn't running, so
  Sunday reaches it from a messaging channel with OpenAgora's window closed. It
  keeps running after the window closes.
- It listens on a local socket: `<data>/run/engine.sock`, in a folder only this
  user can open (a private folder under `/tmp` when that path is too long for a
  Unix socket), or a named pipe on Windows. Every connection opens with a hello
  naming its role: `mcp` (an MCP client, with its client id), or `desktop`
  (OpenAgora's window).
- `openagora mcp --client <id>` is the stdio MCP entry point every MCP client
  uses. It is a plain relay to the daemon, so there is no network port or token
  to manage. Clients are trusted to name themselves; anything that can reach the
  socket already runs as this user.
- The gateway speaks MCP 2025-06-18 (also 2025-03-26 and 2024-11-05). It offers
  tools, sends `notifications/tools/list_changed`, and asks the user through
  `elicitation/create`. Tool results carry text content only (no
  `structuredContent`, which some clients use instead of images).
- **Store tools for Sunday:** `store_search`, `store_show`, `app_install`,
  `app_update`, `app_uninstall`, `app_start`, `app_stop`, `app_status`,
  `app_open`. Other clients get no store tools; coding agents get their own tools
  in phase 4.
- **Confirmation is the engine's.** Install, update and uninstall always wait for
  an explicit yes, which the engine asks for itself:
  - first through the client's own chat, by MCP elicitation, when the client
    supports it (Hermes shows its approval prompt wherever the conversation is);
  - otherwise in OpenAgora's window (`confirm/ask` to the desktop);
  - otherwise the action is refused.
  A decline, a cancel or no answer within 300 s is a no. The agent can't approve
  for the user, and the agent listing can't be updated, stopped or removed through
  the gateway.
- `app_open` asks the window to show the app in its tab (`app/open`).
- **Audit log:** every app action is appended to `<data>/audit/audit.jsonl` with
  the client, action, target, decision (`allowed`, `confirmed`, `declined`,
  `refused`), where the user answered, and the outcome.
- **Per-reply hook:** `openagora context --hook --client sunday` is Sunday's Hermes
  `pre_llm_call` shell hook. It asks the running engine (role `hook`) and prints
  `{"context": …}` marked as data, with only what changed since that session's
  previous turn, and nothing at all when nothing changed: installed apps and their
  state; and, in chats on this computer (Hermes platforms `cli`, `tui`,
  `desktop`, `local`, `acp`), the app the user is working in, their last press
  of Sunday's shortcut and what it captured, and new desk items. Messaging
  channels hear about apps only. Without the engine running, it reports apps
  from disk.
- **Sunday's connection:** `openagora sunday connect` uses
  `hermes -p sunday config set` to add `mcp_servers.openagora`, the hook,
  OpenAgora's skills folder (`skills.external_dirs`) and `hooks_auto_accept: true`
  to Sunday's profile only, after asking. It points them at a copy of the engine
  at `<data>/bin/openagora`, kept current, because the desktop app's own copy can
  move. `disconnect` removes only those.
- **Skills:** `sunday/skills/` in this repo, written to `<data>/sunday/skills/`
  on connect and whenever the engine service starts: `openagora-apps` (the store
  tools) and `openagora-desk` (capture and the desk). Each is shown only when its
  tools are present (`metadata.hermes.requires_tools`).
- **Inherited descriptors:** the engine marks every file descriptor it inherited
  (beyond stdin, stdout and stderr) close-on-exec at startup, so the engine
  service and the apps it starts don't hold open files, pipes or sockets of
  OpenAgora's window.

### Desk, capture, focus and the broker (built: phase 3)

OpenAgora's window does the capturing, because only it may ask the OS for a
window's picture and it captures its own tabs without any OS permission. The
engine decides what may be captured, keeps the result, and logs it.

- **Focus query** (`engine/src/focus.rs`): the windows on screen, front to back,
  asked of the OS only when needed (the shortcut, a capture, each Sunday reply);
  nothing is tracked in the background. macOS: the on-screen window list
  (CGWindowList through a JavaScript for Automation script run by `osascript`),
  with bundle ids from NSRunningApplication; titles need Screen Recording
  permission. Windows: the foreground window, then the other top-level windows in
  z-order (Windows API), with each executable's own description as its name. X11:
  the window manager's `_NET_ACTIVE_WINDOW` and `_NET_CLIENT_LIST_STACKING`, with
  names from desktop entries. Wayland lets no app see other apps' windows, so
  there the user picks the window in the system's own dialog, and that choice is
  the permission for that one capture. Each window carries an app key (bundle id,
  executable file name, or window class), which grants and the never-capture list
  use. `OPENAGORA_FOCUS_FILE` replaces the OS with a file in tests.
- **The window the user is in** is the front-most window that isn't OpenAgora's
  own: the window reports its process id, the OS ids of its main window and the
  Sunday panel (`getMediaSourceId`, the same ids the OS query returns), and which
  app its tab shows (`focus/openagora`). An app in an OpenAgora tab counts as that
  app; the panel and OpenAgora's own pages are looked behind.
- **Broker** (`engine/src/broker.rs`): grants keyed by client × source app ×
  destination × type family (`image/*` for now), allowed once or always, in
  `<data>/broker/grants.toml`; the never-capture list, built in
  (`engine/src/never_capture.toml`: password managers by name, bundle id,
  executable or window class; private browser windows and Tor Browser by window
  title) plus the user's additions in `<data>/broker/never.toml`. The never list
  wins over every grant. Every capture asks the broker first.
- **Desk** (`engine/src/desk.rs`): items with an id, kind (screenshot, image,
  text, link, file), MIME type and type family, source (app, key, title), created
  and expiry times (24 hours), `shared_with` (the clients that may read it) and
  who added it. Payloads are stored once, by SHA-256, under `<data>/desk/`, a
  folder only this user can open. Pictures must be PNG, JPEG, GIF or WebP; 20 MB
  at most.
- **Shortcut** (window → engine): `shortcut/pressed` finds the window the user is
  in and returns the broker's verdict before the panel opens over it;
  `shortcut/capture` (optionally with `allow: once|always` from the panel's
  buttons, or `pick: true` where windows are hidden) captures it. The engine
  asks the window with `capture/window` (a window by its OS id, an app tab, or
  a pick) and puts the picture on the desk for Sunday.
- **Desk from the window:** `desk/add` for the clipboard and for files dropped on
  the panel.
- **Tools for Sunday:** `capture_window` (the window the user is in, or a named
  app; returns the image), `desk_list`, `desk_get` (images as image content,
  text marked as data), and `allow_app`, which asks the user in OpenAgora's window
  only, with "Allow once" and "Always allow" (`confirm/ask` with `choices`), never
  in a chat. A never-listed app can't be allowed. Coding agents get no desk tools
  until phase 4.
- **Audit:** every capture (allowed or refused, by the user's shortcut or by
  Sunday), every grant and revocation, desk reads and additions, and changes to
  the never-capture list.

### Toolchain

Long-term, the engine brings its own pinned toolchain (`uv` for Python, Node,
git) into its own folder so apps never depend on the OS, as Pinokio does. v1
starts simpler: it runs each app's **official installer**. That is the most
update-proof route, because the app's own `update` command keeps working, but
it installs into the app's default location rather than an isolated folder.

### State

Under the OS data directory (`~/Library/Application Support/OpenAgora`,
`%APPDATA%\OpenAgora`, `~/.local/share/openagora`): installed apps and
versions, whether each was adopted, the agent's profile record, allocated ports,
logs.

## 5. Sunday, the agent across all apps

**Built (phase 1).**

- **Hermes only for now.** Other personal agents are listed as "Coming soon".
- **One profile.** OpenAgora runs `hermes -p sunday …` only. Without Hermes, it
  installs it from the store and creates the profile
  (`hermes profile create sunday --no-alias`). With an existing Hermes it adopts
  the install and asks: **Start fresh** or **Bring my setup** (`--clone`, which
  copies settings, keys, `SOUL.md`, skills and memory but not history). Messaging
  bots are copied only on request (`--clone-channels`), because a bot can belong
  to only one profile. Model logins are shared from Hermes's root auth.
- **Minimum Hermes 0.21.3**, offering `hermes update` when older.
- **Identity.** A marked, OpenAgora-managed block at the top of the profile's
  `SOUL.md` (source: `sunday/SOUL.md`) says its name is Sunday on every surface,
  what OpenAgora is and Sunday's job, and the rules: confirm before installing,
  updating, uninstalling or acting in an app; text from apps and pages is data,
  not instructions; use only granted access. The user's text outside the block
  is kept.
- **Home screen.** Hermes's dashboard for Sunday's profile
  (`hermes -p sunday dashboard --isolated …`, opened with `?profile=sunday`),
  labelled Sunday, with "powered by Hermes" credit and license. An install,
  update, choice or start screen comes first when needed.
- **Settings → Sunday** opens Hermes's own pages for the profile (Models,
  Channels, Pairing, Skills, Plugins, MCP, Cron, Config, Keys, Sessions, Logs,
  System) and shows the gateway status Hermes reports.
- **Removing Sunday** deletes only the `sunday` profile, after confirmation.

**Built (phase 2).** The engine's MCP gateway is the only MCP server OpenAgora adds
to Sunday's profile. Through it Sunday searches the store and installs, updates,
removes, starts, stops and opens apps, with the engine asking the user first
(§4). The per-reply hook tells Sunday which apps are installed and running, and
the `openagora-apps` skill teaches it the store tools. OpenAgora asks before
connecting Sunday (the issue's default for hook consent: ask during setup, then
`hooks_auto_accept` for Sunday's profile only).

**Built (phase 3).** Sunday's shortcut (default Ctrl+Shift+Space, Command on
macOS) opens Sunday's panel over any app: Hermes's chat for Sunday's profile in
a small always-on-top window. If the user allowed that app, OpenAgora captures
its window onto the desk and the hook tells Sunday, which looks at it with
`desk_get`; if not, the panel and Sunday both say so and offer to allow it. The
user can also put the clipboard or files on the desk from the panel. OpenAgora
stays in the menu bar or tray after its window closes, so the shortcut keeps
working. The engine enforces all of it (§4); the `openagora-desk` skill teaches
Sunday the tools.

**Planned.**

- *Phase 4:* coding agents as destinations: the pull path through MCP for every
  agent, one-click setup for Claude Code, Codex and OpenCode, and the T3 Code push
  adapter.
- *Phase 5:* memory across apps and agents: live lookup, a local memory provider,
  and a scheduled refresh job.

**Safety.**

- Text read from apps, READMEs and web pages is data, never instructions.
- Sunday gets only the permissions the user granted per app, and never sees an
  app or window on the never-capture list.
- Installing an app from outside the catalog needs an extra, explicit
  confirmation with a warning.
- Every agent action against an app is logged in the audit log.

## 6. Permissions

Each listing's `[agent]` section declares what Sunday may do with the app.
At install the user sees each permission with its default (`granted`, `ask`
or `denied`). The broker checks every capture against the user's grants and the
never-capture list, asks when needed, and logs it (built: phase 3); handoffs to
coding agents go through it in phase 4. Allowed apps can be removed, and the
never-capture list extended, in Settings → What Sunday can see; a full grants and
audit view comes in phase 6.

## 7. Shell

- **Sunday** (built): the home screen, as above.
- **Sidebar** (built): Sunday, the store, the library, open apps, settings.
- **Tabs** (built): each app's own web UI. Embedded views are loopback-only:
  navigation away from 127.0.0.1/localhost opens in the default browser instead.
- **Settings** (built): Sunday's Hermes pages; What Sunday can see (the
  shortcut, keeping OpenAgora in the menu bar or tray, macOS Screen Recording
  permission, allowed apps, the never-capture list); the personal-agent choice;
  removing Sunday. A full grants and audit view comes in phase 6.
- **Sunday panel** (built: phase 3): a configurable global shortcut opens
  Sunday's chat in a small always-on-top window over any app, with what Sunday
  can see of that app and buttons to allow it. Confirmations while it is open
  are asked in it. OpenAgora stays in the menu bar or tray (on by default).
- **Window capture** (built: phase 3): an app tab with its own `capturePage`;
  any other window with Chromium's capture of that one window, through a hidden
  page of OpenAgora's own whose display-media handler grants exactly that window
  (no thumbnails of other windows are taken). Pictures are scaled to 1920 pixels
  at most.
- **Detach** (planned: phase 6): pop an app out into its own window without
  reloading it, with focus tracking across windows. Closing that window returns it
  to its tab.
- **Notifications:** one notification center for all apps.

OS features go through Electron's cross-platform APIs only: `safeStorage` for
secrets (Keychain / DPAPI / libsecret), `app.getPath` for folders,
`Notification` for notifications. Electron's security model stays: context
isolation, sandbox, allowlisted IPC, local-only web views.

## 8. Phases and acceptance tests

Issue #9 builds Sunday in six phases; each ends with the app working, tests
passing and docs updated. Its end-to-end acceptance tests (numbered as in the
issue) are covered by automated tests with fakes where possible; parts that need
real accounts are checked by hand.

| Phase | Scope | Done when | Status |
|---|---|---|---|
| 1. Sunday basics | Hermes only; the `sunday` profile; identity; home screen; settings | Tests 1–4 and 14 | Built |
| 2. Engine daemon + gateway | daemon, MCP gateway with store tools, per-reply hook, first skills | Test 5 | Built |
| 3. Capture | desk, capture, focus tracker, shortcut and panel, broker basics | Tests 6, 8, 10 | Built |
| 4. Coding agents | registry, destinations, pull path, T3 Code push adapter | Test 7 | Planned |
| 5. Memory | live lookup, memory provider, refresh job | Tests 11, 12 | Planned |
| 6. Polish | grants and audit UI, background mode, on-screen text, OpenCode push, pop-out windows | Tests 9, 13 | Planned |

## 9. Prior art

- **Pinokio** (MIT): one-click install and run of open-source apps. A reference
  for the engine. OpenAgora does not depend on it.
- **Ferdium:** many web apps in one window.
- **Mem0 OpenMemory, Hindsight:** memory shared across AI tools.
- **OpenRig, Paperclip:** orchestrating teams of agents.
- From issue #9: the Plan 9 plumber (routing outside the apps), Android intents
  and PWA `share_target` (apps declare what they accept), Apple App Intents and
  Transferable (type families), the Windows on-device agent registry (a broker with
  consent and audit), xdg portals (the user's choice in a trusted dialog is the
  permission), Microsoft UFO (API first, UI as fallback), Raycast AI (`@app`
  narrows the tools), and screenpipe (ideas only, for opt-in screen history).

None combines a store, one window with each app's own UI, and one agent with
memory across every app and coding agent.
