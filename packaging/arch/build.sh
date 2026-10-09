#!/usr/bin/env bash
# Run as an ordinary user on Arch Linux / Arch Linux ARM; does not publish to AUR.
set -euo pipefail
cd "$(dirname "$0")"
command -v makepkg >/dev/null || { echo 'Run on Arch with base-devel installed' >&2; exit 1; }
makepkg --verifysource
makepkg --cleanbuild --force
for package in openagora-bin-*.pkg.tar.zst; do
  sha256sum "$package" > "$package.sha256"
done
