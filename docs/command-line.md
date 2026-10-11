# OpenAgora on the command line

The `openagora` command is the engine inside the desktop app. Install it on its
own if you prefer a terminal or want to script it.

## Native packages

Choose your CPU architecture on the
[latest release page](https://github.com/sanjaykatta1/openagora/releases/latest).
These install for all users. Each download has an adjacent `.sha256` checksum file.
Download links follow `https://github.com/sanjaykatta1/openagora/releases/download/v<version>/<filename>`.

| Platform | Download filename | Install |
|---|---|---|
| macOS, Apple Silicon + Intel | `OpenAgora-CLI-<version>.pkg` (universal, signed and notarized on official builds) | Double-click the `.pkg` |
| Windows x64 | `OpenAgora-CLI-<version>-x64.msi` | Double-click the `.msi` |
| Debian / Ubuntu | `openagora_<version>-1_<amd64 or arm64>.deb` | `sudo apt install ./openagora_*.deb` |
| Fedora / RHEL / openSUSE | `openagora-<version>-1.<x86_64 or aarch64>.rpm` | `sudo dnf install ./openagora-*.rpm` (openSUSE: `sudo zypper install ./openagora-*.rpm`) |
| Arch / derivatives | `openagora-<version>-1-<x86_64 or aarch64>.pkg.tar.zst` | `sudo pacman -U openagora-*.pkg.tar.zst` |
| Alpine | `openagora-<version>-r1.<x86_64 or aarch64>.apk` | `sudo apk add --allow-untrusted openagora-*.apk` |
| Other Linux | `openagora-<x86_64 or aarch64>-unknown-linux-musl.tar.gz` | Use the one-line installer below |

Keep just the intended package in the directory when using these wildcard
commands. The Windows installer is **unsigned for now**: if SmartScreen appears,
choose **More info → Run anyway** after checking you downloaded the official
release. It adds `C:\Program Files\OpenAgora` to the system PATH; open a new
terminal afterwards. macOS installs to `/usr/local/bin/openagora`; Linux packages
install to `/usr/bin/openagora`.

## One-line installer (per user)

macOS and Linux:

```sh
curl -fsSL https://raw.githubusercontent.com/sanjaykatta1/openagora/main/install.sh | sh
```

Windows (PowerShell):

```powershell
irm https://raw.githubusercontent.com/sanjaykatta1/openagora/main/install.ps1 | iex
```

The installer downloads the `openagora` binary for your system from the latest
[release](https://github.com/sanjaykatta1/openagora/releases), checks its SHA-256
checksum, and puts it in `~/.local/bin` (`%LOCALAPPDATA%\OpenAgora\bin` on
Windows). Set `OPENAGORA_VERSION=v0.1.0` to pin a version.

## Using it

```sh
openagora catalog            # browse the store
openagora catalog clipboard  # search
openagora show handy         # what installing it involves on this machine
openagora install handy      # shows the steps and asks before doing anything
openagora start handy        # runs it in the background
openagora ps                 # installed apps and whether they're running
openagora logs handy         # the app's recent output
openagora stop handy
openagora update handy       # runs the app's own update command, after asking
openagora uninstall handy    # stops it and removes its folder
openagora adopt t3code       # an app that's already on this computer: use it as it is
```

`adopt` records an app you installed yourself, without running its installer.
Removing an adopted app only makes OpenAgora forget it; its own uninstaller is
not run.

Apps live in their own folder under `~/Library/Application Support/OpenAgora`
(macOS), `~/.local/share/openagora` (Linux) or `%APPDATA%\OpenAgora`
(Windows). Set `OPENAGORA_HOME` to use another location.

The catalog is built into the binary. `--catalog <dir>` reads a local copy
instead.

## Sunday

Sunday, OpenAgora's personal agent, is Hermes running under its own `sunday`
profile. See [Sunday](personal-agents.md).

```sh
openagora install hermes      # official installer; a Hermes already here is used as it is
openagora sunday status       # Hermes, Sunday's profile, its dashboard and gateway
openagora sunday setup        # create the `sunday` profile and Sunday's SOUL.md
openagora start hermes        # Hermes's dashboard for Sunday's profile
openagora uninstall hermes    # remove Sunday: deletes only the `sunday` profile
```

`openagora sunday setup` options:

- `--fresh`: a new, empty setup. This is the default when OpenAgora installed
  Hermes itself.
- `--clone`: bring your current Hermes setup (settings, keys, `SOUL.md`, skills
  and memory; not history). Required, or `--fresh`, when you already used Hermes.
- `--clone-channels` (with `--clone`): also copy your messaging bots. A bot can
  answer for only one profile at a time.
- `--use-existing`: use a profile named `sunday` that OpenAgora didn't create.

Sunday needs Hermes 0.21.3 or later; `openagora update hermes` runs Hermes's own
`hermes update`. `openagora sunday status --json` prints the same information as
JSON for scripts and the desktop app.

### Letting Sunday use OpenAgora

```sh
openagora sunday connect      # asks first, then configures Sunday's profile
openagora sunday disconnect   # removes only what connect added
```

`connect` uses Hermes's own `hermes -p sunday config set` to add four things to
Sunday's profile, and nothing else:

- `mcp_servers.openagora`: OpenAgora's MCP gateway, run as
  `<data folder>/bin/openagora mcp --client sunday` (a copy of the engine at a
  fixed place, because the desktop app's own copy can move);
- a `pre_llm_call` hook, `openagora context --hook --client sunday`, that tells
  Sunday what changed in OpenAgora since its last reply (your own hooks are kept);
- OpenAgora's skills folder in `skills.external_dirs`;
- `hooks_auto_accept: true`, for Sunday's profile only, so Hermes runs the hook
  without asking each session (also when Sunday answers on a messaging channel).

If Sunday's profile already had other MCP servers (from a copied setup), `connect`
names them and leaves them alone.

## The engine service

The desktop app starts the engine as a service; `openagora mcp` starts it too
when needed. It listens on a local socket only this user can reach
(`<data folder>/run/engine.sock`; a named pipe on Windows).

```sh
openagora daemon start        # start it in the background (no-op if running)
openagora daemon status       # pid and address
openagora daemon stop
openagora serve               # run it in the foreground instead
openagora mcp --client sunday # the MCP server on stdin/stdout, for an agent's config
openagora context             # what the per-reply hook would tell Sunday now
openagora audit               # every app action, who asked and who confirmed
```

Through the MCP gateway, Sunday gets the store tools: `store_search`,
`store_show`, `app_install`, `app_update`, `app_uninstall`, `app_start`,
`app_stop`, `app_status` and `app_open`. Install, update and uninstall always
wait for your confirmation, which the engine asks for itself: in Sunday's chat
(Hermes shows its own approval prompt, wherever the conversation is), or in
OpenAgora's window when the asking agent can't ask in chat. If neither can ask,
nothing happens. Sunday can't update, stop or remove itself this way.

## Uninstall the command line tool

- macOS `.pkg`: `sudo rm /usr/local/bin/openagora && sudo pkgutil --forget app.openagora.cli`
- Windows `.msi`: **Settings → Apps → Installed apps → OpenAgora → Uninstall**.
- Debian / Ubuntu: `sudo apt remove openagora`
- Fedora / RHEL: `sudo dnf remove openagora`; openSUSE: `sudo zypper remove openagora`
- Arch: `sudo pacman -R openagora` (use `openagora-bin` if installed from the AUR recipe)
- Alpine: `sudo apk del openagora`
- macOS / Linux one-line installer: `rm ~/.local/bin/openagora`
- Windows one-line installer: remove `%LOCALAPPDATA%\OpenAgora\bin\openagora.exe`
  and remove that directory from your **user** PATH in Environment Variables.
