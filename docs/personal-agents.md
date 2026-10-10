# Personal agents in OpenAgora 0.3.2

OpenAgora combines existing apps. It does not implement a new personal agent or
replace an assistant's onboarding, permissions, model login, messaging setup,
pairing, or service manager.

Choose an agent on first launch or from **Personal agent**. Choosing later and
changing the choice both persist across app restarts. Switching does not stop
other agents, move their memory, or migrate accounts.

| Agent | Install | Open and configure |
|---|---|---|
| [Hermes](https://hermes-agent.nousresearch.com/docs/) | Store installer | Its original dashboard opens inside OpenAgora. Its own CLI setup is also available. |
| [OpenClaw](https://docs.openclaw.ai/start/getting-started) | Official installer | Shortcuts to `openclaw onboard` and `openclaw dashboard`; the dashboard opens in the browser. |
| [Goose](https://goose-docs.ai/docs/quickstart/) | Official desktop/CLI installer | Use Goose Desktop directly, or shortcuts to `goose configure` and `goose session` when the CLI is installed. |
| [nanobot](https://github.com/HKUDS/nanobot) | Official installer | `nanobot webui` opens the native browser UI, including first-run setup. |
| [NanoClaw](https://github.com/nanocoai/nanoclaw) | Project checkout and container runtime | Follow its own setup and use its interface. No automatic checkout discovery or service management. |
| [PicoClaw](https://github.com/sipeed/picoclaw) | Official installer | Configure in its own launcher/guide; the shortcut opens `picoclaw agent` for native CLI chat. |
| [ZeroClaw](https://github.com/zeroclaw-labs/zeroclaw) | Official installer | Shortcut to its `zeroclaw quickstart`; chat and services stay in ZeroClaw. |

Hermes installation does not run invisible interactive prompts. After installation,
OpenAgora returns to the Personal agent page with **Start and open Hermes**. Its
dashboard includes its own model settings, channels, pairing, and gateway controls.
Use those original screens for Telegram, WhatsApp, or whichever other channels
your installed Hermes version supports. The optional terminal shortcut runs the
original `hermes setup` command; OpenAgora never reads its prompts or answers.

Phone messaging requires the assistant's gateway/service to be running, not just
a saved channel credential. Check connection status in the assistant itself and
use its own service instructions for background operation. A sleeping/offline
computer or a stopped gateway can prevent replies. OpenAgora's app status only
describes a process it launched, not provider authentication or phone connectivity.
Stopping Hermes's dashboard does not stop a separately managed messaging service.

## Native shortcuts

The desktop searches normal executable locations, including `~/.local/bin`,
`~/.cargo/bin`, Homebrew, Windows Hermes, and npm's Windows user bin directory.
Use **Refresh** after installing elsewhere. Desktop-only installs may not expose
a CLI; in that case use the project's app directly. NanoClaw's project directory
is deliberately not guessed.

Shortcuts open a real terminal so upstream OAuth, key prompts, QR codes, and
interactive menus remain usable. Only an allowlisted agent/action pair crosses
the IPC boundary; arbitrary shell text is rejected. Work starts in a separate
agent workspace under OpenAgora's user-data directory, not the OpenAgora checkout.
No credentials or terminal output are captured, and a launched command is never
reported as a configured/connected agent. Commands are displayed as a fallback if
the operating system has no supported terminal application.

## Verification and limits

Automated tests cover preference persistence, corrupt-data recovery, native-action
dispatch and errors, executable detection, shell quoting, installation handoff,
and the desktop chooser. CI builds the app on macOS, Windows, and Linux.

Provider login and real phone pairing are performed by each user in the original
assistant. The release does not claim a live Telegram/WhatsApp conversation was
tested with the user's accounts. Cross-app MCP connections, credential migration,
and unified agent memory are not included. Native assistant features and supported
channels depend on the installed upstream version.
