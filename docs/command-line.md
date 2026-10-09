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
openagora uninstall handy    # stops it and removes its folder
```

Apps live in their own folder under `~/Library/Application Support/OpenAgora`
(macOS), `~/.local/share/openagora` (Linux) or `%APPDATA%\OpenAgora`
(Windows). Set `OPENAGORA_HOME` to use another location.

The catalog is built into the binary. `--catalog <dir>` reads a local copy
instead.

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
