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
- The `openagora` CLI: browse the catalog, and install, start, stop and remove
  apps, with installers for macOS, Linux and Windows.

Next up is the agent: Hermes driving these commands from chat. The full design and roadmap are in
[docs/SPEC.md](docs/SPEC.md).

## Install

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

Then:

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

## Build from source

```sh
cd engine
cargo test
cargo install --path .
```

## Release

Push a tag such as `v0.1.0`, or run the **release** workflow from the Actions
tab and enter the version. It builds macOS (Apple Silicon
and Intel), Linux (x64 and ARM, static) and Windows binaries and publishes them
with checksums, which is what the installers download.

## Add an app

1. Create `catalog/apps/<id>/app.toml`; the existing listings are the template.
2. Run `openagora --catalog catalog validate` from the repo root.
3. Open a pull request.

Listings must install from the app's official source and declare what the
agent may do with the app.

## License

MIT. Each app in the catalog has its own license, shown in its listing.
