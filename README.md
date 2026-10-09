# OpenAgora

An open-source app store for your desktop. Install open-source apps, use them
side by side in one window, and let one agent work across all of them.

- **One window, real apps.** Each app keeps its own interface, in a tab you can
  switch to, detach into its own window, or split side by side.
- **One agent across everything.** [Hermes](https://github.com/NousResearch/hermes-agent)
  is built in. Ask it to "install T3 Code", check on your threads, or
  remember a decision, and it works across every app you've installed.
- **Open catalog.** Each app is one `app.toml` file. Anyone can add an app by
  pull request.
- **macOS, Linux and Windows** from one codebase.

An *agora* was the open marketplace of a Greek city, and Hermes was the god of
markets and messengers.

## Status

Early. What exists today:

- The listing format and its validator ([`engine/src/manifest.rs`](engine/src/manifest.rs)).
- Four listings: [T3 Code](catalog/apps/t3code/app.toml),
  [Hermes](catalog/apps/hermes/app.toml), [Maccy](catalog/apps/maccy/app.toml)
  (clipboard history) and [Handy](catalog/apps/handy/app.toml) (speech-to-text).
- The desktop app ([`desktop/`](desktop)): a Store to search and browse apps,
  app pages showing what installing does, one-click install with a
  confirmation, a Library to start and stop apps, and web apps opening in tabs
  inside the window.
- The `openagora` engine underneath it, also usable on its own from a terminal.

Next up: Hermes in the window, driving all of this from chat. The full design and roadmap are in
[docs/SPEC.md](docs/SPEC.md).

## Install

### Desktop app

Download the installer for your system from the
[latest desktop build](https://github.com/sanjaykatta1/openagora/actions/workflows/desktop.yml)
(open the newest run, then "Artifacts"):

- macOS: `OpenAgora-<version>-mac-arm64.dmg` (Apple Silicon) or `-mac-x64.dmg` (Intel)
- Windows: `OpenAgora-<version>-win-x64.exe`
- Linux: `OpenAgora-<version>-linux-x86_64.AppImage` or `-linux-amd64.deb`

Official macOS builds use Developer ID signing and notarization. PR builds are
signed when credentials are available, but skip notarization; fork builds are
unsigned. For these development builds, macOS may require right-click → Open,
or System Settings → Privacy & Security → Open Anyway. Windows is unsigned:
choose "More info" → "Run anyway" after verifying the download source.

### Command line

The `openagora` command is the engine inside the desktop app. Install it on its
own if you prefer a terminal or want to script it.

#### Native packages

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

#### One-line installer (per user)

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

#### Using it

```sh
openagora catalog            # browse the store
openagora catalog clipboard  # search
openagora show handy         # what installing it involves on this machine
openagora install handy      # shows the steps and asks before doing anything
openagora start handy        # runs it in the background
openagora ps                 # installed apps and whether they're running
openagora logs handy         # the app's recent output
openagora stop handy
openagora uninstall handy    # stops it and removes its folder
```

Apps live in their own folder under `~/Library/Application Support/OpenAgora`
(macOS), `~/.local/share/openagora` (Linux) or `%APPDATA%\OpenAgora`
(Windows). Set `OPENAGORA_HOME` to use another location.

The catalog is built into the binary. `--catalog <dir>` reads a local copy
instead.

#### Uninstall the command line tool

- macOS `.pkg`: `sudo rm /usr/local/bin/openagora && sudo pkgutil --forget app.openagora.cli`
- Windows `.msi`: **Settings → Apps → Installed apps → OpenAgora → Uninstall**.
- Debian / Ubuntu: `sudo apt remove openagora`
- Fedora / RHEL: `sudo dnf remove openagora`; openSUSE: `sudo zypper remove openagora`
- Arch: `sudo pacman -R openagora` (use `openagora-bin` if installed from the AUR recipe)
- Alpine: `sudo apk del openagora`
- macOS / Linux one-line installer: `rm ~/.local/bin/openagora`
- Windows one-line installer: remove `%LOCALAPPDATA%\OpenAgora\bin\openagora.exe`
  and remove that directory from your **user** PATH in Environment Variables.

## Build from source

```sh
cd engine && cargo test && cargo build    # the engine
cd ../desktop && npm install && npm start  # the desktop app (uses the engine build)
```

`OPENAGORA_CATALOG=<dir>` makes the desktop app read a local catalog folder,
handy when writing a listing.

## Release

Push a tag such as `v0.1.0`, or run the **release** workflow from the Actions
tab and enter the version. It builds macOS (Apple Silicon
and Intel), Linux (x64 and ARM, static) and Windows binaries and publishes them
with checksums, which is what the one-line installers download. It also builds
native packages and runs install, command and uninstall checks before publishing.
Pull requests build and test packages without creating a release or tag.
See [packaging/README.md](packaging/README.md) for local builds, signing secrets,
and the manual steps to publish the included `openagora-bin` AUR recipe.

## Add an app

1. Create `catalog/apps/<id>/app.toml`; the existing listings are the template.
2. Run `openagora --catalog catalog validate` from the repo root.
3. Open a pull request.

Listings must install from the app's official source and declare what the
agent may do with the app.

## License

MIT. Each app in the catalog has its own license, shown in its listing.
