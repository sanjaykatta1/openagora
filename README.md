# OpenAgora

An open-source app store for your desktop.

- **Find open-source apps** in one store, and install them with one click from
  their official source.
- **Use them side by side** in one window, or pop an app out into a window of
  its own. Each app keeps its own interface.
- **Meet Sunday**, OpenAgora's personal agent. Sunday is
  [Hermes Agent](https://github.com/NousResearch/hermes-agent), the open-source
  agent by [Nous Research](https://nousresearch.com), running in a Hermes
  profile of its own, so a Hermes you already use is left exactly as it is.
  Sunday's home screen, setup and settings are Hermes's own; OpenAgora never sees
  your keys or sign-ins.
  - **Apps:** Sunday finds, installs and opens apps from the store for you, and
    OpenAgora asks you first every time.
  - **What you're looking at:** press **Ctrl+Shift+Space**
    (**Command+Shift+Space** on a Mac) in any app to call Sunday over it. Sunday
    sees that app's window, or reads its text, only if you allowed the app, and
    never sees password managers or private browser windows.
  - **Your coding agents:** ask Sunday to send what you're working on to Claude
    Code, Codex or OpenCode, or to start a T3 Code thread or an OpenCode session.
    Each agent is set up once, with its own command.
  - **Memory:** ask "what did I do in my coding sessions today?" and Sunday
    answers from those agents' own records, after asking you. Give it long-term
    memory on your computer with Holographic.
  - **Background mode**, off unless you turn it on: Sunday can search what your
    allowed apps showed over the last days, all kept on your computer.
  - **Activity:** Settings → Activity shows everything Sunday and your agents did.

  Other personal agents are coming soon. See [Sunday](docs/personal-agents.md).

## Download

Version 0.4.0, with Sunday across your apps and coding agents and a new app icon.
All downloads are on the [release page](https://github.com/sanjaykatta1/openagora/releases/latest).

| Your computer | Download |
|---|---|
| Mac with Apple Silicon (M1 or later) | [OpenAgora-0.4.0-mac-arm64.dmg](https://github.com/sanjaykatta1/openagora/releases/download/v0.4.0/OpenAgora-0.4.0-mac-arm64.dmg) |
| Windows | [OpenAgora-0.4.0-win-x64.exe](https://github.com/sanjaykatta1/openagora/releases/download/v0.4.0/OpenAgora-0.4.0-win-x64.exe) |
| Linux (any distribution) | [OpenAgora-0.4.0-linux-x86_64.AppImage](https://github.com/sanjaykatta1/openagora/releases/download/v0.4.0/OpenAgora-0.4.0-linux-x86_64.AppImage) |
| Debian / Ubuntu | [OpenAgora-0.4.0-linux-amd64.deb](https://github.com/sanjaykatta1/openagora/releases/download/v0.4.0/OpenAgora-0.4.0-linux-amd64.deb) |

The Mac app needs Apple Silicon (M1 or later): Apple menu → About This Mac says
"Chip: Apple M…". On a Mac with an Intel processor, use
[OpenAgora on the command line](docs/command-line.md), or the Linux app if you
run Linux on it.

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

## Credits

Sunday is [Hermes Agent](https://github.com/NousResearch/hermes-agent) by
[Nous Research](https://nousresearch.com), under the MIT license. OpenAgora gives
it Sunday's name and a place on your desktop. The agent itself, with its
dashboard, setup, memory, skills and messaging, is Hermes's work.

## License

MIT. Each app in the store keeps its own license, shown on its page.
