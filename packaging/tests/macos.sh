#!/usr/bin/env bash
set -euo pipefail
pkg=${1:?Usage: macos.sh package.pkg true|false}
signed=${2:?Specify whether this package must be signed}
notarized=${3:-$signed}
[[ ! -e /usr/local/bin/openagora ]] || { echo 'Refusing to overwrite an existing local installation' >&2; exit 1; }
if pkgutil --pkg-info app.openagora.cli >/dev/null 2>&1; then
  echo 'Refusing to overwrite an existing OpenAgora receipt' >&2; exit 1
fi
if [[ "$signed" == true ]]; then
  pkgutil --check-signature "$pkg"
fi
if [[ "$notarized" == true ]]; then
  spctl --assess --type install -vv "$pkg"
  xcrun stapler validate "$pkg"
fi
cleanup() {
  sudo rm -f /usr/local/bin/openagora
  sudo pkgutil --forget app.openagora.cli >/dev/null 2>&1 || true
}
trap cleanup EXIT
sudo installer -pkg "$pkg" -target /
archs=$(lipo -archs /usr/local/bin/openagora)
[[ " $archs " == *" arm64 "* && " $archs " == *" x86_64 "* ]] || { echo "Missing universal architecture" >&2; exit 1; }
lipo -info /usr/local/bin/openagora
/usr/local/bin/openagora --version
expected=$(python3 "$(dirname "$0")/../version.py")
[[ "$(/usr/local/bin/openagora --version)" == "openagora $expected" ]]
/usr/local/bin/openagora catalog
if [[ "$signed" == true ]]; then
  codesign --verify --strict --verbose=2 /usr/local/bin/openagora
fi
sudo rm /usr/local/bin/openagora
sudo pkgutil --forget app.openagora.cli
[[ ! -e /usr/local/bin/openagora ]]
if pkgutil --pkg-info app.openagora.cli >/dev/null 2>&1; then
  echo 'Package receipt survived uninstall' >&2; exit 1
fi
trap - EXIT
