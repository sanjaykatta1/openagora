#!/usr/bin/env bash
# The official archlinux Docker image is x86_64-only. Build the native ARM
# test container from Arch Linux ARM's signed generic root filesystem.
set -euo pipefail
image=${1:-openagora-archlinuxarm:test}
work=$(mktemp -d "${TMPDIR:-/tmp}/openagora-archlinuxarm.XXXXXX")
trap 'rm -rf "$work"' EXIT
archive=ArchLinuxARM-aarch64-latest.tar.gz
mirror=https://ca.us.mirror.archlinuxarm.org/os
# Published at https://archlinuxarm.org/about/downloads and /about/package-signing.
fingerprint=68B3537F39A313B3E574D06777193F152BDBE6A6
for tool in curl gpg gpgv docker; do command -v "$tool" >/dev/null; done
curl --fail --silent --show-error --location --retry 3 --proto '=https' \
  "$mirror/$archive" -o "$work/$archive"
curl --fail --silent --show-error --location --retry 3 --proto '=https' \
  "$mirror/$archive.sig" -o "$work/$archive.sig"
curl --fail --silent --show-error --location --retry 3 --proto '=https' \
  "https://keyserver.ubuntu.com/pks/lookup?op=get&search=0x$fingerprint" -o "$work/key.asc"
mkdir -m 700 "$work/gnupg"
gpg --batch --homedir "$work/gnupg" --import "$work/key.asc"
# Export only the expected public key; never trust an arbitrary key returned
# by the keyserver. gpgv rejects unsigned or incorrectly signed downloads.
gpg --batch --homedir "$work/gnupg" --export "$fingerprint" > "$work/rootfs-key.gpg"
gpgv --homedir "$work/gnupg" --keyring "$work/rootfs-key.gpg" \
  "$work/$archive.sig" "$work/$archive"
docker import --platform linux/arm64 "$work/$archive" "$image"
