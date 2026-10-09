#!/bin/sh
# Installs the `openagora` command on macOS or Linux:
#
#   curl -fsSL https://raw.githubusercontent.com/sanjaykatta1/openagora/main/install.sh | sh
#
# Settings (environment variables):
#   OPENAGORA_VERSION      release tag to install, e.g. v0.1.0 (default: latest)
#   OPENAGORA_INSTALL_DIR  where to put the binary (default: ~/.local/bin)
#   OPENAGORA_BASE_URL     download from this URL instead of GitHub releases
set -eu

REPO="sanjaykatta1/openagora"

say() { printf 'openagora: %s\n' "$*"; }
fail() { printf 'openagora: error: %s\n' "$*" >&2; exit 1; }

# Everything runs inside main, so a download cut off halfway executes nothing.
main() {
  version="${OPENAGORA_VERSION:-latest}"
  install_dir="${OPENAGORA_INSTALL_DIR:-$HOME/.local/bin}"

  command -v curl >/dev/null 2>&1 || fail "curl is required"
  command -v tar >/dev/null 2>&1 || fail "tar is required"

  case "$(uname -s)" in
    Darwin) os="apple-darwin" ;;
    Linux) os="unknown-linux-musl" ;;
    *) fail "unsupported system $(uname -s); on Windows use install.ps1" ;;
  esac
  case "$(uname -m)" in
    x86_64 | amd64) arch="x86_64" ;;
    arm64 | aarch64) arch="aarch64" ;;
    *) fail "unsupported CPU $(uname -m)" ;;
  esac
  # A shell running under Rosetta on Apple Silicon reports x86_64; install the
  # native build anyway.
  if [ "$os" = "apple-darwin" ] && [ "$arch" = "x86_64" ] &&
    [ "$(sysctl -n sysctl.proc_translated 2>/dev/null || echo 0)" = "1" ]; then
    arch="aarch64"
  fi

  asset="openagora-$arch-$os.tar.gz"
  if [ -n "${OPENAGORA_BASE_URL:-}" ]; then
    base="$OPENAGORA_BASE_URL"
  elif [ "$version" = "latest" ]; then
    base="https://github.com/$REPO/releases/latest/download"
  else
    base="https://github.com/$REPO/releases/download/$version"
  fi

  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT INT TERM

  say "downloading $asset ($version)"
  curl -fsSL "$base/$asset" -o "$tmp/$asset" || fail "could not download $base/$asset"
  curl -fsSL "$base/$asset.sha256" -o "$tmp/$asset.sha256" || fail "could not download the checksum"

  expected="$(cut -d ' ' -f 1 <"$tmp/$asset.sha256")"
  if command -v sha256sum >/dev/null 2>&1; then
    actual="$(sha256sum "$tmp/$asset" | cut -d ' ' -f 1)"
  else
    actual="$(shasum -a 256 "$tmp/$asset" | cut -d ' ' -f 1)"
  fi
  [ "$expected" = "$actual" ] || fail "checksum mismatch for $asset; nothing was installed"

  tar -xzf "$tmp/$asset" -C "$tmp"
  mkdir -p "$install_dir"
  # Copy then rename, so a running openagora is never left half-written.
  cp "$tmp/openagora" "$install_dir/.openagora.new"
  chmod 755 "$install_dir/.openagora.new"
  mv "$install_dir/.openagora.new" "$install_dir/openagora"

  say "installed $("$install_dir/openagora" --version) at $install_dir/openagora"
  case ":$PATH:" in
    *":$install_dir:"*) ;;
    *)
      say "$install_dir is not on your PATH. Add it with:"
      say "  echo 'export PATH=\"$install_dir:\$PATH\"' >> ~/.zshrc   # or ~/.bashrc"
      say "then open a new terminal."
      ;;
  esac
  say "try: openagora catalog"
}

main "$@"
