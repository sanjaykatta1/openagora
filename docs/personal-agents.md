# Sunday, OpenAgora's personal agent

Sunday is OpenAgora's personal agent. It remembers across your apps, can see the
app you're working in when you call it with its shortcut, and will carry your
work to the coding agents you use. Underneath, Sunday is
[Hermes Agent](https://hermes-agent.nousresearch.com/docs/) by
[Nous Research](https://nousresearch.com) (MIT license): OpenAgora does not
implement its own agent, chat screen, setup or settings. It shows Hermes's own
dashboard and pages, labelled Sunday.

**Credit where it's due.** OpenAgora says Sunday is Hermes Agent by Nous Research
wherever Sunday appears: on the home screen, in the dashboard's bar, in Sunday's
panel, in Settings, in the agent chooser and in OpenAgora's About box, each with
a link to Hermes's source and its MIT license. Sunday says so itself when it
describes who it is or what it runs on.

Other personal agents (OpenClaw, Goose, nanobot, NanoClaw, PicoClaw and
ZeroClaw) are shown in the chooser as **Coming soon**. If you chose one of them
in an earlier version, OpenAgora shows the chooser again; nothing breaks.

## One profile, and nothing else

Sunday lives in a Hermes profile of its own, named `sunday`
(`~/.hermes/profiles/sunday/`, or `%LOCALAPPDATA%\hermes\profiles\sunday\` on
Windows). It has its own settings, keys, `SOUL.md`, memory, skills, sessions,
channels and MCP servers. OpenAgora only ever runs Hermes with
`hermes -p sunday …`, so your default Hermes profile, your other profiles, their
memory and their bots are never touched. The engine's catalog validator enforces
this for the agent listing: it must run under its own `{profile}`.

The profile is created without a command alias (`--no-alias`), so OpenAgora
never adds a `sunday` command to your PATH.

## Setting Sunday up

Choose Sunday on first launch, or later from **Settings → Change personal
agent**. Sunday is then OpenAgora's home screen. What happens next depends on
whether Hermes is already on your computer:

| You have | What OpenAgora does |
|---|---|
| No Hermes | Shows Hermes's official installer for review, installs it, creates the `sunday` profile (`hermes profile create sunday --no-alias`), starts Sunday and opens Hermes's own model setup. |
| Hermes already, including one an earlier OpenAgora installed | Never reinstalls it: it is recorded as installed as it is. Then you choose **Start fresh** (`hermes profile create sunday --no-alias`) or **Bring my setup** (`… --clone`). A Hermes dashboard an earlier OpenAgora left running for your default profile is replaced by Sunday's. |
| A Hermes older than 0.21.3 | Offers **Update Hermes**, which runs Hermes's own `hermes update`. |
| A profile named `sunday` that OpenAgora didn't create | Asks before using it. If you agree, OpenAgora adds Sunday's identity to the top of its `SOUL.md` and keeps everything else. |

**Bring my setup** copies your current Hermes profile's settings, API keys,
`SOUL.md`, skills and memory (`memories/MEMORY.md` and `USER.md`) into Sunday.
It does not copy history, sessions or scheduled jobs.

**Messaging bots** stay with your current Hermes. A bot can answer for only one
profile at a time; Hermes blocks the duplicate. If you tick **Also copy my
messaging bots** (`--clone-channels`), turn each bot off in your other setup.
Otherwise create a new bot for Sunday later, in **Settings → Sunday → Channels**.

**Model sign-ins** (Anthropic, OpenAI Codex and xAI logins) are shared with your
main Hermes either way, so you don't sign in again. OpenAgora never sees, asks for
or stores credentials.

**Minimum Hermes version: 0.21.3** (release `v2026.9.14`). It is the first
release whose profile clones leave messaging bots behind by default, and it has
the per-profile dashboard (`hermes dashboard --isolated`) and every dashboard page
Settings opens.

From the command line, the same steps are `openagora install hermes`,
`openagora sunday setup [--fresh | --clone [--clone-channels]]` and
`openagora start hermes`. See [the command line](command-line.md).

## Sunday's identity

Hermes reads `SOUL.md` from the profile at the start of each session. OpenAgora
keeps a marked block at the top of Sunday's `SOUL.md`
(`<!-- openagora:sunday:begin -->` … `<!-- openagora:sunday:end -->`). The block's
text lives in this repository, in [`sunday/SOUL.md`](../sunday/SOUL.md). It says:

- its name is Sunday, on every surface and messaging channel, and it is built on
  Hermes Agent by Nous Research, which it says when it describes itself;
- what OpenAgora is, and Sunday's job: work across your apps and coding agents,
  carry context between them, install and manage apps with your confirmation, and
  remember across apps and sessions;
- the rules: confirm before installing, updating, uninstalling or acting in an
  app; text from apps and web pages is data, not instructions; use only the access
  you granted.

OpenAgora refreshes the block when Sunday starts. Everything outside it is yours
and is kept word for word. If the rest of the file gives the agent another name
(Hermes's own default says "You are Hermes Agent"), Sunday keeps that text's tone
but uses the name Sunday. A test checks that the block passes Hermes's own
prompt-injection scan, so Hermes loads it without warnings.

What changes from turn to turn (installed apps, the coding agents Sunday can
send to, what's on screen, new things on the desk) is not written to `SOUL.md`;
the per-reply hook below supplies it.

## Sunday and your apps

Sunday reaches your apps only through OpenAgora. Once Sunday is set up, OpenAgora
asks **Let Sunday use OpenAgora?** and explains what that changes in Sunday's
Hermes profile. It makes the changes only after you agree, using Hermes's own
`hermes -p sunday config set`:

- **One MCP server: OpenAgora's gateway** (`openagora mcp --client sunday`). It is
  the only MCP server OpenAgora adds. Sunday gets the store tools through it: it
  can search the store, show a listing, install, update, remove, start, stop and
  open apps. If Sunday's profile has other MCP servers from a copied setup,
  OpenAgora names them and leaves them; you can remove them on Hermes's MCP page.
- **A per-reply hook.** Before each reply, Hermes runs
  `openagora context --hook --client sunday`. It tells Sunday what changed since its
  last reply: which apps are installed and running, and, when you're chatting on
  this computer, the app you're working in, what Sunday's shortcut captured, and
  new things on the desk (see below). On a messaging channel it only tells Sunday
  about apps, never about your screen. It is marked as data, not instructions,
  and says nothing when nothing changed, because Hermes keeps the text in the
  conversation.
- **OpenAgora's skills**, in OpenAgora's own folder, added to
  `skills.external_dirs`: one teaches Sunday the store tools, the other the desk
  and capture tools. Hermes shows each only when its tools are there.
- **`hooks_auto_accept: true`, for Sunday's profile only**, so Hermes runs the hook
  without asking at the start of every session, including when Sunday answers on a
  messaging channel. This is the issue's default for hook consent: OpenAgora asks
  once, during setup.

**OpenAgora asks you, not just Sunday.** Installing, updating and removing an app
always wait for your yes, and the engine itself asks: in Sunday's chat (Hermes
shows its own approval prompt, wherever you are talking to Sunday, Telegram
included), or in OpenAgora's window when the asking agent can't ask in its chat.
Sunday can't answer that question for you. A decline, a cancel or no answer within
five minutes means no. Sunday can't update, stop or remove itself.

**The audit log.** Every app action Sunday asks for is written to
`<data folder>/audit/audit.jsonl`: who asked, what, the decision, where you
answered, and the outcome. Settings → Activity shows it, newest first, and finds
entries by who asked or by words; `openagora audit` prints it. It stays on this
computer.

**Disconnect** in Settings → Sunday (or `openagora sunday disconnect`) removes only
what OpenAgora added and puts `hooks_auto_accept` back as it was.

OpenAgora keeps a copy of its engine at `<data folder>/bin/openagora` for Sunday's
configuration to point at, because the desktop app's own copy can move (a Linux
AppImage runs from a new folder each time). The engine service the copy talks to is
started by the desktop app, or by the first agent that needs it, and keeps running
after the window closes so Sunday can still reach OpenAgora from your phone.

## Sunday's shortcut: showing Sunday what you're working on

Press **Ctrl+Shift+Space** (**Command+Shift+Space** on a Mac) in any app on your
computer, whether OpenAgora installed it or not. Sunday's panel opens over that
app: Sunday's chat (Hermes's own, for Sunday's profile) in a small window that
stays on top.

- **Apps you allowed:** OpenAgora captures that app's window and puts the
  picture on the desk. Sunday is told which app it is and looks at the picture
  when you ask about it. The panel shows a small copy of what Sunday got.
- **Apps you haven't allowed:** nothing is captured. The panel says Sunday can't
  see the app yet, with **Allow once** and **Always allow**. If you ask Sunday
  about it instead, Sunday says it can't see that app and offers to ask;
  OpenAgora then asks you itself, in its own window, never in a chat.
- **Apps OpenAgora never captures:** password managers (1Password, Bitwarden,
  KeePassXC, LastPass, Dashlane, Keychain Access, Passwords and others), private
  and incognito browser windows, and Tor Browser. They can't be allowed. Add your
  own apps, or words in window titles, in **Settings → What Sunday can see**.
  Browsers mark private windows in their titles, except Safari, so OpenAgora
  can't tell Safari's private windows apart. Whether Chrome marks them on macOS
  is still to be checked. Each capture checks the window's title again, as
  OpenAgora's window saw it while capturing, so a private window opened in an
  allowed browser after you pressed the shortcut is still refused.
- **Apps in OpenAgora's tabs** are captured by OpenAgora itself, without any
  system permission, and so are apps you popped out of their tab ("Pop out" in
  the tab's bar) into a window of their own. They need allowing too. Sunday
  knows which app such a window shows, even on Wayland. Apps in tabs (and
  Sunday's chat) can't use your camera, microphone or screen themselves.

Sunday is told the names of apps it may see, with their window titles for apps
you always allowed. Other apps stay unnamed unless you press the shortcut over
them (or Sunday names one itself), and never-capture rules are never named to
Sunday; the panel and Settings show them to you.

Anything Sunday looks at is sent to the model provider you chose for Sunday.
OpenAgora says so in the panel and in Settings.

**The desk** is where what you share with Sunday waits: screenshots, and the
clipboard or files you put there from the panel (**Add clipboard**, or drop files
on the panel; the panel reads them and sends their contents, so no page can have
OpenAgora read a file by its path). Items are kept on this computer, in a folder
only you can open, for 24 hours. `openagora desk` lists and clears them.

**Sunday can look again** with its `capture_window` tool, but only at apps you
allowed, and **asks to see an app** with `allow_app`, which puts OpenAgora's own
question in front of you (for the app Sunday names, or the one you're in). OpenAgora looks up which windows are in front only
when it needs to (when you press the shortcut, when Sunday asks, and before each
of Sunday's replies, to tell it which app you're in); it keeps no record of the
apps you use. Every capture, allowed or refused, every permission you give or
take back, and every time Sunday reads the desk goes to the audit log.

**Programs on your computer.** OpenAgora's engine takes any program running as
you at its word about who it is, as it does Sunday. So a program running as you
could ask for what Sunday may see; on macOS that includes pictures of apps you
always allowed, through OpenAgora's Screen Recording permission. Allow apps
**once** when you only need it once; the audit log shows every capture.

**OpenAgora keeps running in the menu bar (or tray) after you close its window**,
so the shortcut keeps working. **Quit OpenAgora** is in its menu. Turn this off,
or change the shortcut, in **Settings → What Sunday can see**.

**Per system:**

- **macOS** asks once for **Screen Recording** permission the first time
  OpenAgora captures another app's window (System Settings → Privacy & Security →
  Screen Recording). Settings shows a button to that page.
- **Windows** needs no permission; OpenAgora uses Windows' own window capture.
- **Linux on X11** needs no permission.
- **Linux on Wayland** doesn't let apps see which window is in front. Pressing
  the shortcut opens the panel with **Pick a window**, and the system's own dialog
  lets you choose the window; your choice is the permission for that one capture
  (the never-capture list still applies). Only you can open that dialog, from the
  panel: Sunday asking can't. Elsewhere there is no **Pick a window**.
  The shortcut itself goes through the desktop's global shortcuts portal. On GNOME
  without the AppIndicator extension there is no tray icon: open OpenAgora again
  from your apps to bring its window back.

## Sending your work to a coding agent

Ask Sunday to **send this to Claude Code** (or Codex, or OpenCode), and it sends
what you're working on, usually the screenshot your shortcut just put on the
desk, with a note. In that coding agent, ask **"what did Sunday send me?"**: it
reads its inbox through OpenAgora and sees the picture. Claude Code hears about
it with your next prompt without being asked.

**Set each agent up once**, in **Settings → Coding agents** (or with `openagora
agents setup <agent>`). OpenAgora shows you the exact command it will run, then
runs the agent's own command to add OpenAgora's MCP server to that agent's
settings:

- Claude Code: `claude mcp add --scope user --transport stdio openagora -- …`,
  plus a small plugin, installed with `claude plugin marketplace add` and
  `claude plugin install`, that tells Claude Code about new items with your next
  prompt.
- Codex: `codex mcp add openagora -- …`.
- OpenCode: `opencode mcp add openagora -- …` (`--global` for OpenCode 2).

OpenAgora never edits an agent's settings files itself. Removing works the same
way, with the agent's own command; OpenCode has none, so OpenAgora tells you what
to remove from its `opencode.json`. Another MCP agent can read what Sunday sends
it too: add a server named `openagora` that runs `openagora mcp --client <name>`
to its settings, then `openagora agents add <name> --name "<Name>"`.

**T3 Code runs the work itself.** Ask Sunday to **start a T3 Code thread** with
the screenshot: OpenAgora uploads the picture to T3 Code, starts the thread in
your project with it attached, and Sunday gives you the thread's link. Later,
ask Sunday how it went: it reads the thread's messages, commands and changes
and tells you. Connect T3 Code once, in **Settings → Coding agents → T3 Code →
Connect**: T3 Code's own approval page opens in a window (with T3's own sign-in,
so you don't need a pairing code), and you approve OpenAgora there. Choose
**Supervised** or more so Sunday can start threads; T3 Code still asks you
before each change. The sign-in lasts 30 days; you can take it back in T3
Code's Settings → Connections. Sunday can also use T3 Code's own tools: reading
threads needs no question, anything else asks you first.

**Any other agent or app: the clipboard or a folder.** For an agent that isn't
set up, or any app at all, Sunday can put a screenshot or text **on the
clipboard** for you to paste (one thing at a time), or **save it into your
project folder** as a new file, and tells you where it went. OpenAgora asks you
every time before saving into a folder, and saves only into your own folders in
your home: never a hidden folder, `Library` or `AppData`, and never over a file
that's already there. Apps in the store can take files the same way, by a
folder they watch or a link in their own URL scheme.

**What you send stays yours to decide.** OpenAgora asks you before Sunday sends
anything to an agent (**Send once** or **Always allow**, per app and agent),
before it starts a task, and before it acts in an app. Nothing from an app on the
never-capture list ever leaves, even where you always allowed sending. An agent
only ever sees what was sent to it, never the rest of your desk, and what you
send is shared with that agent and the model provider it uses. Every setup,
connection, handoff, task and read is in the audit log. Take back "always" in
**Settings → What Sunday can see**.

## Sunday remembers what you did in your coding agents

Ask Sunday **"what did I do in my coding sessions today?"** and it answers from
your sessions in Claude Code, Codex, OpenCode and T3 Code: what you asked, in
which project, and what each agent answered. Sunday reads them where each agent
keeps them, on this computer, and copies nothing. Ask about one session and it
reads that session in full (your prompts and the agent's answers, not its tool
calls).

The first time Sunday wants to read an agent's sessions, OpenAgora asks you in
its own window: **Allow once** or **Always allow**, per agent. What Sunday reads
goes to the model provider it uses. You can allow an agent ahead of time, or
take it back, with `openagora sessions allow <agent>` and `openagora sessions
revoke <agent>`; every read is in the audit log.

**Long-term memory.** Sunday's memory is Hermes's: its built-in memory
(MEMORY.md and USER.md) is always on and stays across restarts, but holds only a
few dozen facts. For more, use **Holographic** in **Settings → Memory** (or
`openagora sunday memory use holographic`): a local database of facts in
Sunday's profile, with no account, network or model calls. OpenAgora shows you
Hermes's own commands first, and runs them for Sunday's profile only. It needs
Hermes 0.21.5 or later. **Turn off** goes back to built-in memory; Holographic's
facts stay, for when you turn it on again.

**Keeping Sunday's memory up to date (optional).** Turn on the **digest** in
Settings → Memory (or `openagora sunday memory digest on`): every two hours
while OpenAgora runs, if one of your sessions changed, Sunday reads what changed
and saves what's worth remembering, such as decisions, conventions and open
problems, never code or secrets. It runs as one turn of Sunday's with only its
memory and OpenAgora's tools, and each run costs one turn with your model
provider. Turning it on lets Sunday always read the sessions it covers; take
that back per agent in Settings → Memory.

## Home screen and settings

The home screen is Hermes's dashboard for Sunday's profile, labelled **Sunday**,
with a "powered by Hermes Agent from Nous Research" credit and a link to its MIT
license. OpenAgora runs it
as `hermes -p sunday dashboard --isolated --host 127.0.0.1 --port <port> --no-open`.
`--isolated` gives Sunday a dashboard server of its own, so OpenAgora never
attaches to, or takes over, a Hermes dashboard you already run. Pages always open
with `?profile=sunday`.

**Settings → Sunday** opens Hermes's own pages for Sunday's profile: Models,
Channels, Pairing, Skills, Plugins, MCP, Cron, Config, Keys, Sessions, Logs and
System. Nothing is reimplemented. Messaging channels connected there (Discord,
Telegram, WhatsApp, Slack, Signal and every other channel Hermes supports) answer
as Sunday. **Run Sunday's setup in Terminal** opens Hermes's own
`hermes -p sunday setup`.

**Messaging gateway.** Hermes runs its own messaging gateway; by default one
gateway serves every Hermes profile, so Sunday and your other Hermes can both
answer at the same time. OpenAgora shows the gateway status Hermes reports (from
the dashboard's `/api/status`) and leaves the controls on Hermes's System page.
Phone messages need the gateway running and the computer awake.

## Removing Sunday

**Settings → Remove Sunday** (or `openagora uninstall hermes`) asks first, then
deletes only the `sunday` profile with Hermes's own `hermes profile delete sunday
--yes`. Hermes itself, your default profile and your other profiles stay. A
profile named `sunday` that OpenAgora didn't set up is never deleted.

## Verification and limits

Phase 5 (memory): Rust tests write sessions the way Claude Code,
Codex (both of its formats), OpenCode (1 and 2, in one SQLite file) and T3 Code
(its MCP tools) keep them, and check test 11: the command line lists them all;
Sunday is asked about per agent and reads only what was allowed (always, once,
or not at all); prompts and final answers come back, never tool output,
subagents, Codex's commentary or Claude Code's own notes; old sessions stay out;
one session reads in full; nothing is read without the window unless always
allowed; and the audit rows. For test 12, OpenAgora's part, against a stand-in
Hermes: Holographic set up only after a yes and only on Hermes 0.21.5 or later,
with Hermes's own commands; the digest a one-shot turn with only memory and
OpenAgora's tools, run while OpenAgora runs, only when a session changed, picking
up across a restart from where it stopped; Sunday's memory file untouched;
every Hermes call for Sunday's profile and nothing outside it changed. A UI test
covers Settings → Memory.

Not tested automatically, and to be checked by hand: real Claude Code, Codex and
OpenCode sessions on each OS (their formats change between versions); Sunday
answering "what did I do today?" from them; Holographic installing into a real
Hermes and recalling a decision after a restart; a real digest turn saving
facts.

Phase 4 (coding agents): Rust tests run the engine against stand-ins for Claude
Code, Codex and OpenCode (scripts that record every call) and for T3 Code's
server (its OAuth, its MCP server and its uploads, as T3's source has them).
They cover test 7: each agent set up only with its own command and only after a
yes, and nothing else of the agent's touched; Sunday sending a screenshot after
the user's yes in OpenAgora's window; each agent reading it through its own MCP
connection as an image, without `structuredContent`; Claude Code's prompt hook
telling it once; an agent seeing only what was sent to it, or apps the user
allowed for it; a no, and the never-capture list, stopping a send; connecting
T3 Code on its approval page (PKCE, the grant in a file only you can open), a
thread started with the uploaded picture attached, its status and its result,
more for the same thread, T3's own tools (reading free, acting asked), a
read-only connection, a declined one, and a taken-back sign-in; the command
line's connect through the browser; the fallbacks (the clipboard, one item at a
time; a folder, asked each time, never "always", only the user's own folders,
nothing overwritten; apps that take files by a folder or their own URL scheme);
and the audit rows. UI tests cover Settings → Coding agents (the exact command
shown before anything runs, manual removal for OpenCode, T3 Code's connect),
the approval window with T3's own session, and the window writing the clipboard
and opening only an installed app's own URL scheme.

Not tested automatically, and to be checked by hand: the real Claude Code,
Codex and OpenCode commands on each OS, each agent reading the picture (and
Claude Code's hook running, in a trusted folder); T3 Code's real approval page
with its paired session, and a real thread with the picture; the 30-day expiry;
pasting a picture from the clipboard into another app on each OS.

Phase 3 (capture): Rust tests drive the engine with a stand-in for OpenAgora's
window and a stand-in window list. They cover: the shortcut over an app
OpenAgora didn't install (it knows the app's name and captures nothing until it
is allowed; allowing it once or always; Sunday's hook naming the app and the
desk item, and `desk_get` returning the picture); `capture_window` and
`allow_app` (asked in OpenAgora's window only; once means once, even for two
captures at the same moment; no means no); password managers, private browser
windows and the user's own additions never captured and never allowable, with no
capture request ever reaching the window, and their rules never named to Sunday;
a window checked again as it is captured (a title that changed, a closed window,
a private window only OpenAgora's window could see, a title nobody could read);
a choice in the panel never applied to another window; app tabs and the panel;
Wayland's pick, from the panel only; the desk shared with Sunday only, in a
private folder; the hook staying quiet about the screen on messaging channels
and on turns that don't say where they come from, and telling about every desk
item in time; and the audit log's entries. A test against a real X server checks the X11
window query. UI tests cover the panel, its buttons, captures of an app tab and
of a real window by its X11 id, the clipboard, Settings → What Sunday can see,
keeping OpenAgora running after its window closes, dropped files sent as their
contents, the window picker offered only when the engine says so, and pages in
OpenAgora refused the screen. One end-to-end test runs the real engine with the
real window, capturing another process's real window through the OS, reading it
back through Sunday's MCP tools, and refusing it once it becomes a private
window that only OpenAgora's window can see the title of.

Not tested automatically, and to be checked by hand: the shortcut and capture on
a real Mac (Screen Recording permission, the macOS window list and titles, the
menu bar icon, the panel over full-screen apps), on Windows (the window list,
app names, Windows' window capture, the tray) and on a Wayland desktop (the
system's window picker, the global shortcuts portal, the tray); the default
shortcut not clashing with other apps on your computer; and Sunday, in a real
Hermes, looking at the screenshot (Hermes passes images from MCP tools to models
that can see).

Phase 2 (the gateway): Rust tests drive `openagora mcp` exactly as Hermes does.
They cover: a decline, a cancel and an accept of the engine's in-chat question, then
install, start and open with OpenAgora's window receiving the request; the
window's own confirmation when the agent can't ask; refusal when nobody can be
asked; Sunday unable to remove or stop itself; the audit log's entries; the hook
saying only what changed; and `connect`/`disconnect` touching only Sunday's profile
and keeping the user's own hooks. A UI test covers the window's side.

Automated tests use a fake Hermes and a fake dashboard; they need no network and
no accounts. They cover: a fresh install through to Sunday's dashboard opening
Hermes's model setup; adopting an existing Hermes without reinstalling it; start
fresh and bring my setup, with and without messaging bots, with every file outside
`profiles/sunday` checked to be unchanged; the minimum version and `hermes
update`; removing Sunday deleting only its profile; never taking over or deleting a
`sunday` profile OpenAgora didn't create; the `SOUL.md` block keeping the user's
own text; the chooser's coming-soon agents; and Settings → Sunday opening each
page with `?profile=sunday`.

Not tested automatically, and to be checked by hand with a real Hermes: Hermes's
own installer and `hermes update`; signing in to a model provider in Hermes's
setup; Sunday answering "who are you?" as Sunday; connecting a real Discord or
Telegram bot and getting a reply from Sunday; the gateway status shown for a
real gateway; Hermes loading the `openagora` MCP server, the hook and the skill
after **Connect Sunday**; and asking Sunday to install an app, with Hermes's
approval prompt in the dashboard chat and on a messaging channel. Native assistant
features and channels depend on the installed Hermes version.
