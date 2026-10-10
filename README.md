# OpenAgora

An open-source app store for your desktop.

- **Find open-source apps** in one store, and install them with one click from
  their official source.
- **Use them side by side** in one window. Each app keeps its own interface.
- **Choose your personal agent** on first launch, or choose later. Change your
  preference anytime from **Personal agent** in the sidebar. Connections across
  apps are coming next.

The next desktop build offers Hermes, OpenClaw, Goose, nanobot, NanoClaw,
PicoClaw, and ZeroClaw. Hermes has a Store installer; the other choices link to
their official setup guides. Choosing an agent saves a preference on this
computer; it doesn't install software or grant access to apps. The existing
downloads below do not yet include this chooser.

## Download

Version 0.3.0. All downloads are on the [release page](https://github.com/sanjaykatta1/openagora/releases/latest).

| Your computer | Download |
|---|---|
| Mac with Apple Silicon (M1 or later) | [OpenAgora-0.3.0-mac-arm64.dmg](https://github.com/sanjaykatta1/openagora/releases/download/v0.3.0/OpenAgora-0.3.0-mac-arm64.dmg) |
| Mac with Intel | [OpenAgora-0.3.0-mac-x64.dmg](https://github.com/sanjaykatta1/openagora/releases/download/v0.3.0/OpenAgora-0.3.0-mac-x64.dmg) |
| Windows | [OpenAgora-0.3.0-win-x64.exe](https://github.com/sanjaykatta1/openagora/releases/download/v0.3.0/OpenAgora-0.3.0-win-x64.exe) |
| Linux (any distribution) | [OpenAgora-0.3.0-linux-x86_64.AppImage](https://github.com/sanjaykatta1/openagora/releases/download/v0.3.0/OpenAgora-0.3.0-linux-x86_64.AppImage) |
| Debian / Ubuntu | [OpenAgora-0.3.0-linux-amd64.deb](https://github.com/sanjaykatta1/openagora/releases/download/v0.3.0/OpenAgora-0.3.0-linux-amd64.deb) |

Not sure which Mac you have? Apple menu → About This Mac: "Chip: Apple M…" means
Apple Silicon; "Processor: Intel" means Intel.

The Mac app is signed and notarized by Apple. The Windows installer isn't signed
yet: if Windows shows "Windows protected your PC", choose **More info → Run
anyway**.

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
