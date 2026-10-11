# OpenAgora

An open-source app store for your desktop.

- **Find open-source apps** in one store, and install them with one click from
  their official source.
- **Use them side by side** in one window. Each app keeps its own interface.
- **Meet Sunday**, OpenAgora's personal agent. Sunday runs on the open-source
  [Hermes](https://github.com/NousResearch/hermes-agent) agent, in a Hermes
  profile of its own, so a Hermes you already use is left exactly as it is.
  Sunday's home screen, setup and settings are Hermes's own; OpenAgora never sees
  your keys or sign-ins. Sunday can find, install and open apps from the store for
  you, and OpenAgora asks you first every time. Press **Ctrl+Shift+Space**
  (**Command+Shift+Space** on a Mac) in any app to call Sunday over it; Sunday sees
  that app's window only if you allowed the app, and never sees password managers
  or private browser windows. Carrying your work to coding agents is coming next.
  Other personal agents are coming soon. See [Sunday](docs/personal-agents.md).

## Download

Version 0.3.2. All downloads are on the [release page](https://github.com/sanjaykatta1/openagora/releases/latest).

| Your computer | Download |
|---|---|
| Mac with Apple Silicon (M1 or later) | [OpenAgora-0.3.2-mac-arm64.dmg](https://github.com/sanjaykatta1/openagora/releases/download/v0.3.2/OpenAgora-0.3.2-mac-arm64.dmg) |
| Mac with Intel | [OpenAgora-0.3.2-mac-x64.dmg](https://github.com/sanjaykatta1/openagora/releases/download/v0.3.2/OpenAgora-0.3.2-mac-x64.dmg) |
| Windows | [OpenAgora-0.3.2-win-x64.exe](https://github.com/sanjaykatta1/openagora/releases/download/v0.3.2/OpenAgora-0.3.2-win-x64.exe) |
| Linux (any distribution) | [OpenAgora-0.3.2-linux-x86_64.AppImage](https://github.com/sanjaykatta1/openagora/releases/download/v0.3.2/OpenAgora-0.3.2-linux-x86_64.AppImage) |
| Debian / Ubuntu | [OpenAgora-0.3.2-linux-amd64.deb](https://github.com/sanjaykatta1/openagora/releases/download/v0.3.2/OpenAgora-0.3.2-linux-amd64.deb) |

Not sure which Mac you have? Apple menu → About This Mac: "Chip: Apple M…" means
Apple Silicon; "Processor: Intel" means Intel.

The Mac app is signed and notarized by Apple. The Windows installer isn't signed
yet: if Windows shows "Windows protected your PC", choose **More info → Run
anyway**.

You only download OpenAgora once. When a new version is out, an **Update**
button appears in the app's sidebar.

Prefer a terminal? See [OpenAgora on the command line](docs/command-line.md).

## Add an app to the store

Every app in the store is one file, `catalog/apps/<id>/app.toml`. Copy an
existing one, describe how the app installs from its official source, check it
with `openagora --catalog catalog validate`, and open a pull request.

## Build from source

```sh
cd engine && cargo test && cargo build    # the engine
cd ../desktop && npm install && npm start  # the desktop app
```

Design notes are in [docs/SPEC.md](docs/SPEC.md). Packaging and releases are in
[packaging/README.md](packaging/README.md).

## License

MIT. Each app in the store keeps its own license, shown on its page.
